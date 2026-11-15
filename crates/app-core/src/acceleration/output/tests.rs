use super::*;

#[test]
fn recognizes_documented_tuning_messages_without_guessing_from_generic_logs() {
    for line in [
        "Performing autotuning",
        "Tuning xGemmDirect ...",
        "Started OpenCL SGEMM tuner",
        "This will take a while",
        "This may take a long time",
        "2026-11-15 12:00:00: Performing autotuning",
    ] {
        assert!(is_tuning_line(line), "{line}");
    }
    for line in ["", "Loading model", "Using OpenCL Device", "driver failure"] {
        assert!(!is_tuning_line(line), "{line}");
    }
}

#[test]
fn backend_identity_comes_from_specific_engine_output() {
    for (line, backend) in [
        (
            "Cuda backend thread 0: Found GPU NVIDIA RTX",
            EngineBackend::Cuda,
        ),
        (
            "TensorRT backend thread 0: Found GPU NVIDIA RTX",
            EngineBackend::TensorRt,
        ),
        ("OpenCL backend thread 0", EngineBackend::OpenCl),
        ("Using OpenCL Device AMD Radeon", EngineBackend::OpenCl),
        ("Compiled with Eigen (CPU)", EngineBackend::Eigen),
    ] {
        assert_eq!(reported_backend(line), Some(backend));
    }
    for line in ["CUDA unavailable", "Cuda backend thread 0", "OpenCL", "CPU"] {
        assert_eq!(reported_backend(line), None);
    }
}

#[test]
fn diagnostics_keep_only_last_twenty_lines_with_bounded_utf8() {
    let mut tail = OutputTail::default();
    for i in 0..30 {
        tail.push(&format!("line {i}"));
    }
    assert_eq!(tail.lines().len(), 20);
    assert_eq!(tail.lines()[0], "line 10");
    assert_eq!(tail.lines()[19], "line 29");
    tail.push(&"显".repeat(5000));
    let lines = tail.lines();
    assert_eq!(lines.len(), 20);
    assert_eq!(lines[19].len(), 4095);
    assert!(lines[19].chars().all(|c| c == '显'));
    tail.push("one\ntwo\r\nthree");
    assert_eq!(&tail.lines()[17..], &["one", "two", "three"]);
}
