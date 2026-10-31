#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn bundled_catalog_contains_java_b11_identity_and_minimum_version() {
    let catalog: ModelCatalog = serde_json::from_str(CATALOG).unwrap();
    let model = &catalog.models[&catalog.default_model_id];
    assert_eq!(model.file_name, "kata1-tf3-b11c768-s12002M-d6304M.bin.gz");
    assert_eq!(model.size_bytes, 262_017_809);
    assert_eq!(
        model.sha256,
        "4a6312e80faadee7b7dd28689a2e87a1efb4640c10132f16290da7a17b4c6d9e"
    );
    assert_eq!(model.minimum_katago_version, "1.17.0");
    assert_eq!(catalog.rank(Some(model)), 0);
}

#[test]
fn mismatched_sizes_never_invoke_the_hasher() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("default.bin.gz");
    fs::write(&path, "small fake model").unwrap();
    let catalog = ModelCatalog::bundled();
    let identified = catalog.identify_with(&path, |_, _| {
        panic!("size mismatch must skip reading model contents")
    });
    assert_eq!(identified, None);
}

#[test]
fn matching_size_without_matching_digest_is_not_a_catalog_model() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("kata1-tf3-b11c768-s12002M-d6304M.bin.gz");
    fs::write(&path, "bad").unwrap();
    let mut catalog = ModelCatalog::bundled();
    catalog.models.get_mut("b11-flagship").unwrap().size_bytes = 3;
    assert_eq!(catalog.identify(&path), None);
}

#[test]
fn hash_read_is_bounded_and_rejects_changed_sizes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("model.bin.gz");
    fs::write(&path, "abc").unwrap();
    assert_eq!(
        hash_file(&path, 3).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert!(hash_file(&path, 2).is_err());
    assert!(hash_file(&path, 4).is_err());
}
