use super::{
    AssetCatalog,
    catalog::{AssetSpec, RuntimeProfileSpec},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
    path::PathBuf,
};

pub(crate) fn zip_files(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        writer
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored)
                    .unix_permissions(if name.ends_with("katago.exe") {
                        0o755
                    } else {
                        0o644
                    }),
            )
            .unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

pub(crate) fn test_runtime_catalog(program: &[u8]) -> (AssetCatalog, Vec<(PathBuf, Vec<u8>)>) {
    let profile_id = "cuda12.8-cudnn9";
    let engine = zip_files(&[
        ("katago/katago.exe", program),
        ("katago/LICENSE.txt", b"engine license"),
    ]);
    let runtime = zip_files(&[
        ("cuda/bin/runtime.dll", b"tiny runtime DLL"),
        ("cuda/LICENSE", b"runtime license"),
    ]);
    let mut specs = Vec::new();
    let mut archives = Vec::new();
    for (id, bytes) in [("windows-nvidia", engine), ("runtime-cuda_cudart", runtime)] {
        let spec = AssetSpec {
            human_style: false,
            id: id.into(),
            file_name: format!("{id}.zip"),
            size: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            url: "http://127.0.0.1:1/not-used".into(),
            weight: false,
            published_at: None,
            runtime_profile: (id == "windows-nvidia").then(|| profile_id.into()),
            executable_sha256: (id == "windows-nvidia")
                .then(|| format!("{:x}", Sha256::digest(program))),
        };
        archives.push((spec.relative_path(), bytes));
        specs.push(spec);
    }
    let profiles = BTreeMap::from([(
        profile_id.into(),
        RuntimeProfileSpec {
            engine_asset: "windows-nvidia".into(),
            assets: specs.iter().map(|spec| spec.id.clone()).collect(),
            required_files: vec!["katago.exe".into(), "runtime.dll".into()],
        },
    )]);
    (AssetCatalog { specs, profiles }, archives)
}

pub(crate) fn test_acceleration_catalog(
    program: &[u8],
    cpu_program: &[u8],
    startup: &serde_json::Value,
) -> (AssetCatalog, Vec<(PathBuf, Vec<u8>)>) {
    let (mut catalog, mut archives) = test_runtime_catalog(program);
    let script = serde_json::to_vec(startup).unwrap();
    let engine = zip_files(&[
        ("katago.exe", program),
        ("startup.json", &script),
        ("LICENSE.txt", b"fake engine license"),
    ]);
    catalog.specs[0].size = engine.len() as u64;
    catalog.specs[0].sha256 = format!("{:x}", Sha256::digest(&engine));
    archives[0].1 = engine;
    let mut cpu = catalog.specs[0].clone();
    cpu.id = "windows-cpu".into();
    cpu.runtime_profile = None;
    cpu.executable_sha256 = Some(format!("{:x}", Sha256::digest(cpu_program)));
    catalog.specs.push(cpu);
    (catalog, archives)
}
