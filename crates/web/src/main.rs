use leptos::tachys::html::attribute::custom::CustomAttribute;
mod browser;
mod client_collab;
mod collaboration_ui;
mod editor_ui;
mod interaction;
mod state;

use leptos::prelude::*;
use pinboard_core::*;
use state::*;
use wasm_bindgen::JsCast;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(AppView);
}

#[component]
pub(crate) fn Icon(#[prop(default = "spark")] name: &'static str) -> impl IntoView {
    let paths = match name {
        "arrow" => "<path d='m5 3 14 9-7 1-3 7z'/>",
        "hand" => {
            "<path d='M8 12V5a2 2 0 0 1 4 0v6-7a2 2 0 0 1 4 0v8-5a2 2 0 0 1 4 0v8c0 4-3 6-6 6h-2c-2 0-3-1-4-3l-4-5a2 2 0 0 1 3-2l1 1z'/>"
        }
        "note" => "<path d='M4 4h16v11l-5 5H4zM14 20v-6h6M8 8h8M8 11h5'/>",
        "oval" => "<ellipse cx='12' cy='12' rx='9' ry='6'/>",
        "step" => "<rect x='3' y='5' width='18' height='14' rx='3'/><path d='M8 10h8M8 14h5'/>",
        "diamond" => "<path d='m12 3 9 9-9 9-9-9z'/>",
        "text" => "<path d='M5 5h14M12 5v15M8 20h8'/>",
        "pen" => "<path d='m4 20 1-6L16 3l5 5-11 11zM14 5l5 5M5 14l5 5'/>",
        "line" => {
            "<circle cx='5' cy='18' r='2'/><circle cx='19' cy='6' r='2'/><path d='m7 17 10-10'/>"
        }
        "frame" => "<path d='M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5'/>",
        "plus" => "<path d='M12 5v14M5 12h14'/>",
        "minus" => "<path d='M5 12h14'/>",
        "chevron" => "<path d='m8 10 4 4 4-4'/>",
        "share" => "<path d='M12 15V3m-4 4 4-4 4 4M5 11v9h14v-9'/>",
        "download" => "<path d='M12 3v12m-4-4 4 4 4-4M4 16v5h16v-5'/>",
        "check" => "<path d='m5 12 4 4 10-10'/>",
        "undo" => "<path d='M8 5 3 10l5 5M3 10h10a7 7 0 0 1 7 7'/>",
        "redo" => "<path d='m16 5 5 5-5 5M21 10H11a7 7 0 0 0-7 7'/>",
        "close" => "<path d='m6 6 12 12M6 18 18 6'/>",
        "mic" => {
            "<rect x='9' y='2' width='6' height='13' rx='3'/><path d='M5 10v2a7 7 0 0 0 14 0v-2M12 19v3M8 22h8'/>"
        }
        "send" => "<path d='M12 19V5m-6 6 6-6 6 6'/>",
        "grid" => {
            "<rect x='3' y='3' width='7' height='7' rx='1'/><rect x='14' y='3' width='7' height='7' rx='1'/><rect x='3' y='14' width='7' height='7' rx='1'/><rect x='14' y='14' width='7' height='7' rx='1'/>"
        }
        "image" => {
            "<rect x='3' y='3' width='18' height='18' rx='3'/><circle cx='8' cy='8' r='2'/><path d='m3 18 6-6 4 4 3-3 5 5'/>"
        }
        "copy" => "<rect x='8' y='8' width='13' height='13' rx='2'/><path d='M16 8V3H3v13h5'/>",
        "trash" => "<path d='M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7'/>",
        "link" => {
            "<path d='m10 13 4-4m-6 7-2 2a4 4 0 0 1-6-6l5-5a4 4 0 0 1 6 0m2 1 2-2a4 4 0 0 1 6 6l-5 5a4 4 0 0 1-6 0' transform='translate(1 0)'/>"
        }
        "comment" => "<path d='M3 3h18v13H9l-6 5z'/><path d='M7 7h10M7 11h7'/>",
        "help" => {
            "<circle cx='12' cy='12' r='9'/><path d='M9 9a3 3 0 1 1 5 2c-1 1-2 1-2 3M12 17h.01'/>"
        }
        "folder" => "<path d='M3 6V4h7l2 3h9v13H3z'/>",
        "external" => "<path d='M14 3h7v7M21 3 10 14M10 3H3v18h18v-7'/>",
        _ => "<path d='m12 3 2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5zM20 2v4M18 4h4'/>",
    };
    view! {<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" inner_html=paths></svg>}
}

#[component]
fn AppView() -> impl IntoView {
    let app = App::new();
    provide_context(app);
    app.start();
    Effect::new(move |_| {
        let v = app.viewport.get();
        if app.following.get().is_empty() {
            browser::send_camera(
                v.x,
                v.y,
                v.zoom,
                &app.me
                    .get()
                    .and_then(|m| m.user)
                    .map(|u| u.name)
                    .unwrap_or_else(|| "You".into()),
            );
        }
    });
    let search = RwSignal::new(String::new());
    view! {
        <Show when=move||app.me.get().is_some_and(|m|m.user.is_some()) fallback=move||view!{<collaboration_ui::LoginScreen/>}>
        <div class="app" class:focus-mode=move||app.focus.get() on:keydown=move|e|interaction::keydown(app,e)>
            <header class="app-header">
                <a class="brand" href="/" aria-label="Pinboard home"><span class="brand-mark"><i></i><i></i><i></i><i></i></span><span>"pinboard"</span><span class="beta">"preview"</span></a>
                <div class="workspace-crumb"><span class="divider"></span><Icon name="folder"/><span>"My workspace"</span><span class="crumb-slash">"/"</span><span>"Canvas"</span></div>
                <div class="header-right"><button class="icon-button mobile-new" aria-label="New canvas" title="New canvas" on:click=move|_|app.create("Untitled canvas".into()) disabled=move||app.busy.get()><Icon name="plus"/></button><span class="private"><span class="tiny-dot"></span>{move||if app.me.get().is_some_and(|m|m.auth_mode!="local"){"Team workspace"}else{"Personal workspace"}}</span><button id="account-settings" class="avatar" title="Account and organization settings" aria-label="Account and organization settings" on:click=move|_|app.settings_open.set(true)>{move||app.me.get().and_then(|m|m.user).and_then(|u|u.name.chars().next()).unwrap_or('Y').to_string()}</button></div>
            </header>
            <div class="app-body">
                <aside class="sidebar">
                    <div class="space-switch"><span class="space-avatar">"Y"</span><div><strong>"Your thinking space"</strong><small>"A home for the big picture"</small></div></div>
                    <button class="new-canvas" on:click=move|_|app.create("Untitled canvas".into()) disabled=move||app.busy.get()><Icon name="plus"/>"New canvas"<kbd>"+"</kbd></button>
                    <div class="sidebar-nav"><span class="nav-current"><Icon name="grid"/>"All canvases"<span class="count">{move||app.library.get().len()}</span></span></div>
                    <div class="section-label">"YOUR CANVASES"</div>
                    <div class="search-box"><input aria-label="Find a canvas" placeholder="Find a canvas…" prop:value=move||search.get() on:input=move|e|search.set(event_target_value(&e))/><span>"⌕"</span></div>
                    <nav class="canvas-list" aria-label="Saved canvases">
                        <For each={move||app.library.get().into_iter().filter(|c|c.title.to_lowercase().contains(&search.get().to_lowercase())).collect::<Vec<_>>()} key=|c|c.id.clone() children=move|c|{let id=c.id.clone();let active_id=c.id.clone();let item_id=StoredValue::new(c.id.clone());view!{<button class="canvas-list-item" class:active=move||app.active.get()==active_id on:click=move|_|app.open(id.clone()) disabled=move||app.busy.get()><span class="mini-canvas"><i></i><i></i><i></i></span><span><strong>{move||app.library.get().iter().find(|c|c.id==item_id.get_value()).map(|c|c.title.clone()).unwrap_or_default()}</strong><small>{move||app.library.get().iter().find(|c|c.id==item_id.get_value()).map(|c|format!("{} ideas · saved",c.node_count)).unwrap_or_default()}</small></span></button>}}/>
                    </nav>
                    <div class="sidebar-bottom"><div class="mcp-card"><span class="mcp-symbol">"✳"</span><strong>"Better with a thinking partner"</strong><p>"Bring Claude or Codex into your canvas."</p><button on:click=move|_|app.setup.set(true)>"Connect an assistant"<Icon name="external"/></button></div><button class="help-button" on:click=move|_|app.setup.set(true)><Icon name="help"/>"A little help getting started"</button><div class="sidebar-footer">"SPACE FOR YOUR NEXT GOOD IDEA"<span><Icon name="external"/></span></div></div>
                </aside>
                <main class="main">
                    <div class="board-header">
                        <div class="board-identity"><span class="board-icon"><Icon name="grid"/></span><div><input id="board-title" aria-label="Canvas title" maxlength="200" prop:value=move||app.board.get().map(|c|c.title).unwrap_or_default() on:change=move|e|app.edit(vec![Operation::Rename{title:event_target_value(&e)}]) disabled=move||app.busy.get()||!app.can_edit()/><div class="save-indicator"><span class="tiny-dot" class:saving=move||app.busy.get()></span>{move||if app.busy.get(){"Saving your changes…"}else{"Your ideas, always saved"}}</div></div></div>
                        <div class="board-actions"><div class="presence"><For each=move||app.peers.get() key=|p|p.id.clone() children=move|p|{let id=p.id.clone();let name=p.name.clone();view!{<button class="peer-avatar" title=format!("Follow {name}") aria-label=format!("Follow {name}") on:click=move|_|{app.following.set(id.clone());if let Some(latest)=app.peers.get_untracked().iter().find(|peer|peer.id==id){app.follow_peer(latest);}}>{name.chars().next().unwrap_or('P').to_string()}</button>}}/></div><button class="button subtle ai-toggle" on:click=move|_|app.panel.set(Panel::Ai)><Icon name="spark"/>"Ask AI"</button><button class="button subtle comments-toggle" aria-pressed=move||app.comment_mode.get().to_string() on:click=move|_|{app.comment_mode.update(|v|*v = !*v);app.panel.set(if app.comment_mode.get_untracked(){Panel::Comments}else{Panel::Ideas});}><Icon name="comment"/>"Comments "{move||app.threads.get().iter().filter(|t|!t.resolved).count()}</button><details class="share-menu"><summary class="button primary"><Icon name="share"/>"Share & export"<Icon name="chevron"/></summary><div class="menu-popover"><button on:click=move|_|app.open_sharing()><Icon name="share"/>"Invite and manage access"</button><button on:click=move|_|app.copy(web_sys::window().map(|w|w.location().href().unwrap_or_default()).unwrap_or_default())><Icon name="link"/>"Copy canvas link"</button><button on:click=move|_|app.png()><Icon name="download"/>"Export as PNG"</button><a href=move||format!("/api/canvases/{}/export/markdown",app.active.get())><Icon name="text"/>"Download document (.md)"</a><a href=move||format!("/api/canvases/{}/export/slides",app.active.get())><Icon name="step"/>"Download slide deck (.html)"</a><a href=move||format!("/api/canvases/{}/export/json",app.active.get())><Icon name="download"/>"Download canvas (.json)"</a><a href=move||format!("/api/canvases/{}/export/mermaid",app.active.get())><Icon name="line"/>"Download Mermaid (.mmd)"</a><p>{move||if app.me.get().is_some_and(|m|m.auth_mode=="local"){"Local links open on this machine. Configure sign-in and email delivery for team sharing."}else{"Recipients need access to this canvas. Invite collaborators by their verified email address."}}</p></div></details></div>
                    </div>
                    <div class="canvas-shell">
                        <section id="stage" class="stage" tabindex="0" role="tree" aria-owns=move||app.board.get().map(|c|c.nodes.iter().filter(|n|n.parent_id.is_empty()).map(|n|format!("node-{}",n.id)).collect::<Vec<_>>().join(" ")).unwrap_or_default() aria-label="Visual canvas. Drag empty space to select; use H to pan. Use Tab to reach nodes."
                            class:pan-tool=move||app.tool.get()==Tool::Pan class:ink-tool=move||app.tool.get()==Tool::Ink
                            style=move||{let v=app.viewport.get();format!("background-size:{}px {}px;background-position:{}px {}px",24.*v.zoom,24.*v.zoom,v.x,v.y)}
                            on:pointerdown=move|e|{browser::focus_element("stage");interaction::stage_down(app,e)}
                            on:pointermove=move|e|interaction::stage_move(app,e)
                            on:pointerup=move|_|interaction::stage_up(app)
                            on:pointercancel=move|_|app.drag.set(None)
                            on:wheel=move|e:web_sys::WheelEvent|{if e.ctrl_key()||e.meta_key(){e.prevent_default();let p=state::local(e.client_x() as f64,e.client_y() as f64);app.zoom_at((-e.delta_y()*0.008).exp(),p.x,p.y);}else{app.following.set(String::new());app.viewport.update(|v|{v.x-=e.delta_x();v.y-=e.delta_y();});}}
                        >
                            <div class="canvas-caption"><span class="eyebrow">"A LITTLE SPACE TO THINK BIG"</span><p>"Gather your ideas. See what connects."</p></div>
                            <div class="world" style=move||{let v=app.viewport.get();format!("transform:translate({}px,{}px) scale({});",v.x,v.y,v.zoom)}>
                                <svg class="connectors" width="1" height="1" aria-label="Dependency connectors"><defs><marker id="arrowhead" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M0,0 L8,4 L0,8" fill="#8c9989"/></marker></defs><For each=move||app.board.get().map(|c|c.edges).unwrap_or_default() key=|e|e.id.clone() children=move|e|view!{<Connector edge=e/>}/></svg>
                                <For each=move||app.board.get().map(|c|c.nodes).unwrap_or_default() key=|n|n.id.clone() children=move|n|view!{<CanvasNode id=n.id/>}/>
                                <collaboration_ui::CommentPins/>
                                {move||app.drag.get().filter(|d|matches!(d.kind,DragKind::Ink|DragKind::Line)).map(|d|view!{<svg class="ink-preview" width="1" height="1"><path d=interaction::path(&d.points) fill="none" stroke="#345341" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/></svg>})}
                            </div>
                            {move||app.drag.get().filter(|d|d.kind==DragKind::Marquee).map(|d|view!{<div class="marquee" style=format!("left:{}px;top:{}px;width:{}px;height:{}px",d.start.x.min(d.current.x),d.start.y.min(d.current.y),(d.current.x-d.start.x).abs(),(d.current.y-d.start.y).abs())></div>})}
                            <Show when=move||app.board.get().is_some_and(|c|c.nodes.is_empty())><div class="empty-canvas"><span>"✳"</span><h2>"Every good idea starts somewhere."</h2><p>"Add a sticky note, capture a thought below,"<br/>"or invite your AI to help you begin."</p><button class="button primary" on:pointerdown=|e|e.stop_propagation() on:click=move|_|{let p=app.center();app.add(NodeKind::Note,p.x-110.,p.y-80.);} ><Icon name="plus"/>"Add your first idea"</button></div></Show>
                        </section>
                        <Toolbar/>
                        <editor_ui::FormatToolbar/>
                        <div class="top-canvas-actions"><button class="icon-button" aria-label="Undo last edit" title="Undo last edit (⌘/Ctrl Z)" disabled=move||app.undo.get().is_empty()||app.busy.get() on:click=move|_|app.undo_last()><Icon name="undo"/></button><span class="tool-separator"></span><button class="icon-button" aria-label="Toggle full canvas" title="Toggle full canvas" on:click=move|_|{app.focus.update(|v|*v = !*v);app.fit();}><Icon name="frame"/></button></div>
                        <Show when=move||!app.following.get().is_empty()><button class="following-pill" on:click=move|_|app.following.set(String::new())>{move||format!("Following {} · click to stop",app.peers.get().iter().find(|p|p.id==app.following.get()).map(|p|p.name.clone()).unwrap_or_else(||"teammate".into()))}<Icon name="close"/></button></Show>
                        <div class="canvas-bottom"><div class="zoom-controls"><button aria-label="Zoom out" on:click=move|_|app.zoom(1./1.15)><Icon name="minus"/></button><span>{move||format!("{}%",(app.viewport.get().zoom*100.).round() as i32)}</span><button aria-label="Zoom in" on:click=move|_|app.zoom(1.15)><Icon name="plus"/></button><span class="tool-separator"></span><button aria-label="Fit canvas to window" title="Fit to window (⌘/Ctrl 0)" on:click=move|_|app.fit()><Icon name="frame"/></button></div><div class="canvas-hint">{move||if app.selection.get().is_empty(){format!("{} · {}",app.tool.get().label(),if app.tool.get()==Tool::Select{"Drag to select"}else if app.tool.get()==Tool::Connect{"Click two nodes"}else{"Use the canvas"})}else{format!("{} selected · Shift-click to add",app.selection.get().len())}}</div><div class="live-state"><span class="tiny-dot" class:offline=move||!app.live.get()></span>{move||if app.live.get(){"Live"}else{"Reconnecting…"}}</div></div>
                        <div class="thought-composer"><div class="composer-row"><span class="composer-spark">"✳"</span><textarea aria-label="Capture a thought" placeholder="A thought, a question, a spark of an idea…" rows="1" disabled=move||!app.can_edit() prop:value=move||app.thought.get() on:input=move|e|app.thought.set(event_target_value(&e)) on:keydown=move|e|if e.key()=="Enter"&&!e.shift_key()&&!e.is_composing(){e.prevent_default();app.capture();}></textarea><button class="icon-button mic-button" class:listening=move||app.voice.get() title="Dictate a thought (browser speech recognition)" aria-label=move||if app.voice.get(){"Pause microphone"}else{"Start microphone"} on:click=move|_|app.voice_toggle()><Icon name="mic"/></button><button class="send-button" aria-label="Add thought to canvas" title="Add to canvas" disabled=move||app.busy.get()||!app.can_edit()||app.thought.get().trim().is_empty() on:click=move|_|app.capture()><Icon name="send"/></button></div><div class="composer-caption" aria-live="polite">{move||match app.voice_state.get().as_str(){"listening"=>"Listening… Escape to pause. Review your words before adding.","paused"=>"Microphone paused. Review or edit your words, then press Enter.","error"=>"Voice capture unavailable. Your words are kept; you can keep typing.",_=>"Make it a note with Enter. Shift + Enter for a new line."}}</div></div>
                    </div>
                    <footer class="status-bar"><span role="status" aria-live="polite">{move||app.status.get()}</span><a href="https://github.com/robot-head/pinboard-mcp" target="_blank" rel="noreferrer">"Made for humans & their AI"<span><Icon name="external"/></span></a></footer>
                </main>
                <SidePanel/>
            </div>
            <Show when=move||app.setup.get()><SetupModal/></Show>
            <Show when=move||app.share_open.get()><collaboration_ui::ShareModal/></Show>
            <Show when=move||app.settings_open.get()><collaboration_ui::SettingsModal/></Show>
            <Show when=move||app.me.get().is_some_and(|m|!m.policy.canvas_enabled)><div class="feature-disabled"><h2>"Canvas is disabled for this organization"</h2><p>"An organization administrator can enable it under Artifacts settings."</p><button class="button primary" on:click=move|_|app.settings_open.set(true)>"Open settings"</button><button class="button" on:click=move|_|app.start()>"Refresh availability"</button></div></Show>
        </div>
        </Show>
    }
}

#[component]
fn Toolbar() -> impl IntoView {
    let app = expect_context::<App>();
    let tools = [
        (Tool::Select, "arrow", "V"),
        (Tool::Pan, "hand", "H"),
        (Tool::Note, "note", "N"),
        (Tool::Rectangle, "step", "R"),
        (Tool::Rounded, "step", "Shift+R"),
        (Tool::Oval, "oval", "O"),
        (Tool::Step, "step", ""),
        (Tool::Decision, "diamond", "D"),
        (Tool::Text, "text", "T"),
        (Tool::Line, "line", "L"),
        (Tool::Connect, "line", "Shift+L"),
        (Tool::Ink, "pen", "P"),
        (Tool::Frame, "frame", "F"),
    ];
    view! {<div class="canvas-toolbar" id="canvas-toolbar" tabindex="0" role="toolbar" aria-label="Canvas drawing tools">{tools.into_iter().map(|(tool,icon,key)|view!{<button class="tool-button" disabled=move||!app.can_edit()&&!matches!(tool,Tool::Select|Tool::Pan) class:active=move||app.tool.get()==tool aria-label=tool.label() aria-pressed=move||(app.tool.get()==tool).to_string() title=format!("{} ({key})",tool.label()) on:click=move|_|{app.tool.set(tool);app.connect_from.set(String::new());}><Icon name=icon/></button>}).collect_view()}</div>}
}

#[component]
fn CanvasNode(id: String) -> impl IntoView {
    let app = expect_context::<App>();
    let node_id = id.clone();
    let node = Memo::new(move |_| {
        app.board
            .get()
            .and_then(|c| c.nodes.into_iter().find(|n| n.id == node_id))
            .map(|n| app.node_display(n))
    });
    let click_id = id.clone();
    let resize_id = id.clone();
    let class_id = id.clone();
    let semantic_id = StoredValue::new(id.clone());
    let focused_id = id;
    let level = move || {
        app.board.get().map(|c| {
            let mut id = semantic_id.get_value();
            let mut level = 1;
            while let Some(n) = c.nodes.iter().find(|n| n.id == id) {
                if n.parent_id.is_empty() {
                    break;
                }
                level += 1;
                id = n.parent_id.clone();
                if level > 100 {
                    break;
                }
            }
            level
        })
    };
    view! {<div class=move||node.get().map(|n|format!("canvas-node {} kind-{} {} {}",n.color,kind_name(&n.kind),if n.suggested{"suggested"}else{""},if app.selection.get().contains(&class_id){"selected"}else{""})).unwrap_or_default()
        style=move||node.get().map(|n|format!("left:{}px;top:{}px;width:{}px;height:{}px;z-index:{}",n.x,n.y,n.width,n.height,n.z_index)).unwrap_or_default()
        id=move||format!("node-{}",semantic_id.get_value()) data-node-id=move||semantic_id.get_value() role="treeitem"
        tabindex=move||if node.get().is_some_and(|n|n.parent_id==app.focus_parent.get()){"0"}else{"-1"}
        aria-selected=move||app.selection.get().contains(&semantic_id.get_value()).to_string()
        aria-expanded=move||node.get().and_then(|n|n.kind.is_container().then_some("true"))
        aria-owns=move||app.board.get().map(|c|c.nodes.iter().filter(|n|n.parent_id==semantic_id.get_value()).map(|n|format!("node-{}",n.id)).collect::<Vec<_>>().join(" ")).filter(|s|!s.is_empty())
        aria-label=move||node.get().map(|n|format!("{}: {}. {}",kind_name(&n.kind),n.title,n.body)).unwrap_or_default()
        on:pointerdown=move|e|interaction::node_down(app,click_id.clone(),e,false)
        on:dblclick=move|_|{app.panel.set(Panel::Inspector);browser::focus_element("node-title");}
        on:focus=move|_|{if !app.selection.get_untracked().contains(&focused_id){app.selection.set(vec![focused_id.clone()]);}app.panel.set(Panel::Inspector);}
    >
        {move||node.get().map(|n|{
            let ink = matches!(n.kind,NodeKind::Sketch|NodeKind::Line);
            view! {
                {ink.then(|| view! { <svg class="sketch-svg" width="100%" height="100%" viewBox=format!("0 0 {} {}",n.width,n.height)><path d=interaction::path(&n.points) fill="none" stroke=ink_color(&n.color) stroke-width=n.stroke_width stroke-linecap="round" stroke-linejoin="round"/></svg> })}
                {(!ink).then(|| view! {
                    <div class="node-content">
                        <span class="node-group">{n.group.clone()}</span>
                        {n.suggested.then(||view!{<span class="suggestion-label">"AI suggestion"</span>})}
                        {(n.kind==NodeKind::Image).then(||view!{<img src=n.url.clone() alt=n.title.clone() draggable="false" loading="lazy" referrerpolicy="no-referrer"/>})}
                        {(n.kind==NodeKind::Video).then(||view!{<video src=n.url.clone() controls preload="metadata" on:pointerdown=|e|e.stop_propagation()></video>})}
                        <editor_ui::NodeTitle node=n.clone()/><editor_ui::StyledBody node=n.clone()/>
                        <div class="node-meta">
                            {(!n.owner.is_empty()).then(||view!{<span class="owner-badge"><i>{n.owner.chars().next().unwrap_or('Y').to_string()}</i>{n.owner.clone()}</span>})}
                            {(!n.due.is_empty()).then(||view!{<span>{n.due.clone()}</span>})}
                        </div>
                    </div>
                })}
            }
        })}
        <button class="resize-handle" aria-label="Resize object" title="Drag to resize" tabindex="-1" on:pointerdown=move|e|interaction::node_down(app,resize_id.clone(),e,true)></button>
    </div>}.attr("aria-level", level)
}
fn kind_name(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Note => "note",
        NodeKind::Step => "step",
        NodeKind::Rectangle => "rectangle",
        NodeKind::RoundedRectangle => "rounded-rectangle",
        NodeKind::Oval => "oval",
        NodeKind::Milestone => "milestone",
        NodeKind::Swimlane => "swimlane",
        NodeKind::Line => "line",
        NodeKind::Decision => "decision",
        NodeKind::Text => "text",
        NodeKind::Frame => "frame",
        NodeKind::Image => "image",
        NodeKind::Video => "video",
        NodeKind::Sketch => "sketch",
    }
}

#[component]
fn Connector(edge: Edge) -> impl IntoView {
    let app = expect_context::<App>();
    let id = edge.id.clone();
    let selected_id = StoredValue::new(id.clone());
    let geometry = Memo::new(move |_| {
        let c = app.board.get()?;
        let e = c.edges.iter().find(|e| e.id == id)?.clone();
        let a = app.node_display(c.nodes.iter().find(|n| n.id == e.from)?.clone());
        let b = app.node_display(c.nodes.iter().find(|n| n.id == e.to)?.clone());
        Some((e, a, b))
    });
    let select = move || {
        app.selected_edge.set(selected_id.get_value());
        app.selection.set(vec![]);
        app.panel.set(Panel::Inspector);
    };
    view! {<g role="button" tabindex="0" aria-label=move||geometry.get().map(|(e,a,b)|format!("Connector: {} to {}. {}",a.title,b.title,e.label)).unwrap_or_default() on:click=move|e|{e.stop_propagation();select();} on:focus=move|_|select() on:pointerdown=|e|e.stop_propagation()>
    <path d=move||geometry.get().map(|(e,a,b)|connector_path(&a,&b,&e.routing)).unwrap_or_default() fill="none" stroke="#8c9989" stroke-width="1.5" marker-end=move||geometry.get().and_then(|(e,_,_)|e.arrow.then_some("url(#arrowhead)"))/>
    <path class="edge-hit" d=move||geometry.get().map(|(e,a,b)|connector_path(&a,&b,&e.routing)).unwrap_or_default() fill="none" stroke="transparent" stroke-width="16"/>
    <text x=move||geometry.get().map(|(_,a,b)|{let(p,q)=connector_endpoints(&a,&b);(p.x+q.x)/2.}).unwrap_or_default() y=move||geometry.get().map(|(_,a,b)|{let(p,q)=connector_endpoints(&a,&b);(p.y+q.y)/2.-10.}).unwrap_or_default() text-anchor="middle" font-size="12" fill="#64705f">{move||geometry.get().map(|(e,_,_)|e.label).unwrap_or_default()}</text></g>}
}

#[component]
fn SidePanel() -> impl IntoView {
    let app = expect_context::<App>();
    view! {<aside class="side-panel" style=move||format!("width:{}px",app.panel_width.get())><button class="panel-resizer" role="separator" aria-label="Resize context panel" aria-orientation="vertical" aria-valuemin="240" aria-valuemax="500" aria-valuenow=move||app.panel_width.get() on:pointerdown=move|e|{e.prevent_default();app.panel_drag.set(Some((e.client_x() as f64,app.panel_width.get_untracked())));if let Some(el)=e.target().and_then(|t|t.dyn_into::<web_sys::Element>().ok()){let _=el.set_pointer_capture(e.pointer_id());}} on:pointermove=move|e|{if let Some((start,width))=app.panel_drag.get_untracked(){app.panel_width.set((width+start-e.client_x() as f64).clamp(240.,500.));}} on:pointerup=move|_|app.panel_drag.set(None) on:pointercancel=move|_|app.panel_drag.set(None) on:keydown=move|e|{if matches!(e.key().as_str(),"ArrowLeft"|"ArrowRight"){e.prevent_default();e.stop_propagation();app.panel_width.update(|v|*v=(*v+if e.key()=="ArrowLeft"{20.}else{-20.}).clamp(240.,500.));}}></button><div class="panel-tabs"><button class:active=move||app.panel.get()==Panel::Ideas on:click=move|_|app.panel.set(Panel::Ideas)>"Overview"</button><button class:active=move||app.panel.get()==Panel::Inspector on:click=move|_|app.panel.set(Panel::Inspector)>"Selection"<Show when=move||!app.selection.get().is_empty()><span>{move||app.selection.get().len()}</span></Show></button><button class:active=move||app.panel.get()==Panel::Ai on:click=move|_|app.panel.set(Panel::Ai)><Icon name="spark"/>"AI"</button><button class:active=move||app.panel.get()==Panel::Comments on:click=move|_|{app.comment_mode.set(true);app.panel.set(Panel::Comments);}><Icon name="comment"/><span class="comment-tab-label">"Comments"</span></button><button class="mobile-panel-close icon-button" aria-label="Close panel" on:click=move|_|app.panel.set(Panel::Ideas)><Icon name="close"/></button></div><div class="panel-scroll">{move||match app.panel.get(){Panel::Ideas=>view!{<Overview/>}.into_any(),Panel::Inspector=>view!{<Inspector/>}.into_any(),Panel::Ai=>view!{<AiPanel/>}.into_any(),Panel::Comments=>view!{<collaboration_ui::CommentsPanel/>}.into_any()}}</div><div class="panel-footer"><span>"✳"</span>"Think freely. Make connections."</div></aside>}
}

#[component]
fn Overview() -> impl IntoView {
    let app = expect_context::<App>();
    view! {<div class="overview"><div class="tiny-illustration"><div>"what if…"</div><div>"a little idea"</div><div>"next step"</div><svg viewBox="0 0 220 100"><path d="M40 60Q100 0 170 65" fill="none" stroke="#a2ae96" stroke-width="1.5" stroke-dasharray="4 4"/></svg></div><span class="eyebrow">"ROOM TO MAKE SENSE OF IT"</span><h1>"Good ideas need a little space."</h1><p>"A canvas for the things you're figuring out. Start anywhere. Let the connections follow."</p><div class="panel-divider"></div><h2>"A few ways to begin"</h2>{[("note","Let it all out","Add a few thoughts. No perfect order needed."),("line","Map out a process","Turn a messy plan into a clear way forward."),("diamond","See both sides","Lay out your options and find what matters.")].into_iter().map(|(icon,title,text)|{let prompt=format!("{title}. {text}");view!{<button class="idea-card" on:click=move|_|{app.prompt.set(prompt.clone());app.panel.set(Panel::Ai);}><span><Icon name=icon/></span><div><strong>{title}</strong><p>{text}</p></div><i><Icon name="external"/></i></button>}}).collect_view()}<div class="quiet-tip"><span>"⌘"</span><p>"A few handy shortcuts"<small>"N for a note · H to pan · ⌘/Ctrl 0 to fit"</small></p></div></div>}
}

#[component]
fn Inspector() -> impl IntoView {
    let app = expect_context::<App>();
    let media_url = RwSignal::new(String::new());
    let media_kind = RwSignal::new("image".to_string());
    view! {<div class="inspector"><span class="eyebrow">"MAKE IT YOURS"</span><h2>{move||if app.selection.get().len()==1{"One idea, many possibilities."}else{"Bring a little focus."}}</h2>
        {move||app.selected().map(|n|{let id=StoredValue::new(n.id.clone());view!{<div class="form"><label for="node-title">"Title"</label><input id="node-title" prop:value=n.title maxlength="1000" on:change=move|e|app.patch(id.get_value(),NodePatch{title:Some(event_target_value(&e)),..Default::default()}) disabled=move||app.busy.get()||!app.can_edit()/><label for="node-body">"Details"</label><textarea id="node-body" rows="5" prop:value=n.body maxlength="20000" placeholder="Give this idea a little more context…" on:change=move|e|app.patch(id.get_value(),NodePatch{body:Some(event_target_value(&e)),..Default::default()}) disabled=move||app.busy.get()||!app.can_edit()></textarea><label>"Color"</label><div class="swatches">{["butter","lavender","mint","peach","sky","paper"].map(|color|{let selected=n.color==color;view!{<button class=format!("swatch {color}") class:chosen=selected disabled=move||!app.can_edit() aria-label=format!("Color {color}") aria-pressed=selected.to_string() on:click=move|_|app.patch(id.get_value(),NodePatch{color:Some(color.into()),..Default::default()})></button>}}).collect_view()}</div><label for="node-group">"Theme or group"</label><input id="node-group" prop:value=n.group maxlength="200" placeholder="e.g. Research, Next steps" on:change=move|e|app.patch(id.get_value(),NodePatch{group:Some(event_target_value(&e)),..Default::default()}) disabled=move||app.busy.get()||!app.can_edit()/><div class="form-pair"><div><label for="node-owner">"Owner"</label><input id="node-owner" prop:value=n.owner maxlength="200" placeholder="Who's on it?" on:change=move|e|app.patch(id.get_value(),NodePatch{owner:Some(event_target_value(&e)),..Default::default()}) disabled=move||app.busy.get()||!app.can_edit()/></div><div><label for="node-due">"Due date"</label><input id="node-due" type="date" prop:value=n.due on:change=move|e|app.patch(id.get_value(),NodePatch{due:Some(event_target_value(&e)),..Default::default()}) disabled=move||app.busy.get()||!app.can_edit()/></div></div><Show when=move||n.suggested><button class="button primary" on:click=move|_|app.edit(vec![Operation::AcceptSuggestion{id:id.get_value()}])>"Keep this suggestion"<Icon name="check"/></button></Show></div>}})}
        <Show when=move||app.selected().is_none()><p class="muted">"Select an idea to edit its details. Drag a box around a few ideas to work with them together."</p></Show>
        <Show when=move||!app.selection.get().is_empty()><div class="selection-actions"><button class="button" on:click=move|_|app.duplicate() disabled=move||app.busy.get()||!app.can_edit()><Icon name="copy"/>"Duplicate"</button><button class="button danger" on:click=move|_|app.delete() disabled=move||app.busy.get()||!app.can_edit()><Icon name="trash"/>"Delete"</button></div><button class="button ai-scope" on:click=move|_|{app.prompt.set("Help me refine these selected ideas.".into());app.panel.set(Panel::Ai);} ><Icon name="spark"/>"Ask AI about this selection"</button></Show>
        <editor_ui::NodeOptions/>
        <editor_ui::EdgeInspector/>
        <div class="panel-divider"></div><h3>"Add a visual reference"</h3><p class="muted small">"Embed an image or a video from an HTTP(S) URL."</p><div class="form"><label for="media-kind">"Media type"</label><select id="media-kind" on:change=move|e|media_kind.set(event_target_value(&e))><option value="image">"Image"</option><option value="video">"Video"</option></select><label for="media-url">"Media URL"</label><input id="media-url" type="url" placeholder="https://…" prop:value=move||media_url.get() on:input=move|e|media_url.set(event_target_value(&e))/><button class="button" disabled=move||media_url.get().is_empty()||app.busy.get()||!app.can_edit() on:click=move|_|{let p=app.center();let mut n=Node::note("Visual reference",p.x-150.,p.y-120.);n.kind=if media_kind.get_untracked()=="video"{NodeKind::Video}else{NodeKind::Image};n.url=media_url.get_untracked();n.width=300.;n.height=240.;n.color="paper".into();app.edit(vec![Operation::AddNode{node:n}]);media_url.set(String::new());}><Icon name="image"/>"Add to canvas"</button></div>
    </div>}
}

#[component]
fn AiPanel() -> impl IntoView {
    let app = expect_context::<App>();
    view! {<div class="ai-panel"><span class="ai-emblem">"✳"</span><span class="eyebrow">"A FRESH PERSPECTIVE"</span><h2>"Think it through, together."</h2><p class="muted">"Your assistant can see your ideas, move the pieces, and help you find what’s missing."</p><div class="scope-badge"><span class="tiny-dot"></span>{move||if app.selection.get().is_empty(){"Working with the whole canvas".into()}else{format!("Only {} selected idea(s)",app.selection.get().len())}}</div><div class="form"><label for="ai-prompt">"What would you like to explore?"</label><textarea id="ai-prompt" rows="5" placeholder="Group these ideas by theme and suggest a next step…" prop:value=move||app.prompt.get() on:input=move|e|app.prompt.set(event_target_value(&e))></textarea><button class="button primary" disabled=move||app.prompt.get().trim().is_empty() on:click=move|_|app.copy(app.ai_request())><Icon name="copy"/>"Copy request for Claude or Codex"</button></div><p class="small muted">"Paste this request into your connected assistant. It includes this canvas and your selection, so edits stay focused."</p><div class="panel-divider"></div><h3>"Try a different lens"</h3>{["Turn this into a process with owners and dates.","Group these notes by theme and draft action items.","Find missing steps. Add them as suggestions for review."].into_iter().map(|prompt|view!{<button class="prompt-suggestion" on:click=move|_|app.prompt.set(prompt.into())>{prompt}<span><Icon name="external"/></span></button>}).collect_view()}<button class="setup-link" on:click=move|_|app.setup.set(true)>"Connect your assistant"<Icon name="external"/></button></div>}
}

#[component]
fn SetupModal() -> impl IntoView {
    let app = expect_context::<App>();
    browser::start_dialog();
    on_cleanup(browser::end_dialog);
    let url = web_sys::window()
        .map(|w| w.location().origin().unwrap_or_default() + "/mcp")
        .unwrap_or_default();
    view! {<div class="modal-backdrop" on:click=move|_|app.setup.set(false)><section class="setup-modal" role="dialog" aria-modal="true" aria-labelledby="setup-title" on:click=|e|e.stop_propagation()><button class="modal-close icon-button" aria-label="Close setup" on:click=move|_|app.setup.set(false)><Icon name="close"/></button><span class="ai-emblem">"✳"</span><span class="eyebrow">"INVITE YOUR THINKING PARTNER"</span><h2 id="setup-title">"Your ideas. A little extra perspective."</h2><p>"Pinboard exposes an MCP server that Claude and Codex can use to create, read, and edit your canvases."</p><label>"Streamable HTTP endpoint"</label><div class="endpoint"><code>{url.clone()}</code><button class="icon-button" aria-label="Copy MCP endpoint" on:click=move|_|app.copy(url.clone())><Icon name="copy"/></button></div><p class="small">{move||if app.me.get().is_some_and(|m|m.auth_mode!="local"){ "Create a personal MCP access token in Account settings, set PINBOARD_MCP_TOKEN in your client environment, and add the endpoint with --bearer-token-env-var PINBOARD_MCP_TOKEN." }else{ "For Codex CLI, add this endpoint with:" }}</p><pre>{format!("codex mcp add pinboard --url {}/mcp",web_sys::window().map(|w|w.location().origin().unwrap_or_default()).unwrap_or_default())}</pre><p class="small">"For Claude Desktop, use the stdio command in the project README. The Rust server supplies the canvas tools; your connected assistant supplies the AI."</p><div class="shortcut-grid"><span><kbd>"N"</kbd>"Sticky note"</span><span><kbd>"H"</kbd>"Pan canvas"</span><span><kbd>"P"</kbd>"Freehand ink"</span><span><kbd>"L / Shift L"</kbd>"Line / connector"</span><span><kbd>"⌘/Ctrl 0"</kbd>"Fit to window"</span><span><kbd>"Escape"</kbd>"Cancel / pause mic"</span></div><p class="small muted">"Voice capture uses your browser's speech service and is available only in supported browsers. Audio processing depends on your browser provider."</p><button class="button primary" on:click=move|_|app.setup.set(false)>"Back to the big picture"<span><Icon name="external"/></span></button></section></div>}
}
