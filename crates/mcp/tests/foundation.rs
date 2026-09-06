use bonaparte_mcp::{session::McpSession, tools::execute_tool};
use bonaparte_model::*;
use serde_json::{json, Value};
#[test]
fn mcp_discovers_manifests_and_applies_the_same_persistent_effect_stack() {
    let mut session = McpSession::new();
    let catalog = execute_tool(&mut session, "effects.list", json!({}));
    assert!(!catalog.is_error);
    let catalog: Value = serde_json::from_str(&catalog.content[0].text).unwrap();
    assert!(catalog["effects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["id"] == "builtin.color_grade"));
    let c = session.active_comp.unwrap();
    let mut layer = Layer::new_solid("Graded", [0.4, 0.3, 0.2, 1.0], Time::ZERO, Time(120000));
    let mut grade = EffectInstance::new("grade", "builtin.color_grade");
    grade
        .params
        .insert("exposure".into(), EffectValue::Float(1.0));
    layer.effects.push(grade);
    assert!(
        !execute_tool(
            &mut session,
            "op.apply",
            json!({"op":Op::AddLayer{comp:c,layer}})
        )
        .is_error
    );
    let saved = execute_tool(&mut session, "project.save", json!({}));
    let value: Value = serde_json::from_str(&saved.content[0].text).unwrap();
    assert!(!execute_tool(&mut session, "project.open", json!({"json":value["json"]})).is_error);
    assert_eq!(
        session
            .project
            .comp(c)
            .unwrap()
            .layers
            .values()
            .next()
            .unwrap()
            .effects
            .len(),
        1
    );
}
#[test]
fn mcp_bad_effect_proposals_and_requested_missing_comp_fail_explicitly() {
    let mut session = McpSession::new();
    let c = session.active_comp.unwrap();
    let mut layer = Layer::new_solid("Invalid", [1.0; 4], Time::ZERO, Time(120000));
    layer
        .effects
        .push(EffectInstance::new("bad", "missing.plugin"));
    let result = execute_tool(
        &mut session,
        "ops.propose",
        json!({"ops":[Op::AddLayer{comp:c,layer}]}),
    );
    let result: Value = serde_json::from_str(&result.content[0].text).unwrap();
    assert_eq!(result["valid"], false);
    assert!(session.project.comp(c).unwrap().layers.is_empty());
    assert_eq!(session.target_comp(Some(9999)), None);
}
