//! Shared, transport-independent canvas model and atomic mutation rules.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
mod editing;
pub use editing::*;
mod collaboration;
pub use collaboration::*;

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    #[default]
    Note,
    Step,
    Rectangle,
    RoundedRectangle,
    Oval,
    Milestone,
    Swimlane,
    Line,
    Decision,
    Text,
    Frame,
    Image,
    Video,
    Sketch,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub due: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub points: Vec<Point>,
    #[serde(default)]
    pub suggested: bool,
    #[serde(default)]
    pub parent_id: String,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default = "default_stroke_width")]
    pub stroke_width: f64,
    #[serde(default)]
    pub text_style: TextStyle,
    #[serde(default)]
    pub suggestion_from: String,
    #[serde(default)]
    pub suggestion_to: String,
}
fn default_stroke_width() -> f64 {
    3.
}
fn default_arrow() -> bool {
    true
}
fn default_color() -> String {
    "butter".into()
}
impl Node {
    pub fn note(title: impl Into<String>, x: f64, y: f64) -> Self {
        Self {
            id: new_id(),
            kind: NodeKind::Note,
            x,
            y,
            width: 220.,
            height: 160.,
            title: title.into(),
            body: String::new(),
            color: default_color(),
            owner: String::new(),
            due: String::new(),
            group: String::new(),
            url: String::new(),
            points: vec![],
            suggested: false,
            parent_id: String::new(),
            z_index: 0,
            stroke_width: 3.,
            text_style: TextStyle::default(),
            suggestion_from: String::new(),
            suggestion_to: String::new(),
        }
    }
    pub fn intersects(&self, r: &Region) -> bool {
        self.x < r.x + r.width
            && self.x + self.width > r.x
            && self.y < r.y + r.height
            && self.y + self.height > r.y
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Edge {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub routing: Routing,
    #[serde(default = "default_arrow")]
    pub arrow: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Canvas {
    pub id: String,
    pub title: String,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}
impl Canvas {
    pub fn new(title: impl Into<String>) -> Self {
        let time = now();
        Self {
            id: new_id(),
            title: title.into(),
            revision: 0,
            created_at: time.clone(),
            updated_at: time,
            nodes: vec![],
            edges: vec![],
        }
    }
    pub fn summary(&self) -> CanvasSummary {
        CanvasSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            revision: self.revision,
            updated_at: self.updated_at.clone(),
            node_count: self.nodes.len(),
        }
    }
    /// Work on a copy so an invalid operation never leaves a partial change.
    pub fn apply(&mut self, batch: &Mutation) -> Result<(), CanvasError> {
        if batch.base_revision != self.revision {
            return Err(CanvasError::Conflict {
                expected: batch.base_revision,
                actual: self.revision,
            });
        }
        if batch.operations.is_empty() || batch.operations.len() > 500 {
            return Err(CanvasError::Invalid("Provide 1–500 operations".into()));
        }
        let mut next = self.clone();
        for op in &batch.operations {
            next.apply_one(op, batch.scope.as_deref())?;
        }
        next.validate()?;
        next.revision += 1;
        next.updated_at = now();
        *self = next;
        Ok(())
    }
    fn apply_one(&mut self, op: &Operation, scope: Option<&[String]>) -> Result<(), CanvasError> {
        let allowed = |id: &str| -> Result<(), CanvasError> {
            if scope.is_some_and(|s| !s.iter().any(|v| v == id)) {
                Err(CanvasError::Scope(id.into()))
            } else {
                Ok(())
            }
        };
        match op {
            Operation::AddNode { node } => {
                if scope.is_some() {
                    return Err(CanvasError::Invalid("Scoped edits may only change existing selected nodes. Add suggestions in an unscoped batch after user review.".into()));
                }
                self.nodes.push(node.clone());
            }
            Operation::UpdateNode { id, changes } => {
                allowed(id)?;
                let n = self
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == *id)
                    .ok_or_else(|| CanvasError::NotFound(id.clone()))?;
                if let Some(parent) = &changes.parent_id {
                    if !n.parent_id.is_empty() {
                        allowed(&n.parent_id)?;
                    }
                    if !parent.is_empty() {
                        allowed(parent)?;
                    }
                }
                if changes.suggested == Some(false) && n.suggested && !n.suggestion_from.is_empty()
                {
                    return Err(CanvasError::Invalid(
                        "Use accept_suggestion to approve and reconnect a gap proposal".into(),
                    ));
                }
                changes.apply(n);
            }
            Operation::DeleteNode { id } => {
                allowed(id)?;
                if !self.nodes.iter().any(|n| n.id == *id) {
                    return Err(CanvasError::NotFound(id.clone()));
                }
                for e in self.edges.iter().filter(|e| e.from == *id || e.to == *id) {
                    allowed(&e.from)?;
                    allowed(&e.to)?;
                }
                let parent = self
                    .nodes
                    .iter()
                    .find(|n| n.id == *id)
                    .unwrap()
                    .parent_id
                    .clone();
                if !parent.is_empty() {
                    allowed(&parent)?;
                }
                for child in self.nodes.iter().filter(|n| n.parent_id == *id) {
                    allowed(&child.id)?;
                }
                for child in self.nodes.iter_mut().filter(|n| n.parent_id == *id) {
                    child.parent_id = parent.clone();
                }
                self.nodes.retain(|n| n.id != *id);
                self.edges.retain(|e| e.from != *id && e.to != *id);
            }
            Operation::AddEdge { edge } => {
                allowed(&edge.from)?;
                allowed(&edge.to)?;
                self.edges.push(edge.clone());
            }
            Operation::DeleteEdge { id } => {
                let e = self
                    .edges
                    .iter()
                    .find(|e| e.id == *id)
                    .ok_or_else(|| CanvasError::NotFound(id.clone()))?;
                allowed(&e.from)?;
                allowed(&e.to)?;
                self.edges.retain(|e| e.id != *id);
            }
            Operation::UpdateEdge { id, changes } => {
                let edge = self
                    .edges
                    .iter_mut()
                    .find(|e| e.id == *id)
                    .ok_or_else(|| CanvasError::NotFound(id.clone()))?;
                allowed(&edge.from)?;
                allowed(&edge.to)?;
                if let Some(label) = &changes.label {
                    edge.label = label.clone();
                }
                if let Some(routing) = &changes.routing {
                    edge.routing = routing.clone();
                }
                if let Some(arrow) = changes.arrow {
                    edge.arrow = arrow;
                }
            }
            Operation::AcceptSuggestion { id } => {
                allowed(id)?;
                let n = self
                    .nodes
                    .iter()
                    .find(|n| n.id == *id)
                    .ok_or_else(|| CanvasError::NotFound(id.clone()))?
                    .clone();
                if !n.suggested {
                    return Err(CanvasError::Invalid(
                        "This node is not a pending suggestion".into(),
                    ));
                }
                if !n.suggestion_from.is_empty() {
                    allowed(&n.suggestion_from)?;
                    allowed(&n.suggestion_to)?;
                    self.edges
                        .retain(|e| !(e.from == n.suggestion_from && e.to == n.suggestion_to));
                    self.edges.extend([
                        Edge {
                            id: new_id(),
                            from: n.suggestion_from.clone(),
                            to: id.clone(),
                            label: String::new(),
                            routing: Routing::Curved,
                            arrow: true,
                        },
                        Edge {
                            id: new_id(),
                            from: id.clone(),
                            to: n.suggestion_to.clone(),
                            label: String::new(),
                            routing: Routing::Curved,
                            arrow: true,
                        },
                    ]);
                }
                let n = self.nodes.iter_mut().find(|n| n.id == *id).unwrap();
                n.suggested = false;
                n.suggestion_from.clear();
                n.suggestion_to.clear();
            }
            Operation::Rename { title } => {
                if scope.is_some() {
                    return Err(CanvasError::Scope(self.id.clone()));
                }
                self.title = title.clone();
            }
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), CanvasError> {
        let invalid = |s: &str| CanvasError::Invalid(s.into());
        if self.title.trim().is_empty() || self.title.len() > 200 {
            return Err(invalid("Title must contain 1–200 bytes"));
        }
        if self.nodes.len() > 3000 || self.edges.len() > 6000 {
            return Err(invalid("Canvas item limit exceeded"));
        }
        let mut ids = HashSet::new();
        for n in &self.nodes {
            if n.id.is_empty() || n.id.len() > 128 || !ids.insert(n.id.as_str()) {
                return Err(invalid(
                    "Node IDs must be unique and nonempty (max 128 bytes)",
                ));
            }
            if ![n.x, n.y, n.width, n.height]
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 100_000.)
                || n.width < 20.
                || n.height < 20.
            {
                return Err(invalid("Invalid node geometry"));
            }
            if n.title.len() > 1000
                || n.body.len() > 20000
                || n.owner.len() > 200
                || n.due.len() > 100
                || n.group.len() > 200
            {
                return Err(invalid("Node text limit exceeded"));
            }
            if !["butter", "lavender", "mint", "peach", "sky", "paper"].contains(&n.color.as_str())
            {
                return Err(invalid("Unknown node color"));
            }
            if !n.url.is_empty()
                && (!(n.url.starts_with("https://") || n.url.starts_with("http://"))
                    || n.url.len() > 4096)
            {
                return Err(invalid("Media requires an HTTP(S) URL"));
            }
            if n.points.len() > 10000
                || n.points.iter().any(|p| {
                    !p.x.is_finite()
                        || !p.y.is_finite()
                        || p.x.abs() > 100_000.
                        || p.y.abs() > 100_000.
                })
            {
                return Err(invalid("Invalid sketch points"));
            }
        }
        for n in &self.nodes {
            if !n.stroke_width.is_finite()
                || !(1. ..=24.).contains(&n.stroke_width)
                || n.z_index.unsigned_abs() > 100000
                || !n.text_style.font_size.is_finite()
                || !(8. ..=72.).contains(&n.text_style.font_size)
            {
                return Err(invalid("Invalid formatting or layer order"));
            }
            let mut parent = n.parent_id.as_str();
            let mut visited = HashSet::from([n.id.as_str()]);
            while !parent.is_empty() {
                if !visited.insert(parent) {
                    return Err(invalid("Container membership must not form a cycle"));
                }
                let p = self
                    .nodes
                    .iter()
                    .find(|p| p.id == parent)
                    .ok_or_else(|| invalid("Container not found"))?;
                if !p.kind.is_container() {
                    return Err(invalid("Parent must be a frame or swimlane"));
                }
                parent = &p.parent_id;
            }
            if (!n.suggestion_from.is_empty() || !n.suggestion_to.is_empty())
                && (!n.suggested
                    || n.suggestion_from == n.suggestion_to
                    || n.id == n.suggestion_from
                    || n.id == n.suggestion_to
                    || !ids.contains(n.suggestion_from.as_str())
                    || !ids.contains(n.suggestion_to.as_str()))
            {
                return Err(invalid(
                    "Suggestion anchors must refer to two existing steps",
                ));
            }
        }
        let mut edge_ids = HashSet::new();
        for e in &self.edges {
            if e.id.is_empty()
                || e.id.len() > 128
                || !edge_ids.insert(&e.id)
                || !ids.contains(e.from.as_str())
                || !ids.contains(e.to.as_str())
                || e.from == e.to
                || e.label.len() > 1000
            {
                return Err(invalid(
                    "Invalid connector: use unique IDs and distinct existing endpoints",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CanvasSummary {
    pub id: String,
    pub title: String,
    pub revision: u64,
    pub updated_at: String,
    pub node_count: usize,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodePatch {
    pub kind: Option<NodeKind>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub color: Option<String>,
    pub owner: Option<String>,
    pub due: Option<String>,
    pub group: Option<String>,
    pub url: Option<String>,
    pub points: Option<Vec<Point>>,
    pub suggested: Option<bool>,
    pub parent_id: Option<String>,
    pub z_index: Option<i32>,
    pub stroke_width: Option<f64>,
    pub text_style: Option<TextStyle>,
    pub suggestion_from: Option<String>,
    pub suggestion_to: Option<String>,
}
impl NodePatch {
    fn apply(&self, node: &mut Node) {
        macro_rules! set { ($($f:ident),*) => { $(if let Some(v) = &self.$f { node.$f = v.clone(); })* }; }
        set!(
            kind,
            x,
            y,
            width,
            height,
            title,
            body,
            color,
            owner,
            due,
            group,
            url,
            points,
            suggested,
            parent_id,
            z_index,
            stroke_width,
            text_style,
            suggestion_from,
            suggestion_to
        );
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    AddNode { node: Node },
    UpdateNode { id: String, changes: NodePatch },
    DeleteNode { id: String },
    AddEdge { edge: Edge },
    DeleteEdge { id: String },
    UpdateEdge { id: String, changes: EdgePatch },
    AcceptSuggestion { id: String },
    Rename { title: String },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mutation {
    pub base_revision: u64,
    #[serde(default)]
    pub scope: Option<Vec<String>>,
    pub operations: Vec<Operation>,
}
#[derive(Debug, thiserror::Error)]
pub enum CanvasError {
    #[error(
        "Revision conflict: requested {expected}, current {actual}. Read the latest canvas and rebase your changes."
    )]
    Conflict { expected: u64, actual: u64 },
    #[error("Outside selection: {0}")]
    Scope(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
}

/// A real sample board, persisted through the same path as user-created work.
pub fn welcome_canvas() -> Canvas {
    let mut c = Canvas::new("A little space to think big");
    let specs = [
        (
            "Start with a question",
            "What would you like to make clearer? A project, a decision, or the next big thing.",
            100.,
            160.,
            "butter",
            "01 · EXPLORE",
        ),
        (
            "Get it all out",
            "Drop your ideas here. Messy is a perfectly good place to start.",
            400.,
            160.,
            "lavender",
            "02 · CONNECT",
        ),
        (
            "Find the next step",
            "Connect the dots, choose a direction, and turn your thinking into action.",
            700.,
            160.,
            "mint",
            "03 · MAKE IT HAPPEN",
        ),
        (
            "A thinking partner, on call",
            "Connect Claude or Codex through MCP to map a plan, group ideas, or spot the gaps.",
            250.,
            430.,
            "paper",
            "WORK WITH AI",
        ),
        (
            "Make room for the unexpected",
            "Drag to arrange. Shift + drag to select a region. Use the toolbar to add your own ideas.",
            550.,
            430.,
            "peach",
            "YOUR CANVAS, YOUR RULES",
        ),
    ];
    for (title, body, x, y, color, group) in specs {
        let mut n = Node::note(title, x, y);
        n.body = body.into();
        n.color = color.into();
        n.group = group.into();
        n.width = 260.;
        n.height = 200.;
        c.nodes.push(n);
    }
    for i in 0..2 {
        c.edges.push(Edge {
            id: new_id(),
            from: c.nodes[i].id.clone(),
            to: c.nodes[i + 1].id.clone(),
            label: String::new(),
            routing: Routing::Curved,
            arrow: true,
        });
    }
    c
}

pub fn markdown(canvas: &Canvas) -> String {
    let mut out = format!("# {}\n\n", canvas.title);
    let mut nodes: Vec<_> = canvas.nodes.iter().collect();
    nodes.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    for n in nodes {
        out.push_str(&format!(
            "## {}{}\n\n{}\n\n",
            if n.suggested { "[Suggestion] " } else { "" },
            n.title,
            n.body
        ));
        if !n.owner.is_empty() {
            out.push_str(&format!("- Owner: {}\n", n.owner));
        }
        if !n.due.is_empty() {
            out.push_str(&format!("- Due: {}\n", n.due));
        }
        if !n.url.is_empty() {
            out.push_str(&format!("- Media: {}\n", n.url));
        }
        out.push('\n');
    }
    if !canvas.edges.is_empty() {
        out.push_str("## Connections\n\n");
        for e in &canvas.edges {
            let name = |id: &str| {
                canvas
                    .nodes
                    .iter()
                    .find(|n| n.id == id)
                    .map(|n| n.title.as_str())
                    .unwrap_or(id)
                    .to_string()
            };
            out.push_str(&format!(
                "- {} → {} {}\n",
                name(&e.from),
                name(&e.to),
                e.label
            ));
        }
    }
    out
}
