use crate::{browser, state::*};
use leptos::prelude::*;
use pinboard_core::*;
use wasm_bindgen::JsCast;

pub fn node_down(app: App, id: String, event: web_sys::PointerEvent, resize: bool) {
    event.stop_propagation();
    if event.button() != 0 || app.busy.get_untracked() {
        return;
    }
    if app.comment_mode.get_untracked() {
        app.selection.set(vec![id]);
        app.active_thread.set(String::new());
        app.panel.set(Panel::Comments);
        browser::focus_element("new-comment");
        return;
    }
    if !app.can_edit() {
        app.selection.set(vec![id]);
        app.panel.set(Panel::Inspector);
        return;
    }
    if app.tool.get_untracked() == Tool::Connect {
        let from = app.connect_from.get_untracked();
        if from.is_empty() {
            app.connect_from.set(id.clone());
            app.selection.set(vec![id]);
            app.status.set("Choose a second node to connect".into());
        } else if from != id {
            app.edit(vec![Operation::AddEdge {
                edge: Edge {
                    id: new_id(),
                    from,
                    to: id,
                    label: String::new(),
                    routing: Routing::Curved,
                    arrow: true,
                },
            }]);
            app.connect_from.set(String::new());
            app.tool.set(Tool::Select);
        }
        return;
    }
    if event.shift_key() || event.ctrl_key() || event.meta_key() {
        app.selection.update(|ids| {
            if ids.contains(&id) {
                ids.retain(|s| s != &id);
            } else {
                ids.push(id.clone());
            }
        });
        return;
    }
    if !app.selection.get_untracked().contains(&id) {
        app.selection.set(vec![id.clone()]);
    }
    app.selected_edge.set(String::new());
    app.panel.set(Panel::Inspector);
    app.following.set(String::new());
    let selected = app.selection.get_untracked();
    let nodes = app
        .board
        .get_untracked()
        .map(|c| {
            let selected = c.expanded_selection(&selected);
            c.nodes
                .into_iter()
                .filter(|n| {
                    if resize {
                        n.id == id
                    } else {
                        selected.contains(&n.id)
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let p = local(event.client_x() as f64, event.client_y() as f64);
    let v = app.viewport.get_untracked();
    app.drag.set(Some(Drag {
        kind: if resize {
            DragKind::Resize
        } else {
            DragKind::Move
        },
        world: world(&p, v),
        start: p.clone(),
        current: p,
        camera: v,
        nodes,
        points: vec![],
    }));
    browser::capture_pointer(event.pointer_id());
}
pub fn stage_down(app: App, event: web_sys::PointerEvent) {
    if event.button() != 0 && event.button() != 1 || app.busy.get_untracked() {
        return;
    }
    event.prevent_default();
    app.following.set(String::new());
    let p = local(event.client_x() as f64, event.client_y() as f64);
    let camera = app.viewport.get_untracked();
    let w = world(&p, camera);
    let tool = app.tool.get_untracked();
    let kind = match tool {
        Tool::Note => Some(NodeKind::Note),
        Tool::Step => Some(NodeKind::Step),
        Tool::Rectangle => Some(NodeKind::Rectangle),
        Tool::Rounded => Some(NodeKind::RoundedRectangle),
        Tool::Oval => Some(NodeKind::Oval),
        Tool::Decision => Some(NodeKind::Decision),
        Tool::Text => Some(NodeKind::Text),
        Tool::Frame => Some(NodeKind::Frame),
        _ => None,
    };
    if let Some(kind) = kind {
        app.add(kind, w.x - 110., w.y - 80.);
        return;
    }
    let kind = if event.button() == 1 || tool == Tool::Pan {
        DragKind::Pan
    } else if tool == Tool::Line {
        DragKind::Line
    } else if tool == Tool::Ink {
        DragKind::Ink
    } else {
        app.selected_edge.set(String::new());
        app.selection.set(vec![]);
        DragKind::Marquee
    };
    app.drag.set(Some(Drag {
        kind,
        start: p.clone(),
        current: p,
        world: w.clone(),
        camera,
        nodes: vec![],
        points: vec![w],
    }));
    browser::capture_pointer(event.pointer_id());
}
pub fn stage_move(app: App, event: web_sys::PointerEvent) {
    let p = local(event.client_x() as f64, event.client_y() as f64);
    app.drag.update(|drag| {
        if let Some(d) = drag {
            d.current = p.clone();
            if d.kind == DragKind::Pan {
                app.viewport.set(Viewport {
                    x: d.camera.x + p.x - d.start.x,
                    y: d.camera.y + p.y - d.start.y,
                    zoom: d.camera.zoom,
                });
            }
            if d.kind == DragKind::Line {
                d.points = vec![d.world.clone(), world(&p, d.camera)];
            }
            if d.kind == DragKind::Ink {
                let w = world(&p, d.camera);
                if d.points
                    .last()
                    .is_none_or(|last| (last.x - w.x).hypot(last.y - w.y) > 2. / d.camera.zoom)
                    && d.points.len() < 10000
                {
                    d.points.push(w);
                }
            }
        }
    });
}
pub fn stage_up(app: App) {
    let Some(d) = app.drag.get_untracked() else {
        return;
    };
    app.drag.set(None);
    let dx = (d.current.x - d.start.x) / d.camera.zoom;
    let dy = (d.current.y - d.start.y) / d.camera.zoom;
    match d.kind {
        DragKind::Move => {
            if dx.abs() + dy.abs() > 1. {
                app.edit(
                    d.nodes
                        .iter()
                        .map(|n| Operation::UpdateNode {
                            id: n.id.clone(),
                            changes: NodePatch {
                                x: Some(n.x + dx),
                                y: Some(n.y + dy),
                                ..Default::default()
                            },
                        })
                        .collect(),
                );
            }
        }
        DragKind::Resize => {
            if let Some(n) = d.nodes.first() {
                app.patch(
                    n.id.clone(),
                    NodePatch {
                        width: Some((n.width + dx).clamp(60., 2000.)),
                        height: Some((n.height + dy).clamp(60., 2000.)),
                        ..Default::default()
                    },
                );
            }
        }
        DragKind::Marquee => {
            let region = Region {
                x: d.world.x.min(d.world.x + dx),
                y: d.world.y.min(d.world.y + dy),
                width: dx.abs(),
                height: dy.abs(),
            };
            if let Some(c) = app.board.get_untracked() {
                app.selection.set(
                    c.nodes
                        .iter()
                        .filter(|n| n.intersects(&region))
                        .map(|n| n.id.clone())
                        .collect(),
                );
                if !app.selection.get_untracked().is_empty() {
                    app.panel.set(Panel::Inspector);
                }
            }
        }
        DragKind::Ink | DragKind::Line if d.points.len() >= 2 => {
            let minx = d.points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
            let miny = d.points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
            let maxx = d
                .points
                .iter()
                .map(|p| p.x)
                .fold(f64::NEG_INFINITY, f64::max);
            let maxy = d
                .points
                .iter()
                .map(|p| p.y)
                .fold(f64::NEG_INFINITY, f64::max);
            let mut n = Node::note("Freehand sketch", minx, miny);
            n.kind = if d.kind == DragKind::Line {
                NodeKind::Line
            } else {
                NodeKind::Sketch
            };
            n.width = (maxx - minx).max(20.);
            n.height = (maxy - miny).max(20.);
            n.points = d
                .points
                .iter()
                .map(|p| Point {
                    x: p.x - minx,
                    y: p.y - miny,
                })
                .collect();
            n.color = "paper".into();
            app.selection.set(vec![n.id.clone()]);
            app.panel.set(Panel::Inspector);
            app.edit(vec![Operation::AddNode { node: n }]);
        }
        _ => {}
    }
    app.refresh();
}
pub fn keydown(app: App, event: web_sys::KeyboardEvent) {
    if event.key() == "Escape" {
        browser::stop_voice();
        app.drag.set(None);
        app.tool.set(Tool::Select);
        app.connect_from.set(String::new());
        app.setup.set(false);
        app.share_open.set(false);
        app.settings_open.set(false);
        app.following.set(String::new());
    }
    let in_dialog = event
        .target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .is_some_and(|e| e.closest("[role=dialog]").ok().flatten().is_some());
    if in_dialog {
        return;
    }
    if event.key() == "F6" {
        event.prevent_default();
        let document = web_sys::window().and_then(|w| w.document());
        let active = document.as_ref().and_then(|d| d.active_element());
        let id = active.as_ref().map(|e| e.id()).unwrap_or_default();
        if let Some(node_id) = id.strip_prefix("node-")
            && app
                .board
                .get_untracked()
                .is_some_and(|c| c.nodes.iter().any(|n| n.id == node_id))
            && !app.selection.get_untracked().iter().any(|id| id == node_id)
        {
            app.selection.set(vec![node_id.into()]);
        }
        let in_format = active.as_ref().is_some_and(|e| {
            e.closest("#format-toolbar,#canvas-toolbar")
                .ok()
                .flatten()
                .is_some()
        });
        let toolbar = if app.selection.get_untracked().is_empty() || !app.can_edit() {
            "canvas-toolbar"
        } else {
            "format-toolbar"
        };
        browser::focus_element(if in_format {
            if app.can_edit() {
                "board-title"
            } else {
                "account-settings"
            }
        } else if id == "stage" || id.starts_with("node-") {
            toolbar
        } else {
            "stage"
        });
        return;
    }
    if hierarchy(app, &event) {
        return;
    }
    let typing = event
        .target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .is_some_and(|e| {
            ["INPUT", "TEXTAREA", "SELECT"].contains(&e.tag_name().as_str())
                || e.get_attribute("contenteditable").is_some()
        });
    if typing || event.is_composing() {
        return;
    }
    let command = event.ctrl_key() || event.meta_key();
    if command {
        match event.key().as_str() {
            "+" | "=" => {
                event.prevent_default();
                app.zoom(1.15);
            }
            "-" => {
                event.prevent_default();
                app.zoom(1. / 1.15);
            }
            "0" => {
                event.prevent_default();
                app.fit();
            }
            "z" | "Z" => {
                event.prevent_default();
                app.undo_last();
            }
            "d" | "D" => {
                event.prevent_default();
                app.duplicate();
            }
            "a" | "A" => {
                event.prevent_default();
                app.selection.set(
                    app.board
                        .get_untracked()
                        .map(|c| c.nodes.into_iter().map(|n| n.id).collect())
                        .unwrap_or_default(),
                );
            }
            _ => {}
        }
        return;
    }
    match event.key().as_str() {
        "v" | "V" => app.tool.set(Tool::Select),
        "h" | "H" => app.tool.set(Tool::Pan),
        "n" | "N" => app.tool.set(Tool::Note),
        "r" | "R" => {
            let p = app.center();
            app.add(
                if event.shift_key() {
                    NodeKind::RoundedRectangle
                } else {
                    NodeKind::Rectangle
                },
                p.x - 110.,
                p.y - 80.,
            );
        }
        "o" | "O" => {
            let p = app.center();
            app.add(NodeKind::Oval, p.x - 110., p.y - 80.);
        }
        "d" | "D" => {
            let p = app.center();
            app.add(NodeKind::Decision, p.x - 110., p.y - 80.);
        }
        "t" | "T" => app.tool.set(Tool::Text),
        "p" | "P" => app.tool.set(Tool::Ink),
        "l" | "L" => app.tool.set(if event.shift_key() {
            Tool::Connect
        } else {
            Tool::Line
        }),
        "f" | "F" => app.tool.set(Tool::Frame),
        "Delete" | "Backspace" => {
            event.prevent_default();
            app.delete();
        }
        "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" => {
            if app.selection.get_untracked().is_empty() {
                return;
            }
            event.prevent_default();
            let step = if event.shift_key() { 1. } else { 10. };
            let (dx, dy) = match event.key().as_str() {
                "ArrowUp" => (0., -step),
                "ArrowDown" => (0., step),
                "ArrowLeft" => (-step, 0.),
                _ => (step, 0.),
            };
            let ids = app
                .board
                .get_untracked()
                .map(|c| c.expanded_selection(&app.selection.get_untracked()))
                .unwrap_or_default();
            app.edit(
                app.board
                    .get_untracked()
                    .map(|c| {
                        c.nodes
                            .into_iter()
                            .filter(|n| ids.contains(&n.id))
                            .map(|n| Operation::UpdateNode {
                                id: n.id,
                                changes: NodePatch {
                                    x: Some(n.x + dx),
                                    y: Some(n.y + dy),
                                    ..Default::default()
                                },
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            );
        }
        _ => {}
    }
}
pub fn path(points: &[Point]) -> String {
    points
        .iter()
        .enumerate()
        .map(|(i, p)| format!("{}{},{}", if i == 0 { "M" } else { " L" }, p.x, p.y))
        .collect()
}

fn hierarchy(app: App, event: &web_sys::KeyboardEvent) -> bool {
    let Some(element) = event
        .target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
    else {
        return false;
    };
    let Some(canvas) = app.board.get_untracked() else {
        return false;
    };
    if (event.key() == "Escape" || (event.key() == "Enter" && event.shift_key()))
        && let Some(thread) = element.closest(".comment-thread").ok().flatten()
    {
        event.prevent_default();
        let id = thread.get_attribute("data-thread-id").unwrap_or_default();
        if app
            .threads
            .get_untracked()
            .iter()
            .any(|t| t.id == id && !t.resolved)
        {
            browser::focus_element(&format!("pin-{id}"));
        } else {
            browser::focus_element("stage");
        }
        app.status
            .set("Returned from comment thread to canvas".into());
        return true;
    }
    let node_id = element.get_attribute("data-node-id");
    if event.key() == "Escape"
        && ["node-title", "node-body"].contains(&element.id().as_str())
        && let Some(id) = app.selection.get_untracked().first()
    {
        browser::focus_element(&format!("node-{id}"));
        event.prevent_default();
        return true;
    }
    let Some(id) = node_id else {
        return false;
    };
    let Some(node) = canvas.nodes.iter().find(|n| n.id == id) else {
        return false;
    };
    if event.key() == "Tab" {
        event.prevent_default();
        let siblings = canvas
            .nodes
            .iter()
            .filter(|n| n.parent_id == node.parent_id)
            .collect::<Vec<_>>();
        if let Some(index) = siblings.iter().position(|n| n.id == id) {
            let next = if event.shift_key() {
                (index + siblings.len() - 1) % siblings.len()
            } else {
                (index + 1) % siblings.len()
            };
            browser::focus_element(&format!("node-{}", siblings[next].id));
        }
        return true;
    }
    if event.key() == "Escape" || (event.key() == "Enter" && event.shift_key()) {
        event.prevent_default();
        if node.parent_id.is_empty() {
            app.focus_parent.set(String::new());
            browser::focus_element("stage");
            app.status.set("Canvas overview".into());
        } else if let Some(parent) = canvas.nodes.iter().find(|p| p.id == node.parent_id) {
            app.focus_parent.set(parent.parent_id.clone());
            browser::focus_element(&format!("node-{}", parent.id));
            app.status
                .set(format!("Left group. {} selected", parent.title));
        }
        return true;
    }
    if event.key() == "Enter" {
        event.prevent_default();
        if node.kind.is_container() {
            let children = canvas
                .nodes
                .iter()
                .filter(|n| n.parent_id == id)
                .collect::<Vec<_>>();
            app.focus_parent.set(id.clone());
            if let Some(child) = children.first() {
                browser::focus_element(&format!("node-{}", child.id));
            }
            app.status
                .set(format!("Entered {}. {} items", node.title, children.len()));
        } else {
            app.panel.set(Panel::Inspector);
            browser::focus_element("node-title");
            app.status.set(format!(
                "Editing {}. Escape returns to the canvas",
                node.title
            ));
        }
        return true;
    }
    false
}
