use crate::{Icon, state::*};
use leptos::prelude::*;
use pinboard_core::*;

#[component]
pub fn NodeTitle(node: Node) -> impl IntoView {
    let app = expect_context::<App>();
    let id = StoredValue::new(node.id.clone());
    let title = StoredValue::new(node.title.clone());
    view! {{move||if app.editing_node.get()==id.get_value(){view!{<input id=format!("inline-{}",id.get_value()) class="inline-title" aria-label="Edit object title" maxlength="1000" prop:value=title.get_value() on:pointerdown=|e|e.stop_propagation() on:blur=move|e|finish_title(app,id.get_value(),event_target_value(&e)) on:keydown=move|e|{if e.key()=="Enter"{e.prevent_default();e.stop_propagation();finish_title(app,id.get_value(),event_target_value(&e));crate::browser::focus_element(&format!("node-{}",id.get_value()));}else if e.key()=="Escape"{e.prevent_default();e.stop_propagation();app.editing_node.set(String::new());crate::browser::focus_element(&format!("node-{}",id.get_value()));}}/>}.into_any()}else{view!{<h3 on:dblclick=move|e|{e.stop_propagation();if !app.can_edit(){return;}app.editing_node.set(id.get_value());crate::browser::focus_element(&format!("inline-{}",id.get_value()));}>{title.get_value()}</h3>}.into_any()}}}
}
fn finish_title(app: App, id: String, text: String) {
    if app.editing_node.get_untracked() != id {
        return;
    }
    app.editing_node.set(String::new());
    if app
        .board
        .get_untracked()
        .is_some_and(|c| c.nodes.iter().any(|n| n.id == id && n.title != text))
    {
        app.patch(
            id,
            NodePatch {
                title: Some(text),
                ..Default::default()
            },
        );
    }
}

#[component]
pub fn StyledBody(node: Node) -> impl IntoView {
    let style = format!(
        "font-size:{}px;font-weight:{};font-style:{}",
        node.text_style.font_size,
        if node.text_style.bold { 700 } else { 400 },
        if node.text_style.italic {
            "italic"
        } else {
            "normal"
        }
    );
    match node.text_style.list {
        ListStyle::None=>view!{<p style=style>{node.body}</p>}.into_any(),
        ListStyle::Bullet=>view!{<ul class="node-list" style=style>{node.body.lines().map(|line|view!{<li>{line.to_string()}</li>}).collect_view()}</ul>}.into_any(),
        ListStyle::Numbered=>view!{<ol class="node-list" style=style>{node.body.lines().map(|line|view!{<li>{line.to_string()}</li>}).collect_view()}</ol>}.into_any()
    }
}
#[component]
pub fn FormatToolbar() -> impl IntoView {
    let app = expect_context::<App>();
    let selected = Memo::new(move |_| {
        let ids = app.selection.get();
        app.board
            .get()
            .map(|c| {
                c.nodes
                    .into_iter()
                    .filter(|n| ids.contains(&n.id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    view! {<Show when=move||!selected.get().is_empty()&&app.can_edit()><div id="format-toolbar" class="format-toolbar" role="toolbar" aria-label="Selection formatting" tabindex="0">
        <label>"Color"<select aria-label="Selection color" prop:value=move||common(selected.get().iter().map(|n|n.color.clone())) on:change=move|e|app.format("color",&event_target_value(&e))><option value="" disabled>"Mixed"</option>{["butter","lavender","mint","peach","sky","paper"].map(|c|view!{<option value=c>{c}</option>}).collect_view()}</select></label>
        <label>"Size"<select aria-label="Font size" prop:value=move||common(selected.get().iter().map(|n|n.text_style.font_size.to_string())) on:change=move|e|app.format("size",&event_target_value(&e))><option value="" disabled>"Mixed"</option>{[8,10,12,14,16,20,24,32,48,72].map(|n|view!{<option value=n.to_string()>{n}</option>}).collect_view()}</select></label>
        <button aria-label="Bold" aria-pressed=move||tri_state(selected.get().iter().map(|n|n.text_style.bold)) on:click=move|_|app.format("bold","")><strong>"B"</strong></button>
        <button aria-label="Italic" aria-pressed=move||tri_state(selected.get().iter().map(|n|n.text_style.italic)) on:click=move|_|app.format("italic","")><em>"I"</em></button>
        <label>"List"<select aria-label="List style" prop:value=move||common(selected.get().iter().map(|n|match n.text_style.list{ListStyle::None=>"none",ListStyle::Bullet=>"bullet",ListStyle::Numbered=>"numbered"}.to_string())) on:change=move|e|app.format("list",&event_target_value(&e))><option value="" disabled>"Mixed"</option><option value="none">"None"</option><option value="bullet">"Bullets"</option><option value="numbered">"Numbers"</option></select></label>
        <button aria-label="Bring to front" title="Bring to front" on:click=move|_|app.layer(true)>"↑"</button><button aria-label="Send to back" title="Send to back" on:click=move|_|app.layer(false)>"↓"</button>
        <button aria-label="Group selection" title="Group selection" on:click=move|_|app.group()><Icon name="frame"/></button>
    </div></Show>}
}
fn common(mut values: impl Iterator<Item = String>) -> String {
    let first = values.next().unwrap_or_default();
    if values.all(|v| v == first) {
        first
    } else {
        String::new()
    }
}
fn tri_state(values: impl Iterator<Item = bool>) -> &'static str {
    let v = values.collect::<Vec<_>>();
    if v.iter().all(|v| *v) {
        "true"
    } else if v.iter().all(|v| !*v) {
        "false"
    } else {
        "mixed"
    }
}

#[component]
pub fn NodeOptions() -> impl IntoView {
    let app = expect_context::<App>();
    view! {
        {move||app.selected().map(|n|{let id=StoredValue::new(n.id.clone());let current_kind=serde_json::to_string(&n.kind).unwrap().trim_matches('"').to_string();let parent=n.parent_id.clone();let containers=app.board.get().map(|c|{let descendants=c.descendants(&n.id);c.nodes.into_iter().filter(|p|p.kind.is_container()&&!descendants.contains(&p.id)).collect::<Vec<_>>()}).unwrap_or_default();view!{<fieldset disabled=move||!app.can_edit() class="form extra-options"><label for="node-kind">"Object type"</label><select id="node-kind" prop:value=current_kind on:change=move|e|{if let Ok(kind)=serde_json::from_value(serde_json::Value::String(event_target_value(&e))){app.patch(id.get_value(),NodePatch{kind:Some(kind),..Default::default()});}}>{["note","step","rectangle","rounded_rectangle","oval","decision","text","frame","swimlane","milestone","image","video","sketch","line"].map(|kind|view!{<option value=kind>{kind.replace('_'," ")}</option>}).collect_view()}</select><label for="node-parent">"Container"</label><select id="node-parent" prop:value=parent on:change=move|e|app.patch(id.get_value(),NodePatch{parent_id:Some(event_target_value(&e)),..Default::default()})><option value="">"No container"</option>{containers.into_iter().map(|p|view!{<option value=p.id>{p.title}</option>}).collect_view()}</select>
            {matches!(n.kind,NodeKind::Sketch|NodeKind::Line).then(||view!{<label for="stroke-width">"Stroke width"</label><input id="stroke-width" type="number" min="1" max="24" step="1" prop:value=n.stroke_width on:change=move|e|if let Ok(width)=event_target_value(&e).parse(){app.patch(id.get_value(),NodePatch{stroke_width:Some(width),..Default::default()});}/>})}
            {n.kind.is_container().then(||view!{<button class="button" on:click=move|_|app.ungroup()>"Ungroup children"</button>})}
        </fieldset>}})}
    }
}

#[component]
pub fn EdgeInspector() -> impl IntoView {
    let app = expect_context::<App>();
    view! {{move||app.board.get().and_then(|c|c.edges.into_iter().find(|e|e.id==app.selected_edge.get())).map(|e|{let id=StoredValue::new(e.id);view!{<fieldset disabled=move||!app.can_edit() class="form edge-options"><h3>"Dependency connector"</h3><label for="edge-label">"Connector label"</label><input id="edge-label" prop:value=e.label on:change=move|ev|app.edit(vec![Operation::UpdateEdge{id:id.get_value(),changes:EdgePatch{label:Some(event_target_value(&ev)),..Default::default()}}])/>
    <label for="edge-route">"Routing"</label><select id="edge-route" prop:value=if e.routing==Routing::Orthogonal{"orthogonal"}else{"curved"} on:change=move|ev|app.edit(vec![Operation::UpdateEdge{id:id.get_value(),changes:EdgePatch{routing:Some(if event_target_value(&ev)=="orthogonal"{Routing::Orthogonal}else{Routing::Curved}),..Default::default()}}])><option value="curved">"Curved"</option><option value="orthogonal">"Orthogonal"</option></select>
    <label><input type="checkbox" prop:checked=e.arrow on:change=move|ev|app.edit(vec![Operation::UpdateEdge{id:id.get_value(),changes:EdgePatch{arrow:Some(event_target_checked(&ev)),..Default::default()}}])/>"Arrowhead"</label><button class="button danger" on:click=move|_|app.delete()>"Delete connector"</button></fieldset>}})}}
}
