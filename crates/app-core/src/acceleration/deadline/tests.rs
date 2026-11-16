use super::*;

#[test]
fn candidates_use_the_backend_base_timeout() {
    let config = crate::AppConfig::default();
    for (backend, seconds) in [
        (AccelerationBackend::Cuda, 180),
        (AccelerationBackend::TensorRt, 600),
        (AccelerationBackend::OpenCl, 600),
    ] {
        assert_eq!(timeout(backend, &config), Duration::from_secs(seconds));
    }
}
