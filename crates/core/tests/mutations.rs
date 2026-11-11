use pinboard_core::*;

fn fixture() -> Canvas {
    let mut c = Canvas::new("Planning");
    c.nodes = vec![Node::note("Research", 0., 0.), Node::note("Ship", 300., 0.)];
    c.edges = vec![Edge {
        id: new_id(),
        from: c.nodes[0].id.clone(),
        to: c.nodes[1].id.clone(),
        label: "then".into(),
        routing: Routing::Curved,
        arrow: true,
    }];
    c
}
fn batch(c: &Canvas, operations: Vec<Operation>) -> Mutation {
    Mutation {
        base_revision: c.revision,
        scope: None,
        operations,
    }
}

#[test]
fn invalid_later_operation_rolls_back_entire_batch() {
    let mut c = fixture();
    let before = serde_json::to_value(&c).unwrap();
    let mut invalid = Node::note("Invalid", 0., 0.);
    invalid.width = -1.;
    let b = batch(
        &c,
        vec![
            Operation::Rename {
                title: "Changed".into(),
            },
            Operation::AddNode { node: invalid },
        ],
    );
    assert!(c.apply(&b).is_err());
    assert_eq!(serde_json::to_value(c).unwrap(), before);
}
#[test]
fn stale_revision_never_overwrites_a_newer_edit() {
    let mut c = fixture();
    let stale = batch(
        &c,
        vec![Operation::Rename {
            title: "Stale".into(),
        }],
    );
    let fresh = batch(
        &c,
        vec![Operation::Rename {
            title: "Fresh".into(),
        }],
    );
    c.apply(&fresh).unwrap();
    assert!(matches!(
        c.apply(&stale),
        Err(CanvasError::Conflict { actual: 1, .. })
    ));
    assert_eq!(c.title, "Fresh");
}
#[test]
fn scope_blocks_indirect_connector_damage() {
    let mut c = fixture();
    let id = c.nodes[0].id.clone();
    let before = serde_json::to_value(&c).unwrap();
    let b = Mutation {
        base_revision: 0,
        scope: Some(vec![id.clone()]),
        operations: vec![Operation::DeleteNode { id }],
    };
    assert!(matches!(c.apply(&b), Err(CanvasError::Scope(_))));
    assert_eq!(serde_json::to_value(c).unwrap(), before);
}
#[test]
fn scope_allows_selected_node_edit_but_nothing_else() {
    let mut c = fixture();
    let a = c.nodes[0].id.clone();
    let b = c.nodes[1].id.clone();
    let patch = NodePatch {
        body: Some("Review interviews".into()),
        ..Default::default()
    };
    c.apply(&Mutation {
        base_revision: 0,
        scope: Some(vec![a.clone()]),
        operations: vec![Operation::UpdateNode {
            id: a.clone(),
            changes: patch.clone(),
        }],
    })
    .unwrap();
    let outside = Mutation {
        base_revision: 1,
        scope: Some(vec![a]),
        operations: vec![Operation::UpdateNode {
            id: b,
            changes: patch,
        }],
    };
    assert!(matches!(c.apply(&outside), Err(CanvasError::Scope(_))));
    assert_eq!(c.nodes[1].body, "");
}
#[test]
fn empty_scope_cannot_rename_or_add_nodes() {
    for op in [
        Operation::Rename {
            title: "Oops".into(),
        },
        Operation::AddNode {
            node: Node::note("Oops", 0., 0.),
        },
    ] {
        let mut c = fixture();
        let b = Mutation {
            base_revision: 0,
            scope: Some(vec![]),
            operations: vec![op],
        };
        assert!(c.apply(&b).is_err());
        assert_eq!(c.revision, 0);
    }
}
#[test]
fn deleting_a_node_removes_its_edges_atomically() {
    let mut c = fixture();
    let b = batch(
        &c,
        vec![Operation::DeleteNode {
            id: c.nodes[0].id.clone(),
        }],
    );
    c.apply(&b).unwrap();
    assert_eq!(c.nodes.len(), 1);
    assert!(c.edges.is_empty());
    assert_eq!(c.revision, 1);
}
#[test]
fn rejects_dangling_edges_duplicate_ids_and_unsafe_media() {
    let c = fixture();
    let mut duplicate = c.clone();
    duplicate.nodes.push(duplicate.nodes[0].clone());
    assert!(duplicate.validate().is_err());
    let mut dangling = c.clone();
    dangling.edges[0].to = "missing".into();
    assert!(dangling.validate().is_err());
    for url in [
        "javascript:alert(1)",
        "data:text/html,<script>",
        "file:///etc/passwd",
    ] {
        let mut invalid = c.clone();
        invalid.nodes[0].url = url.into();
        assert!(invalid.validate().is_err());
    }
    let mut nan = c;
    nan.nodes[0].x = f64::NAN;
    assert!(nan.validate().is_err());
}
#[test]
fn rejects_unknown_mutation_fields() {
    assert!(serde_json::from_value::<Mutation>(serde_json::json!({"base_revision":0,"operations":[{"op":"rename","title":"X","extra":"typo"}]})).is_err());
    assert!(serde_json::from_value::<NodePatch>(serde_json::json!({"unknown":10})).is_err());
}
#[test]
fn welcome_canvas_and_selection_bounds_are_valid() {
    let c = welcome_canvas();
    c.validate().unwrap();
    let n = &c.nodes[0];
    assert!(n.intersects(&Region {
        x: n.x - 1.,
        y: n.y - 1.,
        width: 2.,
        height: 2.
    }));
    assert!(!n.intersects(&Region {
        x: n.x + n.width + 1.,
        y: n.y,
        width: 20.,
        height: 20.
    }));
}
