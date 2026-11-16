use super::*;

#[test]
fn real_opencl_and_tensorrt_lines_select_grace_periods() {
    for line in [
        "Performing autotuning",
        "2026-11-16 08:15:20+0800: Performing autotuning",
        "Creating new plan cache",
        "2026-11-16 08:15:20+0800: Creating new plan cache",
    ] {
        assert_eq!(
            startup_extension(line),
            Some(Duration::from_secs(600)),
            "{line}"
        );
    }
    for line in [
        "Beginning GPU tuning for NVIDIA GeForce RTX 3070 Laptop GPU modelVersion 15 channels 512",
        "Tuning xGemmDirect for 1x1 convolutions and matrix mult",
        "Tuning hGemmWmmaNCHW for 1x1 convolutions",
        "Testing 55 different configs",
        "Tuning 40/55 ...",
        "Tuning 0/55 (reference) Calls/sec 104.872 ErrorProp 5.2897e-08",
    ] {
        assert_eq!(
            startup_extension(line),
            Some(Duration::from_secs(120)),
            "{line}"
        );
    }
    for line in [
        "Tuning dummy thread numeric total: 3.87805e+09",
        "2026-11-16 08:17:12+0800: Tuning dummy thread numeric total: 3.87805e+09",
        "Done tuning",
        "2026-11-16 08:17:12+0800: Done tuning, saved results to home/opencltuning/tune.txt",
        "TensorRT backend: building network via ONNX emitter",
        "Saved new plan cache to home/trtcache/trt-100900_gpu-1662361e_net-model",
        "TensorRT backend thread 0: Model version 15 useFP16 = true",
        "Testing 0 different configs",
        "Testing unknown different configs",
        "Warning: Performing autotuning failed",
        "This will take a while",
        "Started OpenCL SGEMM",
    ] {
        assert_eq!(startup_extension(line), None, "{line}");
    }
}

#[tokio::test(start_paused = true)]
async fn progress_uses_now_plus_grace_without_accumulating_or_reviving() {
    let started = Instant::now();
    let mut deadline =
        StartupDeadline::new(started, Duration::from_secs(120), Duration::from_secs(1800));
    tokio::time::advance(Duration::from_secs(100)).await;
    assert!(deadline.observe("Tuning 40/55 ...", Instant::now()));
    assert_eq!(deadline.deadline(), started + Duration::from_secs(220));
    for _ in 0..100 {
        assert!(deadline.observe("Testing 55 different configs", Instant::now()));
    }
    assert_eq!(deadline.deadline(), started + Duration::from_secs(220));
    assert!(deadline.observe("Creating new plan cache", Instant::now()));
    assert_eq!(deadline.deadline(), started + Duration::from_secs(700));
    assert!(!deadline.observe("Done tuning", Instant::now()));
    tokio::time::advance(Duration::from_secs(600)).await;
    assert!(!deadline.observe("Performing autotuning", Instant::now()));
    assert_eq!(deadline.deadline(), Instant::now());
}

#[tokio::test(start_paused = true)]
async fn startup_extensions_stop_at_attempt_cap_and_new_attempt_resets_it() {
    let started = Instant::now();
    let mut deadline =
        StartupDeadline::new(started, Duration::from_secs(120), Duration::from_secs(1800));
    for _ in 0..17 {
        tokio::time::advance(Duration::from_secs(100)).await;
        assert!(deadline.observe("Performing autotuning", Instant::now()));
    }
    assert_eq!(deadline.deadline(), started + Duration::from_secs(1800));
    tokio::time::advance(Duration::from_secs(100)).await;
    assert!(!deadline.observe("Tuning xGemmDirect", Instant::now()));
    let next = StartupDeadline::new(
        Instant::now(),
        Duration::from_secs(120),
        Duration::from_secs(1800),
    );
    assert_eq!(next.deadline(), Instant::now() + Duration::from_secs(120));
}

#[tokio::test(start_paused = true)]
async fn no_progress_keeps_the_original_deadline() {
    let started = Instant::now();
    let mut deadline =
        StartupDeadline::new(started, Duration::from_secs(120), Duration::from_secs(1800));
    tokio::time::advance(Duration::from_secs(119)).await;
    assert!(!deadline.observe("Loaded model", Instant::now()));
    assert!(Instant::now() < deadline.deadline());
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(Instant::now(), deadline.deadline());
}

#[test]
fn diagnostic_lines_are_truncated_at_a_utf8_boundary() {
    assert_eq!(bounded_line("a".repeat(5000)).len(), 4096);
    let line = bounded_line("显".repeat(2000));
    assert_eq!(line.len(), 4095);
    assert!(line.chars().all(|character| character == '显'));
}

#[test]
fn startup_cap_is_validated_like_the_other_session_timeouts() {
    for duration in [Duration::ZERO, Duration::from_secs(86401)] {
        let cfg = crate::SessionConfig {
            max_startup_duration: duration,
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }
    assert!(crate::SessionConfig::default().validate().is_ok());
}
