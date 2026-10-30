#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn split(default_left: u8) -> Value {
    json!({
        "left_children": [1, -1, -1],
        "right_children": [2, -1, -1],
        "split_indices": [0, 0, 0],
        "split_conditions": [0.5, -2.0, 3.0],
        "default_left": [default_left, 0, 0]
    })
}

fn booster(trees: Vec<Value>) -> String {
    json!({"learner": {
        "learner_model_param": {"base_score": "[6E0]", "num_feature": "1"},
        "gradient_booster": {"model": {"trees": trees}}
    }})
    .to_string()
}

fn calibrator(feature: &str) -> String {
    json!({"final_calibrator": {
        "spec": {"gate_start": 0.0, "gate_full": 1.0},
        "feature_order": [feature], "scaler_mean": [0.0], "scaler_scale": [1.0],
        "intercept": 1.0, "coefficients": [1.0]
    }})
    .to_string()
}

fn known_full29_features() -> [f64; 29] {
    // XGBoostStrengthModelTest.knownFull29Features, evaluated by the unmodified Java scorer.
    [
        0.4444,
        0.7444,
        0.8111,
        0.2284148013,
        0.5444,
        0.8222,
        0.8222,
        0.67,
        1.0,
        1.0,
        0.5154639175,
        0.6406149904,
        0.9132420091,
        0.5379236148,
        0.3597122302,
        0.2777777778,
        0.2317497103,
        0.546149645,
        0.32,
        0.5800464037,
        0.4803073967,
        0.7942811755,
        0.9333,
        0.74,
        0.9,
        0.142208,
        0.263104,
        0.2144,
        0.259552,
    ]
}

#[test]
fn single_tree_uses_strict_threshold_and_leaf_split_conditions() {
    let model = XGBoostStrengthModel::from_json(&booster(vec![split(0)]), None).unwrap();
    assert_eq!(model.predict(&[0.49]), 4.0);
    assert_eq!(model.predict(&[0.5]), 9.0);
}

#[test]
fn multiple_trees_add_to_base_score() {
    let model = XGBoostStrengthModel::from_json(&booster(vec![split(0), split(1)]), None).unwrap();
    assert_eq!(model.predict(&[0.49]), 2.0);
    assert_eq!(model.predict(&[0.5]), 12.0);
}

#[test]
fn missing_features_and_nan_follow_both_default_branches() {
    for (default, expected) in [(0, 9.0), (1, 4.0)] {
        let model = XGBoostStrengthModel::from_json(&booster(vec![split(default)]), None).unwrap();
        assert_eq!(model.predict(&[f64::NAN]), expected);
        assert_eq!(model.predict(&[]), expected);
    }
}

#[test]
fn bundled_model_matches_java_prediction_to_one_micro_rank() {
    let model = XGBoostStrengthModel::bundled().unwrap();
    assert_eq!(model.feature_count, 20);
    assert_eq!(model.trees.len(), 400);
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/strength/java-model-predictions.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let features = std::array::from_fn(|index| case["features"][index].as_f64().unwrap());
        let selected = FEATURE_INDICES.map(|index| features[index]);
        assert!(
            (model.predict(&selected) - case["raw_prediction"].as_f64().unwrap()).abs() < 1e-6,
            "{} raw prediction",
            case["name"]
        );
        assert!(
            (model.predict_rank(&features) - case["rank_value"].as_f64().unwrap()).abs() < 1e-6,
            "{} calibrated prediction",
            case["name"]
        );
    }
}

#[test]
fn residual_calibration_matches_java_gate_and_correction() {
    let model = XGBoostStrengthModel::bundled().unwrap();
    let features = known_full29_features();
    assert_eq!(model.calibrate(4.8, &features), 4.8);
    assert!((model.calibrate(7.4, &features) - 8.25371852).abs() < 1e-6);
    assert_eq!(model.calibrate(100.0, &features), 12.0);
    assert_eq!(model.calibrate(-100.0, &features), -1.0);
}

#[test]
fn malformed_hinge_and_unknown_features_keep_uncalibrated_prediction() {
    for feature in ["hinge_above_not-a-number", "hinge_above_inf", "unknown"] {
        let model =
            XGBoostStrengthModel::from_json(&booster(vec![split(0)]), Some(&calibrator(feature)))
                .unwrap();
        assert_eq!(model.calibrate(8.0, &known_full29_features()), 8.0);
    }
}

#[test]
fn invalid_calibrator_lengths_fall_back_and_zero_scale_is_one() {
    let mut data: Value = serde_json::from_str(&calibrator("base_prediction")).unwrap();
    data["final_calibrator"]["scaler_scale"] = json!([0.0]);
    let model = XGBoostStrengthModel::from_json(&booster(vec![]), Some(&data.to_string())).unwrap();
    assert_eq!(model.calibrate(8.0, &known_full29_features()), 9.0);
    data["final_calibrator"]["scaler_mean"] = json!([]);
    let model = XGBoostStrengthModel::from_json(&booster(vec![]), Some(&data.to_string())).unwrap();
    assert_eq!(model.calibrate(8.0, &known_full29_features()), 8.0);
}

#[test]
fn malformed_trees_return_typed_errors_without_panics_or_loops() {
    let mut tree = split(0);
    tree["left_children"] = json!([0, -1, -1]);
    assert!(matches!(
        XGBoostStrengthModel::from_json(&booster(vec![tree]), None),
        Err(ModelError::Invalid("cyclic tree"))
    ));
    for (field, value) in [
        ("right_children", json!([3, -1, -1])),
        ("split_indices", json!([1, 0, 0])),
        ("default_left", json!([])),
    ] {
        let mut tree = split(0);
        tree[field] = value;
        assert!(XGBoostStrengthModel::from_json(&booster(vec![tree]), None).is_err());
    }
    assert!(XGBoostStrengthModel::from_json("{}", None).is_err());
}

#[test]
fn bundled_files_match_recorded_sha256() {
    let provenance: Value =
        serde_json::from_str(include_str!("../../models/provenance.json")).unwrap();
    for (name, bytes) in [
        ("xgboost20tun_booster.json", BOOSTER.as_bytes()),
        (
            "xgboost20tun_residual_calibrator.json",
            CALIBRATOR.as_bytes(),
        ),
    ] {
        let entry = provenance["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["file"] == name)
            .unwrap();
        assert_eq!(entry["sha256"], format!("{:x}", Sha256::digest(bytes)));
        assert_eq!(
            entry["source_path"],
            format!("src/main/resources/models/strength/{name}")
        );
    }
}
