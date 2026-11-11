use std::collections::HashSet;
use std::fs;
use std::sync::Barrier;

use super::TempDirectory;

#[test]
fn parallel_directories_with_identical_timestamps_are_isolated_and_cleaned() {
    let barrier = Barrier::new(8);
    let directories = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|index| {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    let directory = TempDirectory::new_at("same-timestamp", 42);
                    fs::create_dir(directory.path().join("nested")).unwrap();
                    fs::write(directory.path().join("nested/owner"), index.to_string()).unwrap();
                    directory
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    let paths: HashSet<_> = directories
        .iter()
        .map(|directory| directory.path().to_path_buf())
        .collect();
    assert_eq!(paths.len(), directories.len());
    for (index, directory) in directories.iter().enumerate() {
        assert_eq!(
            fs::read_to_string(directory.path().join("nested/owner")).unwrap(),
            index.to_string()
        );
    }
    for directory in directories {
        let path = directory.path().to_path_buf();
        drop(directory);
        assert!(
            !path.exists(),
            "Drop must remove the directory and its contents"
        );
    }
}

#[test]
fn directory_is_cleaned_during_panic_unwinding() {
    let directory = TempDirectory::new("panic-cleanup");
    let path = directory.path().to_path_buf();
    fs::write(path.join("marker"), b"test").unwrap();
    let result = std::panic::catch_unwind(move || {
        let _directory = directory;
        panic!("simulate a failing test");
    });
    assert!(result.is_err());
    assert!(!path.exists(), "a failing test must not leak its directory");
}
