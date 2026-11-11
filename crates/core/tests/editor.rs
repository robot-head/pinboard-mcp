use pinboard_core::*;
fn edit(c: &mut Canvas, operations: Vec<Operation>) {
    c.apply(&Mutation {
        base_revision: c.revision,
        scope: None,
        operations,
    })
    .unwrap();
}
fn fixture() -> Canvas {
    let mut c = Canvas::new("Editor");
    c.nodes = vec![Node::note("A", 0., 0.), Node::note("B", 500., 0.)];
    c
}
#[test]
fn old_documents_load_with_compatible_defaults() {
    let value = serde_json::json!({"id":"old","kind":"note","title":"Legacy","x":0,"y":0,"width":220,"height":160});
    let n: Node = serde_json::from_value(value).unwrap();
    assert_eq!(n.stroke_width, 3.);
    assert!(n.parent_id.is_empty());
    assert_eq!(n.text_style, TextStyle::default());
}
#[test]
fn container_cycles_and_noncontainers_are_rejected() {
    let mut c = fixture();
    c.nodes[0].parent_id = c.nodes[1].id.clone();
    assert!(c.validate().is_err());
    c.nodes[0].kind = NodeKind::Frame;
    c.nodes[1].kind = NodeKind::Frame;
    c.nodes[1].parent_id = c.nodes[0].id.clone();
    assert!(c.validate().is_err());
}
#[test]
fn scoped_frame_deletion_cannot_detach_unselected_children() {
    let mut c = fixture();
    c.nodes[0].kind = NodeKind::Frame;
    c.nodes[1].parent_id = c.nodes[0].id.clone();
    let before = serde_json::to_value(&c).unwrap();
    let id = c.nodes[0].id.clone();
    assert!(
        c.apply(&Mutation {
            base_revision: 0,
            scope: Some(vec![id.clone()]),
            operations: vec![Operation::DeleteNode { id }]
        })
        .is_err()
    );
    assert_eq!(serde_json::to_value(c).unwrap(), before);
}
#[test]
fn scoped_reparenting_cannot_change_an_unselected_container() {
    let mut c = fixture();
    c.nodes[0].kind = NodeKind::Frame;
    let id = c.nodes[1].id.clone();
    assert!(
        c.apply(&Mutation {
            base_revision: 0,
            scope: Some(vec![id.clone()]),
            operations: vec![Operation::UpdateNode {
                id,
                changes: NodePatch {
                    parent_id: Some(c.nodes[0].id.clone()),
                    ..Default::default()
                }
            }]
        })
        .is_err()
    );
}
#[test]
fn gap_suggestions_preserve_dependencies_until_accepted() {
    let mut c = fixture();
    let a = c.nodes[0].id.clone();
    let b = c.nodes[1].id.clone();
    c.edges.push(Edge {
        id: new_id(),
        from: a.clone(),
        to: b.clone(),
        label: "existing".into(),
        routing: Routing::Orthogonal,
        arrow: true,
    });
    let proposal = c.propose_gap(&a, &b, "Review", "Check quality").unwrap();
    let id = proposal.id.clone();
    edit(&mut c, vec![Operation::AddNode { node: proposal }]);
    assert_eq!(c.edges.len(), 1);
    assert!(c.nodes[2].suggested);
    let before = c.clone();
    edit(&mut c, vec![Operation::AcceptSuggestion { id: id.clone() }]);
    assert_eq!(c.edges.len(), 2);
    assert!(c.edges.iter().any(|e| e.from == a && e.to == id));
    assert!(c.edges.iter().any(|e| e.from == id && e.to == b));
    assert!(!c.nodes[2].suggested);
    let after = c.clone();
    let ops = inverse_operations(&c, &before, &after).unwrap();
    edit(&mut c, ops);
    assert!(c.nodes[2].suggested);
    assert_eq!(c.edges.len(), 1);
}
#[test]
fn undo_preserves_unrelated_collaborator_fields() {
    let mut c = fixture();
    let id = c.nodes[0].id.clone();
    let before = c.clone();
    edit(
        &mut c,
        vec![Operation::UpdateNode {
            id: id.clone(),
            changes: NodePatch {
                title: Some("My edit".into()),
                ..Default::default()
            },
        }],
    );
    let after = c.clone();
    edit(
        &mut c,
        vec![Operation::UpdateNode {
            id: id.clone(),
            changes: NodePatch {
                body: Some("Teammate's detail".into()),
                ..Default::default()
            },
        }],
    );
    let (before, after) = compact_history(before, after);
    assert_eq!(before.nodes.len(), 1);
    let ops = inverse_operations(&c, &before, &after).unwrap();
    edit(&mut c, ops);
    assert_eq!(c.nodes[0].title, "A");
    assert_eq!(c.nodes[0].body, "Teammate's detail");
}
#[test]
fn undo_rejects_same_field_changes_and_new_dependencies() {
    let mut c = fixture();
    let id = c.nodes[0].id.clone();
    let before = c.clone();
    edit(
        &mut c,
        vec![Operation::UpdateNode {
            id: id.clone(),
            changes: NodePatch {
                title: Some("Mine".into()),
                ..Default::default()
            },
        }],
    );
    let after = c.clone();
    edit(
        &mut c,
        vec![Operation::UpdateNode {
            id,
            changes: NodePatch {
                title: Some("Theirs".into()),
                ..Default::default()
            },
        }],
    );
    assert!(inverse_operations(&c, &before, &after).is_err());
    let before = c.clone();
    let n = Node::note("New", 100., 300.);
    let new_id = n.id.clone();
    edit(&mut c, vec![Operation::AddNode { node: n }]);
    let after = c.clone();
    let a = c.nodes[0].id.clone();
    edit(
        &mut c,
        vec![Operation::AddEdge {
            edge: Edge {
                id: pinboard_core::new_id(),
                from: a,
                to: new_id,
                label: "External dependency".into(),
                routing: Routing::Curved,
                arrow: true,
            },
        }],
    );
    assert!(inverse_operations(&c, &before, &after).is_err());
}
#[test]
fn routing_anchors_to_oval_and_diamond_boundaries() {
    let mut a = Node::note("Oval", 0., 0.);
    a.kind = NodeKind::Oval;
    a.width = 200.;
    a.height = 100.;
    let mut b = Node::note("Diamond", 300., 200.);
    b.kind = NodeKind::Decision;
    b.width = 200.;
    b.height = 100.;
    let (p, q) = connector_endpoints(&a, &b);
    let ellipse = ((p.x - 100.) / 100.).powi(2) + ((p.y - 50.) / 50.).powi(2);
    let diamond = ((q.x - 400.) / 100.).abs() + ((q.y - 250.) / 50.).abs();
    assert!((ellipse - 1.).abs() < 1e-8);
    assert!((diamond - 1.).abs() < 1e-8);
    assert!(connector_path(&a, &b, &Routing::Orthogonal).contains('H'));
    assert!(connector_path(&a, &b, &Routing::Curved).contains('C'));
}
#[test]
fn malformed_formatting_and_extreme_layers_are_rejected() {
    for patch in [
        NodePatch {
            z_index: Some(i32::MIN),
            ..Default::default()
        },
        NodePatch {
            stroke_width: Some(0.),
            ..Default::default()
        },
        NodePatch {
            text_style: Some(TextStyle {
                font_size: f64::NAN,
                ..Default::default()
            }),
            ..Default::default()
        },
    ] {
        let mut c = fixture();
        let id = c.nodes[0].id.clone();
        assert!(
            c.apply(&Mutation {
                base_revision: 0,
                scope: None,
                operations: vec![Operation::UpdateNode { id, changes: patch }]
            })
            .is_err()
        );
    }
}
