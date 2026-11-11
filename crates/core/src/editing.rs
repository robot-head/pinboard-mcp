use crate::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Routing {
    #[default]
    Curved,
    Orthogonal,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EdgePatch {
    pub label: Option<String>,
    pub routing: Option<Routing>,
    pub arrow: Option<bool>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ListStyle {
    #[default]
    None,
    Bullet,
    Numbered,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct TextStyle {
    pub font_size: f64,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub list: ListStyle,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 12.,
            bold: false,
            italic: false,
            list: ListStyle::None,
        }
    }
}
impl NodeKind {
    pub fn is_container(&self) -> bool {
        matches!(self, Self::Frame | Self::Swimlane)
    }
}

pub fn ink_color(color: &str) -> &'static str {
    match color {
        "butter" => "#8b741c",
        "lavender" => "#795990",
        "mint" => "#3c7049",
        "peach" => "#ab603e",
        "sky" => "#3d7597",
        _ => "#345341",
    }
}

impl Canvas {
    pub fn descendants(&self, id: &str) -> Vec<String> {
        let mut ids = vec![id.to_string()];
        let mut cursor = 0;
        while cursor < ids.len() {
            let parent = ids[cursor].clone();
            for n in self.nodes.iter().filter(|n| n.parent_id == parent) {
                if !ids.contains(&n.id) {
                    ids.push(n.id.clone());
                }
            }
            cursor += 1;
        }
        ids
    }
    pub fn expanded_selection(&self, ids: &[String]) -> Vec<String> {
        let mut out = vec![];
        for id in ids {
            for child in self.descendants(id) {
                if !out.contains(&child) {
                    out.push(child);
                }
            }
        }
        out
    }
    pub fn propose_gap(
        &self,
        from: &str,
        to: &str,
        title: &str,
        body: &str,
    ) -> Result<Node, CanvasError> {
        let a = self
            .nodes
            .iter()
            .find(|n| n.id == from)
            .ok_or_else(|| CanvasError::NotFound(from.into()))?;
        let b = self
            .nodes
            .iter()
            .find(|n| n.id == to)
            .ok_or_else(|| CanvasError::NotFound(to.into()))?;
        if from == to {
            return Err(CanvasError::Invalid("Choose two different steps".into()));
        }
        let mut n = Node::note(
            title,
            (a.x + a.width / 2. + b.x + b.width / 2.) / 2. - 110.,
            (a.y + a.height / 2. + b.y + b.height / 2.) / 2. - 70.,
        );
        n.kind = NodeKind::Step;
        n.height = 140.;
        n.body = body.into();
        n.color = "lavender".into();
        n.suggested = true;
        n.suggestion_from = from.into();
        n.suggestion_to = to.into();
        Ok(n)
    }
}

/// Compute exact connector intersections with the visual shape's boundary.
pub fn connector_endpoints(a: &Node, b: &Node) -> (Point, Point) {
    fn boundary(n: &Node, target: &Point) -> Point {
        let cx = n.x + n.width / 2.;
        let cy = n.y + n.height / 2.;
        let dx = target.x - cx;
        let dy = target.y - cy;
        if dx.abs() + dy.abs() < 0.0001 {
            return Point {
                x: cx + n.width / 2.,
                y: cy,
            };
        }
        let rx = n.width / 2.;
        let ry = n.height / 2.;
        let t = match n.kind {
            NodeKind::Oval => 1. / ((dx / rx).powi(2) + (dy / ry).powi(2)).sqrt(),
            NodeKind::Decision => 1. / (dx.abs() / rx + dy.abs() / ry),
            _ => 1. / (dx.abs() / rx).max(dy.abs() / ry),
        };
        Point {
            x: cx + dx * t,
            y: cy + dy * t,
        }
    }
    let ac = Point {
        x: a.x + a.width / 2.,
        y: a.y + a.height / 2.,
    };
    let bc = Point {
        x: b.x + b.width / 2.,
        y: b.y + b.height / 2.,
    };
    (boundary(a, &bc), boundary(b, &ac))
}
pub fn connector_path(a: &Node, b: &Node, routing: &Routing) -> String {
    let (p, q) = connector_endpoints(a, b);
    match routing {
        Routing::Curved => {
            let dx = (q.x - p.x).abs() / 2.;
            if (q.x - p.x).abs() >= (q.y - p.y).abs() {
                let sign = if q.x >= p.x { 1. } else { -1. };
                format!(
                    "M{},{} C{},{} {},{} {},{}",
                    p.x,
                    p.y,
                    p.x + dx * sign,
                    p.y,
                    q.x - dx * sign,
                    q.y,
                    q.x,
                    q.y
                )
            } else {
                let dy = (q.y - p.y).abs() / 2.;
                let sign = if q.y >= p.y { 1. } else { -1. };
                format!(
                    "M{},{} C{},{} {},{} {},{}",
                    p.x,
                    p.y,
                    p.x,
                    p.y + dy * sign,
                    q.x,
                    q.y - dy * sign,
                    q.x,
                    q.y
                )
            }
        }
        Routing::Orthogonal => {
            let mid = (p.x + q.x) / 2.;
            format!("M{},{} H{} V{} H{}", p.x, p.y, mid, q.y, q.x)
        }
    }
}

/// An author-local compensating edit, preserving fields changed by other authors.
/// Refuse to undo if someone changed the same field or attached new dependencies.
pub fn inverse_operations(
    current: &Canvas,
    before: &Canvas,
    after: &Canvas,
) -> Result<Vec<Operation>, CanvasError> {
    let conflict = || {
        CanvasError::Invalid("Cannot undo: a collaborator changed an affected field or dependency. Newer work was preserved.".into())
    };
    let mut ops = vec![];
    for e in &after.edges {
        if !before.edges.contains(e) {
            if !current.edges.contains(e) {
                return Err(conflict());
            }
            ops.push(Operation::DeleteEdge { id: e.id.clone() });
        }
    }
    for n in &after.nodes {
        if !before.nodes.iter().any(|b| b.id == n.id) {
            if current.nodes.iter().find(|c| c.id == n.id) != Some(n)
                || current
                    .edges
                    .iter()
                    .any(|e| (e.from == n.id || e.to == n.id) && !after.edges.contains(e))
                || current
                    .nodes
                    .iter()
                    .any(|c| c.parent_id == n.id && !after.nodes.contains(c))
            {
                return Err(conflict());
            }
            ops.push(Operation::DeleteNode { id: n.id.clone() });
        }
    }
    for n in &before.nodes {
        if let Some(new) = after.nodes.iter().find(|a| a.id == n.id) {
            let cur = current
                .nodes
                .iter()
                .find(|c| c.id == n.id)
                .ok_or_else(conflict)?;
            let old = serde_json::to_value(n).unwrap();
            let new = serde_json::to_value(new).unwrap();
            let cur = serde_json::to_value(cur).unwrap();
            let mut patch = serde_json::Map::new();
            for (key, value) in old.as_object().unwrap() {
                if key != "id" && new[key] != *value {
                    if cur[key] != new[key] {
                        return Err(conflict());
                    }
                    patch.insert(key.clone(), value.clone());
                }
            }
            if !patch.is_empty() {
                ops.push(Operation::UpdateNode {
                    id: n.id.clone(),
                    changes: serde_json::from_value(serde_json::Value::Object(patch))
                        .map_err(|_| conflict())?,
                });
            }
        } else {
            if current.nodes.iter().any(|c| c.id == n.id) {
                return Err(conflict());
            }
            ops.push(Operation::AddNode { node: n.clone() });
        }
    }
    for e in &before.edges {
        if !after.edges.contains(e) {
            if current
                .edges
                .iter()
                .any(|c| c.id == e.id && !after.edges.contains(c))
            {
                return Err(conflict());
            }
            ops.push(Operation::AddEdge { edge: e.clone() });
        }
    }
    if before.title != after.title {
        if current.title != after.title {
            return Err(conflict());
        }
        ops.push(Operation::Rename {
            title: before.title.clone(),
        });
    }
    // Restoring an accepted proposal must set its flags/anchors together, which is
    // permitted; clearing a proposal still uses the explicit acceptance operation.
    Ok(ops)
}

/// Keep undo history proportional to the changed objects rather than board size.
pub fn compact_history(mut before: Canvas, mut after: Canvas) -> (Canvas, Canvas) {
    let changed_nodes = before
        .nodes
        .iter()
        .filter(|n| !after.nodes.contains(n))
        .chain(after.nodes.iter().filter(|n| !before.nodes.contains(n)))
        .map(|n| n.id.clone())
        .collect::<HashSet<_>>();
    let changed_edges = before
        .edges
        .iter()
        .filter(|e| !after.edges.contains(e))
        .chain(after.edges.iter().filter(|e| !before.edges.contains(e)))
        .map(|e| e.id.clone())
        .collect::<HashSet<_>>();
    before.nodes.retain(|n| changed_nodes.contains(&n.id));
    after.nodes.retain(|n| changed_nodes.contains(&n.id));
    before.edges.retain(|e| changed_edges.contains(&e.id));
    after.edges.retain(|e| changed_edges.contains(&e.id));
    (before, after)
}
