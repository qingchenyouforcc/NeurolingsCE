use api::{
    Anchor, ApiError, ApiRequest, CliLabel, Command, MascotCommandStatus, MascotInfo, MascotPatch,
    Selector, SpawnMascotRequest,
};
use serde_json::json;

#[test]
fn mascot_info_uses_stable_wire_names_and_null_optionals() {
    let info = MascotInfo {
        id: 35,
        data_id: 0,
        name: "Default".into(),
        active_behavior: None,
        label: None,
        anchor: Anchor::new(67.2, 225.63864462595598).unwrap(),
    };

    let encoded = serde_json::to_value(info).unwrap();

    assert_eq!(
        encoded,
        json!({
            "id": 35,
            "data_id": 0,
            "name": "Default",
            "active_behavior": null,
            "label": null,
            "anchor": {"x": 67.2, "y": 225.63864462595598}
        })
    );
}

#[test]
fn spawn_request_flattens_patch_fields() {
    let request = SpawnMascotRequest {
        name: Some("Default Mascot".into()),
        data_id: None,
        patch: MascotPatch {
            anchor: Some(Anchor::new(150.0, 150.0).unwrap()),
            behavior: Some("SplitIntoTwo".into()),
        },
    };

    assert_eq!(
        serde_json::to_value(request).unwrap(),
        json!({
            "name": "Default Mascot",
            "anchor": {"x": 150.0, "y": 150.0},
            "behavior": "SplitIntoTwo"
        })
    );
}

#[test]
fn selector_rejects_values_longer_than_protocol_limit() {
    let long_selector = "x".repeat(1025);

    let error = Selector::try_from(long_selector.as_str()).unwrap_err();

    assert_eq!(error, ApiError::selector_too_long());
}

#[test]
fn selector_round_trips_unicode_by_character_count() {
    let selector = Selector::try_from("名称 == '桌宠'").unwrap();

    assert_eq!(selector.as_str(), "名称 == '桌宠'");
    assert_eq!(
        serde_json::to_string(&selector).unwrap(),
        r#""名称 == '桌宠'""#
    );
}

#[test]
fn anchor_rejects_non_finite_coordinates() {
    assert_eq!(
        Anchor::new(f64::NAN, 0.0).unwrap_err(),
        ApiError::invalid_anchor()
    );
    assert_eq!(
        Anchor::new(0.0, f64::INFINITY).unwrap_err(),
        ApiError::invalid_anchor()
    );
}

#[test]
fn label_is_non_negative_and_serializes_as_number() {
    let label = CliLabel::try_from(3_i64).unwrap();

    assert_eq!(label.value(), 3);
    assert_eq!(serde_json::to_value(label).unwrap(), json!(3));
    assert_eq!(
        CliLabel::try_from(-1_i64).unwrap_err(),
        ApiError::invalid_label()
    );
}

#[test]
fn command_and_request_keep_unknown_commands_for_dispatch_validation() {
    let request: ApiRequest = serde_json::from_value(json!({
        "command": "future_command",
        "selector": "name == 'Default'"
    }))
    .unwrap();

    assert_eq!(request.command, Command::Unknown("future_command".into()));
    assert_eq!(request.field("selector"), Some(&json!("name == 'Default'")));
}

#[test]
fn api_error_serialization_omits_missing_code_but_keeps_status() {
    let error = ApiError::new(503, None, "temporary unavailable");

    assert_eq!(
        serde_json::to_value(error).unwrap(),
        json!({"error": "temporary unavailable", "status": 503})
    );
}

#[test]
fn command_status_exposes_success_and_failure_helpers() {
    let success = MascotCommandStatus::success();
    assert!(success.ok());

    let failure = MascotCommandStatus::failure(404, "mascot_not_found", "No such mascot");
    assert!(!failure.ok());
    assert_eq!(failure.status, 404);
    assert_eq!(failure.code.as_deref(), Some("mascot_not_found"));
    assert_eq!(failure.message, "No such mascot");
}
