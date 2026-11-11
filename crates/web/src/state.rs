use crate::browser;
use gloo_net::http::Request;
use leptos::{prelude::*, task::spawn_local};
use pinboard_core::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, closure::Closure};
use wasm_bindgen_futures::JsFuture;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Pan,
    Note,
    Step,
    Rectangle,
    Rounded,
    Oval,
    Line,
    Decision,
    Text,
    Frame,
    Ink,
    Connect,
}
impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Pan => "Pan",
            Self::Note => "Sticky note",
            Self::Step => "Process step",
            Self::Rectangle => "Rectangle",
            Self::Rounded => "Rounded rectangle",
            Self::Oval => "Oval",
            Self::Line => "Line",
            Self::Decision => "Decision",
            Self::Text => "Text",
            Self::Frame => "Frame",
            Self::Ink => "Draw",
            Self::Connect => "Connect",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Ideas,
    Inspector,
    Ai,
    Comments,
}
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub height: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragKind {
    Move,
    Pan,
    Marquee,
    Ink,
    Line,
    Resize,
}
#[derive(Clone, Debug)]
pub struct Drag {
    pub kind: DragKind,
    pub start: Point,
    pub current: Point,
    pub world: Point,
    pub camera: Viewport,
    pub nodes: Vec<Node>,
    pub points: Vec<Point>,
}
#[derive(Clone, Debug)]
pub struct Undo {
    pub before: Canvas,
    pub after: Canvas,
}

#[derive(Clone, Copy)]
pub struct App {
    pub board: RwSignal<Option<Canvas>>,
    pub library: RwSignal<Vec<CanvasSummary>>,
    pub active: RwSignal<String>,
    pub selection: RwSignal<Vec<String>>,
    pub tool: RwSignal<Tool>,
    pub viewport: RwSignal<Viewport>,
    pub drag: RwSignal<Option<Drag>>,
    pub busy: RwSignal<bool>,
    pub status: RwSignal<String>,
    pub panel: RwSignal<Panel>,
    pub prompt: RwSignal<String>,
    pub thought: RwSignal<String>,
    pub setup: RwSignal<bool>,
    pub focus: RwSignal<bool>,
    pub peers: RwSignal<Vec<Peer>>,
    pub self_id: RwSignal<String>,
    pub following: RwSignal<String>,
    pub live: RwSignal<bool>,
    pub voice: RwSignal<bool>,
    pub voice_state: RwSignal<String>,
    pub undo: RwSignal<Vec<Undo>>,
    pub connect_from: RwSignal<String>,
    pub focus_parent: RwSignal<String>,
    pub selected_edge: RwSignal<String>,
    pub editing_node: RwSignal<String>,
    pub me: RwSignal<Option<Me>>,
    pub role: RwSignal<AccessRole>,
    pub sharing: RwSignal<Option<Sharing>>,
    pub share_open: RwSignal<bool>,
    pub settings_open: RwSignal<bool>,
    pub threads: RwSignal<Vec<CommentThread>>,
    pub comment_mode: RwSignal<bool>,
    pub active_thread: RwSignal<String>,
    pub panel_width: RwSignal<f64>,
    pub panel_drag: RwSignal<Option<(f64, f64)>>,
}
impl App {
    pub fn new() -> Self {
        Self {
            board: RwSignal::new(None),
            library: RwSignal::new(vec![]),
            active: RwSignal::new(String::new()),
            selection: RwSignal::new(vec![]),
            tool: RwSignal::new(Tool::Select),
            viewport: RwSignal::new(Viewport {
                x: 40.,
                y: 30.,
                zoom: 0.8,
            }),
            drag: RwSignal::new(None),
            busy: RwSignal::new(false),
            status: RwSignal::new("Opening your space…".into()),
            panel: RwSignal::new(Panel::Ideas),
            prompt: RwSignal::new(String::new()),
            thought: RwSignal::new(String::new()),
            setup: RwSignal::new(false),
            focus: RwSignal::new(false),
            peers: RwSignal::new(vec![]),
            self_id: RwSignal::new(String::new()),
            following: RwSignal::new(String::new()),
            live: RwSignal::new(false),
            voice: RwSignal::new(false),
            voice_state: RwSignal::new("idle".into()),
            undo: RwSignal::new(vec![]),
            connect_from: RwSignal::new(String::new()),
            focus_parent: RwSignal::new(String::new()),
            selected_edge: RwSignal::new(String::new()),
            editing_node: RwSignal::new(String::new()),
            me: RwSignal::new(None),
            role: RwSignal::new(AccessRole::Owner),
            sharing: RwSignal::new(None),
            share_open: RwSignal::new(false),
            settings_open: RwSignal::new(false),
            threads: RwSignal::new(vec![]),
            comment_mode: RwSignal::new(false),
            active_thread: RwSignal::new(String::new()),
            panel_width: RwSignal::new(296.),
            panel_drag: RwSignal::new(None),
        }
    }
    pub fn start(self) {
        let callback = Closure::<dyn FnMut()>::new(move || {
            if self.following.get_untracked().is_empty() {
                self.fit();
            } else if let Some(p) = self
                .peers
                .get_untracked()
                .iter()
                .find(|p| p.id == self.following.get_untracked())
            {
                self.follow_peer(p);
            }
        })
        .into_js_value();
        browser::observe_canvas(callback.unchecked_ref());
        spawn_local(async move {
            let me = match get::<Me>("/api/me").await {
                Ok(me) => me,
                Err(e) => {
                    self.status.set(e);
                    return;
                }
            };
            let signed_in = me.user.is_some();
            let enabled = me.policy.canvas_enabled;
            let can_create = me.user.as_ref().is_some_and(|u| u.internal || u.admin);
            self.me.set(Some(me));
            if !signed_in || !enabled {
                return;
            }
            let query = web_sys::window()
                .and_then(|w| w.location().search().ok())
                .and_then(|s| web_sys::UrlSearchParams::new_with_str(&s).ok());
            let invited = if let Some(token) = query.as_ref().and_then(|q| q.get("invite")) {
                match post::<serde_json::Value>(
                    "/api/invitations/accept",
                    &serde_json::json!({"token":token}),
                )
                .await
                {
                    Ok(value) => value["canvas_id"].as_str().map(str::to_string),
                    Err(e) => {
                        self.status.set(e);
                        return;
                    }
                }
            } else {
                None
            };
            match get::<Vec<CanvasSummary>>("/api/canvases").await {
                Ok(list) => {
                    let requested =
                        invited.or_else(|| query.as_ref().and_then(|q| q.get("canvas")));
                    let id = requested.or_else(|| list.first().map(|c| c.id.clone()));
                    self.library.set(list);
                    if let Some(id) = id {
                        self.open(id);
                    } else if can_create {
                        self.create("Untitled canvas".into());
                    } else {
                        self.status
                            .set("Open an invitation link to join a shared canvas".into());
                    }
                }
                Err(e) => self.status.set(e),
            }
        });
    }
    pub fn open(self, id: String) {
        if self.busy.get_untracked() {
            return;
        }
        browser::stop_voice();
        self.focus_parent.set(String::new());
        self.selected_edge.set(String::new());
        self.threads.set(vec![]);
        self.comment_mode.set(false);
        self.active_thread.set(String::new());
        self.active.set(id.clone());
        self.board.set(None);
        self.selection.set(vec![]);
        self.undo.set(vec![]);
        self.following.set(String::new());
        self.panel.set(Panel::Ideas);
        self.tool.set(Tool::Select);
        self.connect_from.set(String::new());
        self.status.set("Opening canvas…".into());
        spawn_local(async move {
            match get::<Canvas>(&format!("/api/canvases/{id}")).await {
                Ok(c) => {
                    if self.active.get_untracked() == id {
                        if let Ok(value) =
                            get::<serde_json::Value>(&format!("/api/canvases/{id}/access")).await
                            && let Ok(role) = serde_json::from_value(value["role"].clone())
                        {
                            self.role.set(role);
                        }
                        self.board.set(Some(c));
                        self.load_threads();
                        self.status.set("All changes saved".into());
                        browser::set_location(&id);
                        self.fit();
                        self.connect(id);
                    }
                }
                Err(e) => self.status.set(e),
            }
        });
    }
    pub fn connect(self, id: String) {
        let cb = Closure::<dyn FnMut(String)>::new(move |message: String| {
            let Ok(m) = serde_json::from_str::<serde_json::Value>(&message) else {
                return;
            };
            if m["canvas_id"]
                .as_str()
                .is_some_and(|id| id != self.active.get_untracked())
            {
                return;
            }
            match m["type"].as_str() {
                Some("connection") => {
                    let connected = m["connected"].as_bool().unwrap_or(false);
                    self.live.set(connected);
                    if !connected {
                        self.load_me();
                    }
                }
                Some("canvas") | Some("refresh") => {
                    if !self.busy.get_untracked() && self.drag.get_untracked().is_none() {
                        self.refresh();
                    }
                }
                Some("access") => {
                    if let Ok(role) = serde_json::from_value(m["role"].clone()) {
                        self.role.set(role);
                    }
                }
                Some("peers") => {
                    if let Some(id) = m["self_id"].as_str() {
                        self.self_id.set(id.into());
                    }
                    if let Ok(peers) = serde_json::from_value::<Vec<Peer>>(m["peers"].clone()) {
                        let follow = self.following.get_untracked();
                        if !follow.is_empty() {
                            if let Some(p) = peers.iter().find(|p| p.id == follow) {
                                self.follow_peer(p);
                            } else {
                                self.following.set(String::new());
                            }
                        }
                        self.peers.set(
                            peers
                                .into_iter()
                                .filter(|p| p.id != self.self_id.get_untracked())
                                .collect(),
                        );
                    }
                }
                _ => {}
            }
        })
        .into_js_value();
        browser::open_live(&id, cb.unchecked_ref());
    }
    pub fn refresh(self) {
        self.load_threads();
        let id = self.active.get_untracked();
        spawn_local(async move {
            if let Ok(c) = get::<Canvas>(&format!("/api/canvases/{id}")).await
                && self.active.get_untracked() == id
                && !self.busy.get_untracked()
                && self.drag.get_untracked().is_none()
                && self
                    .board
                    .get_untracked()
                    .is_none_or(|b| c.revision > b.revision)
            {
                self.selection
                    .update(|ids| ids.retain(|id| c.nodes.iter().any(|n| n.id == *id)));
                self.board.set(Some(c));
            }
            if let Ok(list) = get("/api/canvases").await {
                self.library.set(list);
            }
        });
    }
    pub fn create(self, title: String) {
        if self.me.get_untracked().is_some_and(|m| {
            !m.policy.canvas_enabled || m.user.is_none_or(|u| !u.internal && !u.admin)
        }) {
            self.status
                .set("Only workspace members can create canvases".into());
            return;
        }
        if self.busy.get_untracked() {
            return;
        }
        self.busy.set(true);
        spawn_local(async move {
            let result = post::<Canvas>("/api/canvases", &serde_json::json!({"title":title})).await;
            self.busy.set(false);
            match result {
                Ok(c) => {
                    self.library.update(|l| l.insert(0, c.summary()));
                    self.open(c.id);
                }
                Err(e) => self.status.set(e),
            }
        });
    }
    pub fn edit(self, operations: Vec<Operation>) {
        self.edit_inner(operations, true);
    }
    fn edit_inner(self, operations: Vec<Operation>, remember: bool) {
        if !self.can_edit() {
            self.status
                .set("Your role is read-only for canvas content".into());
            return;
        }
        if operations.is_empty() {
            return;
        }
        if self.busy.get_untracked() {
            self.status
                .set("Wait for the current change to save".into());
            return;
        }
        let Some(before) = self.board.get_untracked() else {
            return;
        };
        self.busy.set(true);
        self.status.set("Saving…".into());
        spawn_local(async move {
            let mutation = Mutation {
                base_revision: before.revision,
                scope: None,
                operations,
            };
            let result =
                post::<Canvas>(&format!("/api/canvases/{}/mutations", before.id), &mutation).await;
            self.busy.set(false);
            match result {
                Ok(c) => {
                    if self.active.get_untracked() == c.id {
                        if remember {
                            self.undo.update(|history| {
                                let (before, after) = compact_history(before, c.clone());
                                history.push(Undo { before, after });
                                if history.len() > 100 {
                                    history.remove(0);
                                }
                            });
                        } else {
                            self.undo.update(|history| {
                                history.pop();
                            });
                        }
                        self.selection
                            .update(|ids| ids.retain(|id| c.nodes.iter().any(|n| n.id == *id)));
                        self.board.set(Some(c.clone()));
                        self.library.update(|l| {
                            if let Some(s) = l.iter_mut().find(|s| s.id == c.id) {
                                *s = c.summary();
                            }
                        });
                        self.status.set("All changes saved".into());
                        self.refresh();
                    }
                }
                Err(e) => {
                    self.status.set(e);
                    self.refresh();
                }
            }
        });
    }
    pub fn patch(self, id: String, changes: NodePatch) {
        self.edit(vec![Operation::UpdateNode { id, changes }]);
    }
    pub fn selected(self) -> Option<Node> {
        let ids = self.selection.get();
        if ids.len() != 1 {
            return None;
        }
        self.board.get()?.nodes.into_iter().find(|n| n.id == ids[0])
    }
    pub fn delete(self) {
        let Some(c) = self.board.get_untracked() else {
            return;
        };
        let ids = self.selection.get_untracked();
        if !self.selected_edge.get_untracked().is_empty() {
            self.edit(vec![Operation::DeleteEdge {
                id: self.selected_edge.get_untracked(),
            }]);
            self.selected_edge.set(String::new());
            return;
        }
        let mut ops = vec![];
        for e in c
            .edges
            .iter()
            .filter(|e| ids.contains(&e.from) || ids.contains(&e.to))
        {
            ops.push(Operation::DeleteEdge { id: e.id.clone() });
        }
        for id in ids {
            ops.push(Operation::DeleteNode { id });
        }
        self.edit(ops);
    }
    pub fn duplicate(self) {
        let Some(c) = self.board.get_untracked() else {
            return;
        };
        let selected = c.expanded_selection(&self.selection.get_untracked());
        let mut map = std::collections::HashMap::new();
        let mut ops = vec![];
        for id in &selected {
            map.insert(id.clone(), new_id());
        }
        for n in c.nodes.iter().filter(|n| selected.contains(&n.id)) {
            let mut copy = n.clone();
            copy.id = map[&n.id].clone();
            if let Some(parent) = map.get(&n.parent_id) {
                copy.parent_id = parent.clone();
            }
            copy.x += 28.;
            copy.y += 28.;
            ops.push(Operation::AddNode { node: copy });
        }
        for e in &c.edges {
            if let (Some(a), Some(b)) = (map.get(&e.from), map.get(&e.to)) {
                ops.push(Operation::AddEdge {
                    edge: Edge {
                        id: new_id(),
                        from: a.clone(),
                        to: b.clone(),
                        label: e.label.clone(),
                        routing: e.routing.clone(),
                        arrow: e.arrow,
                    },
                });
            }
        }
        self.selection.set(
            self.selection
                .get_untracked()
                .iter()
                .filter_map(|id| map.get(id).cloned())
                .collect(),
        );
        self.edit(ops);
    }
    pub fn undo_last(self) {
        let (Some(last), Some(current)) = (
            self.undo.get_untracked().last().cloned(),
            self.board.get_untracked(),
        ) else {
            return;
        };
        match inverse_operations(&current, &last.before, &last.after) {
            Ok(ops) => self.edit_inner(ops, false),
            Err(e) => self.status.set(e.to_string()),
        }
    }
    pub fn add(self, kind: NodeKind, x: f64, y: f64) {
        if !self.can_edit() {
            self.status.set("Your canvas role is read-only".into());
            return;
        }
        let mut n = Node::note(
            match kind {
                NodeKind::Step => "New step",
                NodeKind::Decision => "A decision to make",
                NodeKind::Text => "Your text here",
                NodeKind::Frame => "New group",
                _ => "An idea worth keeping",
            },
            x,
            y,
        );
        n.kind = kind.clone();
        if kind == NodeKind::Step {
            n.color = "mint".into();
            n.height = 120.;
        }
        if kind.is_container() {
            n.width = 500.;
            n.height = 350.;
            n.color = "paper".into();
            n.z_index = self
                .board
                .get_untracked()
                .map(|c| c.nodes.iter().map(|n| n.z_index).min().unwrap_or(0) - 1)
                .unwrap_or(-1);
        }
        if kind == NodeKind::Swimlane {
            n.width = 800.;
            n.height = 220.;
        }
        if kind == NodeKind::Milestone {
            n.height = 100.;
            n.color = "sky".into();
        }
        let id = n.id.clone();
        self.selection.set(vec![n.id.clone()]);
        self.panel.set(Panel::Inspector);
        self.edit(vec![Operation::AddNode { node: n }]);
        self.tool.set(Tool::Select);
        leptos::task::spawn_local(async move {
            for _ in 0..40 {
                gloo_timers::future::TimeoutFuture::new(25).await;
                if !self.busy.get_untracked() {
                    break;
                }
            }
            if self.selection.get_untracked().contains(&id) {
                self.editing_node.set(id.clone());
                browser::focus_element(&format!("inline-{id}"));
            }
        });
    }
    pub fn capture(self) {
        if !self.can_edit() {
            self.status.set("Your canvas role is read-only".into());
            return;
        }
        let text = self.thought.get_untracked().trim().to_string();
        if text.is_empty() {
            return;
        }
        browser::stop_voice();
        let p = self.center();
        let mut n = Node::note(
            text.lines()
                .next()
                .unwrap_or("A new thought")
                .chars()
                .take(65)
                .collect::<String>(),
            p.x - 110.,
            p.y - 80.,
        );
        if text.len() > 65 || text.contains('\n') {
            n.body = text.clone();
        }
        self.selection.set(vec![n.id.clone()]);
        self.panel.set(Panel::Inspector);
        self.edit(vec![Operation::AddNode { node: n }]);
        self.thought.set(String::new());
    }
    pub fn copy(self, text: String) {
        spawn_local(async move {
            match JsFuture::from(browser::copy_text(&text)).await {
                Ok(_) => self.status.set("Copied to clipboard".into()),
                Err(_) => self
                    .status
                    .set("Clipboard access was blocked. Select and copy the text manually.".into()),
            }
        });
    }
    pub fn ai_request(self) -> String {
        let Some(c) = self.board.get_untracked() else {
            return String::new();
        };
        let ids = self.selection.get_untracked();
        format!(
            "Use the Pinboard MCP server. {}\nCanvas ID: {}\nLast seen revision: {}. Read the latest state before editing.\n{}\nPreserve all unrelated content. Show proposals as suggested nodes and ask before accepting them.",
            self.prompt.get_untracked(),
            c.id,
            c.revision,
            if ids.is_empty() {
                "Scope: whole canvas.".to_string()
            } else {
                format!(
                    "Scope: ONLY these node IDs: {}. Pass this exact scope array to edit_canvas. Do not modify anything outside it or add nodes in a scoped batch.",
                    serde_json::to_string(&ids).unwrap_or_default()
                )
            }
        )
    }
    pub fn voice_toggle(self) {
        let cb = Closure::<dyn FnMut(String)>::new(move |s: String| {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                self.voice.set(v["state"] == "listening");
                self.voice_state
                    .set(v["state"].as_str().unwrap_or("idle").into());
                if let Some(text) = v["text"].as_str() {
                    self.thought.set(text.into());
                }
                if let Some(message) = v["message"].as_str() {
                    self.status.set(message.into());
                }
            }
        })
        .into_js_value();
        browser::toggle_voice(cb.unchecked_ref(), &self.thought.get_untracked());
    }
    pub fn png(self) {
        let Some(c) = self.board.get_untracked() else {
            return;
        };
        spawn_local(async move {
            self.status.set("Preparing PNG…".into());
            match JsFuture::from(browser::export_png(
                &{let mut value=serde_json::to_value(&c).unwrap();value["paths"]=serde_json::Value::Array(c.edges.iter().filter_map(|e|{let a=c.nodes.iter().find(|n|n.id==e.from)?;let b=c.nodes.iter().find(|n|n.id==e.to)?;let(p,q)=connector_endpoints(a,b);let angle=if (q.x-p.x).abs()>=(q.y-p.y).abs()||e.routing==Routing::Orthogonal{if q.x>=p.x{0.}else{std::f64::consts::PI}}else if q.y>=p.y{std::f64::consts::FRAC_PI_2}else{-std::f64::consts::FRAC_PI_2};Some(serde_json::json!({"path":connector_path(a,b,&e.routing),"end":q,"start":p,"angle":angle,"arrow":e.arrow,"label":e.label}))}).collect());value.to_string()},
            ))
            .await
            {
                Ok(v) => self
                    .status
                    .set(v.as_string().unwrap_or_else(|| "PNG exported".into())),
                Err(_) => self.status.set("PNG export failed".into()),
            }
        });
    }
    pub fn follow_peer(self, p: &Peer) {
        let (w, h) = stage_size();
        self.viewport.set(Viewport {
            x: p.x + if p.width > 0. { (w - p.width) / 2. } else { 0. },
            y: p.y
                + if p.height > 0. {
                    (h - p.height) / 2.
                } else {
                    0.
                },
            zoom: p.zoom,
        });
    }
    pub fn layer(self, front: bool) {
        let Some(c) = self.board.get_untracked() else {
            return;
        };
        let ids = self.selection.get_untracked();
        let edge = if front {
            c.nodes.iter().map(|n| n.z_index).max().unwrap_or(0) + 1
        } else {
            c.nodes.iter().map(|n| n.z_index).min().unwrap_or(0) - ids.len() as i32
        };
        let mut selected = c
            .nodes
            .iter()
            .filter(|n| ids.contains(&n.id))
            .collect::<Vec<_>>();
        selected.sort_by_key(|n| n.z_index);
        self.edit(
            selected
                .into_iter()
                .enumerate()
                .map(|(i, n)| Operation::UpdateNode {
                    id: n.id.clone(),
                    changes: NodePatch {
                        z_index: Some(edge + i as i32),
                        ..Default::default()
                    },
                })
                .collect(),
        );
    }
    pub fn format(self, field: &str, value: &str) {
        let Some(c) = self.board.get_untracked() else {
            return;
        };
        let ids = self.selection.get_untracked();
        let selected = c
            .nodes
            .iter()
            .filter(|n| ids.contains(&n.id))
            .collect::<Vec<_>>();
        let all_bold = selected.iter().all(|n| n.text_style.bold);
        let all_italic = selected.iter().all(|n| n.text_style.italic);
        self.edit(
            selected
                .into_iter()
                .map(|n| {
                    let mut style = n.text_style.clone();
                    let mut changes = NodePatch::default();
                    match field {
                        "color" => changes.color = Some(value.into()),
                        "bold" => {
                            style.bold = !all_bold;
                            changes.text_style = Some(style);
                        }
                        "italic" => {
                            style.italic = !all_italic;
                            changes.text_style = Some(style);
                        }
                        "size" => {
                            if let Ok(size) = value.parse() {
                                style.font_size = size;
                                changes.text_style = Some(style);
                            }
                        }
                        "list" => {
                            style.list = match value {
                                "bullet" => ListStyle::Bullet,
                                "numbered" => ListStyle::Numbered,
                                _ => ListStyle::None,
                            };
                            changes.text_style = Some(style);
                        }
                        _ => {}
                    }
                    Operation::UpdateNode {
                        id: n.id.clone(),
                        changes,
                    }
                })
                .collect(),
        );
    }
    pub fn group(self) {
        let Some(c) = self.board.get_untracked() else {
            return;
        };
        let ids = self.selection.get_untracked();
        let nodes = c
            .nodes
            .iter()
            .filter(|n| ids.contains(&n.id))
            .collect::<Vec<_>>();
        if nodes.is_empty() {
            return;
        }
        let minx = nodes.iter().map(|n| n.x).fold(f64::INFINITY, f64::min) - 30.;
        let miny = nodes.iter().map(|n| n.y).fold(f64::INFINITY, f64::min) - 50.;
        let maxx = nodes
            .iter()
            .map(|n| n.x + n.width)
            .fold(f64::NEG_INFINITY, f64::max)
            + 30.;
        let maxy = nodes
            .iter()
            .map(|n| n.y + n.height)
            .fold(f64::NEG_INFINITY, f64::max)
            + 30.;
        let mut frame = Node::note("New group", minx, miny);
        frame.kind = NodeKind::Frame;
        frame.color = "paper".into();
        frame.width = maxx - minx;
        frame.height = maxy - miny;
        frame.z_index = nodes.iter().map(|n| n.z_index).min().unwrap_or(0) - 1;
        let id = frame.id.clone();
        let mut ops = vec![Operation::AddNode { node: frame }];
        for n in nodes {
            if !c.nodes.iter().any(|other| {
                ids.contains(&other.id)
                    && c.descendants(&other.id).contains(&n.id)
                    && other.id != n.id
            }) {
                ops.push(Operation::UpdateNode {
                    id: n.id.clone(),
                    changes: NodePatch {
                        parent_id: Some(id.clone()),
                        ..Default::default()
                    },
                });
            }
        }
        self.selection.set(vec![id]);
        self.edit(ops);
    }
    pub fn ungroup(self) {
        let Some(n) = self.selected() else {
            return;
        };
        if n.kind.is_container() {
            self.edit(vec![Operation::DeleteNode { id: n.id }]);
        }
    }
    pub fn center(self) -> Point {
        let (w, h) = stage_size();
        let v = self.viewport.get_untracked();
        Point {
            x: (w / 2. - v.x) / v.zoom,
            y: (h / 2. - v.y) / v.zoom,
        }
    }
    pub fn fit(self) {
        let Some(c) = self.board.get_untracked() else {
            return;
        };
        let (w, h) = stage_size();
        if c.nodes.is_empty() {
            self.viewport.set(Viewport {
                x: w / 2. - 110.,
                y: h / 2. - 80.,
                zoom: 1.,
            });
            return;
        }
        let minx = c.nodes.iter().map(|n| n.x).fold(f64::INFINITY, f64::min);
        let miny = c.nodes.iter().map(|n| n.y).fold(f64::INFINITY, f64::min);
        let maxx = c
            .nodes
            .iter()
            .map(|n| n.x + n.width)
            .fold(f64::NEG_INFINITY, f64::max);
        let maxy = c
            .nodes
            .iter()
            .map(|n| n.y + n.height)
            .fold(f64::NEG_INFINITY, f64::max);
        let zoom = ((w - 160.) / (maxx - minx))
            .min((h - 220.) / (maxy - miny))
            .clamp(0.1, 1.1);
        self.viewport.set(Viewport {
            x: (w - (maxx - minx) * zoom) / 2. - minx * zoom,
            y: (h - (maxy - miny) * zoom) / 2. - miny * zoom - 20.,
            zoom,
        });
    }
    pub fn zoom(self, factor: f64) {
        let (w, h) = stage_size();
        self.zoom_at(factor, w / 2., h / 2.);
    }
    pub fn zoom_at(self, factor: f64, x: f64, y: f64) {
        self.following.set(String::new());
        self.viewport.update(|v| {
            let next = (v.zoom * factor).clamp(0.1, 3.);
            let ratio = next / v.zoom;
            v.x = x - (x - v.x) * ratio;
            v.y = y - (y - v.y) * ratio;
            v.zoom = next;
        });
    }
    pub fn node_display(self, mut node: Node) -> Node {
        if let Some(d) = self.drag.get()
            && let Some(n) = d.nodes.iter().find(|n| n.id == node.id)
        {
            match d.kind {
                DragKind::Move => {
                    node.x = n.x + (d.current.x - d.start.x) / d.camera.zoom;
                    node.y = n.y + (d.current.y - d.start.y) / d.camera.zoom;
                }
                DragKind::Resize => {
                    node.width =
                        (n.width + (d.current.x - d.start.x) / d.camera.zoom).clamp(60., 2000.);
                    node.height =
                        (n.height + (d.current.y - d.start.y) / d.camera.zoom).clamp(60., 2000.);
                }
                _ => {}
            }
        }
        node
    }
}

pub fn stage_size() -> (f64, f64) {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("stage"))
        .map(|e| {
            let r = e.get_bounding_client_rect();
            (r.width(), r.height())
        })
        .unwrap_or((1000., 750.))
}
pub fn local(x: f64, y: f64) -> Point {
    let rect = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("stage"))
        .map(|e| e.get_bounding_client_rect());
    Point {
        x: x - rect.as_ref().map_or(0., |r| r.left()),
        y: y - rect.as_ref().map_or(0., |r| r.top()),
    }
}
pub fn world(p: &Point, v: Viewport) -> Point {
    Point {
        x: (p.x - v.x) / v.zoom,
        y: (p.y - v.y) / v.zoom,
    }
}
pub(crate) async fn get<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    let r = Request::get(url)
        .send()
        .await
        .map_err(|_| "Cannot reach Pinboard. Check that the server is running.".to_string())?;
    decode(r).await
}
pub(crate) async fn post<T: serde::de::DeserializeOwned>(
    url: &str,
    body: &impl Serialize,
) -> Result<T, String> {
    let r = Request::post(url)
        .json(body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|_| {
            "Connection lost. Your change was not confirmed; reload before retrying.".to_string()
        })?;
    decode(r).await
}
pub(crate) async fn decode<T: serde::de::DeserializeOwned>(
    response: gloo_net::http::Response,
) -> Result<T, String> {
    if !response.ok() {
        let status = response.status();
        let error = response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| v["error"].as_str().map(str::to_string))
            .unwrap_or_else(|| format!("Request failed ({status})"));
        return Err(error);
    }
    response
        .json()
        .await
        .map_err(|_| "The server returned an invalid response".into())
}
