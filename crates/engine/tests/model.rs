use engine::{EngineModel, EngineRuntime, Value};

#[test]
fn parses_actions_behaviors_and_runs_sequence() {
    let actions = r#"<actions><Action name="Greet" type="Sequence"><Action type="Offset" dx="2" dy="-1"/><Action type="Stay" duration="20"/></Action></actions>"#;
    let behaviors = r#"<behaviors><Behavior name="Idle" frequency="1"><Action name="Greet"/></Behavior></behaviors>"#;
    let model = EngineModel::from_xml(actions, behaviors).unwrap();
    assert_eq!(model.behaviors()[0].name, "Idle");
    let mut runtime = EngineRuntime::new(model);
    runtime.spawn("Idle", 0.0, 0.0).unwrap();
    runtime.tick(1);
    assert_eq!(runtime.mascots()[0].position(), (2.0, -1.0));
}

#[test]
fn self_destruct_marks_mascot_dead_after_tick() {
    let model = EngineModel::from_xml(
        r#"<actions><Action name="Gone" type="SelfDestruct"/></actions>"#,
        r#"<behaviors><Behavior name="Bye"><Action name="Gone"/></Behavior></behaviors>"#,
    )
    .unwrap();
    let mut runtime = EngineRuntime::new(model);
    runtime.spawn("Bye", 0.0, 0.0).unwrap();
    runtime.tick(1);
    assert!(runtime.mascots()[0].is_dead());
}

#[test]
fn expression_condition_controls_behavior() {
    let model = EngineModel::from_xml(
        r#"<actions><Action name="Move" type="Offset" dx="1" dy="0"/></actions>"#,
        r#"<behaviors><Behavior name="Conditional" condition="enabled"><Action name="Move"/></Behavior></behaviors>"#,
    )
    .unwrap();
    let mut runtime = EngineRuntime::new(model);
    let id = runtime.spawn("Conditional", 0.0, 0.0).unwrap();
    runtime
        .set_variable(id, "enabled", Value::Bool(false))
        .unwrap();
    runtime.tick(1);
    assert_eq!(runtime.mascots()[0].position(), (0.0, 0.0));
}
