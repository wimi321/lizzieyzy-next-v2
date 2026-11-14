#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;

#[test]
fn documented_plans_and_weights_build_fixed_connection_arguments() {
    let default = ZhiziEngineOptions::default();
    assert_eq!(
        default.connection_args(),
        "--platform all --engine-type go --gpu-type vip-share --kata-name katago-TENSORRT --kata-weight 10b512t"
    );
    for (plan, gpu, backend) in [
        (ZhiziPlan::Vip, "vip-share", "TENSORRT"),
        (ZhiziPlan::OnDemand1x, "1x", "TENSORRT"),
        (ZhiziPlan::OnDemand3x, "3x", "TENSORRT"),
        (ZhiziPlan::OnDemand6x, "6x", "TENSORRT"),
        (ZhiziPlan::OnDemand12x, "12x", "TENSORRT"),
        (ZhiziPlan::OnDemand24x, "24x", "TENSORRT"),
        (ZhiziPlan::OnDemand1xCuda, "1x", "CUDA"),
    ] {
        let options = ZhiziEngineOptions { plan, ..default };
        assert_eq!(
            options.connection_args(),
            format!(
                "--platform all --engine-type go --gpu-type {gpu} --kata-name katago-{backend} --kata-weight 10b512t"
            )
        );
    }
    for weight in [
        "20b", "10b384t", "10b512t", "11b768t", "18bnbt", "28bnbt", "fdx", "60b", "40b",
    ] {
        let options = ZhiziEngineOptions {
            weight: serde_json::from_value(serde_json::json!(weight)).unwrap(),
            ..default
        };
        assert!(
            options
                .connection_args()
                .ends_with(&format!("--kata-weight {weight}"))
        );
    }
    assert!(serde_json::from_str::<ZhiziWeight>("\"arbitrary --command\"").is_err());
    assert_eq!(
        serde_json::from_str::<ZhiziEngineOptions>("{}").unwrap(),
        default
    );
}

#[tokio::test]
async fn fingerprints_distinguish_provider_plan_weight_and_version_without_local_files() {
    let original = ZhiziEngineOptions::default().profile();
    let fingerprint = original.fingerprint().await.unwrap();
    let plan = ZhiziEngineOptions {
        plan: ZhiziPlan::OnDemand3x,
        ..Default::default()
    }
    .profile();
    let weight = ZhiziEngineOptions {
        weight: ZhiziWeight::Fdx,
        ..Default::default()
    }
    .profile();
    for profile in [plan, weight, original.with_remote_version("1.18.0")] {
        assert_ne!(fingerprint, profile.fingerprint().await.unwrap());
    }
    let websocket = EngineProfile {
        remote: Some(RemoteEngineConfig::WebSocket {
            host: "127.0.0.1".into(),
            port: 443,
            secure: true,
            identity: "0".repeat(64),
            version: None,
        }),
        ..EngineProfile::default()
    };
    assert_ne!(fingerprint, websocket.fingerprint().await.unwrap());
}
