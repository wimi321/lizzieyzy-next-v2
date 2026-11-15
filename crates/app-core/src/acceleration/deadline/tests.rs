use super::*;

#[tokio::test(start_paused = true)]
async fn backend_deadlines_expire_without_tuning() {
    for (backend, seconds) in [
        (AccelerationBackend::Cuda, 180),
        (AccelerationBackend::TensorRt, 600),
        (AccelerationBackend::OpenCl, 600),
    ] {
        let now = Instant::now();
        let timer = StartupDeadline::new(backend, now);
        tokio::time::advance(Duration::from_secs(seconds - 1)).await;
        assert!(Instant::now() < timer.deadline());
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(Instant::now(), timer.deadline());
    }
}

#[tokio::test(start_paused = true)]
async fn opencl_tuning_extends_each_line_but_stops_at_thirty_minutes() {
    let started = Instant::now();
    let mut timer = StartupDeadline::new(AccelerationBackend::OpenCl, started);
    tokio::time::advance(Duration::from_secs(599)).await;
    assert!(timer.on_tuning(Instant::now()));
    tokio::time::advance(Duration::from_secs(1)).await;
    assert!(Instant::now() < timer.deadline());
    assert_eq!(timer.deadline(), started + Duration::from_secs(720));
    for _ in 0..20 {
        assert!(timer.on_tuning(Instant::now()));
    }
    assert_eq!(timer.deadline(), started + MAX_OPENCL_STARTUP);
    tokio::time::advance(Duration::from_secs(1200)).await;
    assert!(!timer.on_tuning(Instant::now()));
    assert_eq!(timer.deadline(), started + MAX_OPENCL_STARTUP);
}

#[tokio::test(start_paused = true)]
async fn tuning_does_not_extend_other_backends_or_revive_expired_opencl() {
    for backend in [AccelerationBackend::Cuda, AccelerationBackend::TensorRt] {
        let mut timer = StartupDeadline::new(backend, Instant::now());
        let deadline = timer.deadline();
        assert!(!timer.on_tuning(Instant::now()));
        assert_eq!(timer.deadline(), deadline);
    }
    let mut timer = StartupDeadline::new(AccelerationBackend::OpenCl, Instant::now());
    tokio::time::advance(Duration::from_secs(601)).await;
    let deadline = timer.deadline();
    assert!(!timer.on_tuning(Instant::now()));
    assert_eq!(timer.deadline(), deadline);
}
