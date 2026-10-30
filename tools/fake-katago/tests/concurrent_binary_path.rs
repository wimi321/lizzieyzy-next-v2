#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Barrier;
use std::thread;

use common::Engine;

// Keep this test in its own integration-test executable so binary_path's
// process-wide cache is empty when all eight callers enter together.
#[test]
fn concurrent_first_callers_can_immediately_start_the_fake_engine() {
    let barrier = Barrier::new(8);
    thread::scope(|scope| {
        let callers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    let path = fake_katago::binary_path().expect("build the fake engine");
                    let mut engine = Engine::spawn_at(&path, &[]);
                    engine.ready();
                    engine.finish();
                    path
                })
            })
            .collect();
        let paths: Vec<_> = callers
            .into_iter()
            .map(|caller| {
                caller
                    .join()
                    .expect("caller should build and start the engine")
            })
            .collect();
        assert!(paths.iter().all(|path| path == &paths[0]));
    });
}
