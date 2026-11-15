use std::{io::Read, path::Path, process::Stdio, time::Duration};

use sha2::{Digest, Sha256};
use tokio::{io::AsyncReadExt, process::Command};

use super::{
    AccelerationBackend, AccelerationGpu, AccelerationNote, AccelerationOption, AccelerationPlan,
    AccelerationReason, AssetError, EngineBackend, catalog::AssetCatalog,
};

#[cfg(test)]
pub(crate) fn select(
    platform: &str,
    detected: bool,
    compute_rank: Option<u32>,
    catalog: &AssetCatalog,
) -> Result<AccelerationPlan, AssetError> {
    select_gpus(
        platform,
        if detected {
            vec![AccelerationGpu {
                name: "NVIDIA".into(),
                compute_capability: compute_rank,
                driver_version: None,
                nvidia: true,
            }]
        } else {
            Vec::new()
        },
        catalog,
    )
}

/// The transactional trial is the final compatibility check.
pub(crate) fn select_gpus(
    platform: &str,
    gpus: Vec<AccelerationGpu>,
    catalog: &AssetCatalog,
) -> Result<AccelerationPlan, AssetError> {
    let mut plan = AccelerationPlan {
        archives_only: false,
        ..Default::default()
    };
    if platform != "windows-x64" {
        return Ok(plan);
    }
    plan.gpus = gpus;
    let nvidia = plan.gpus.iter().any(|gpu| gpu.nvidia);
    let rank = plan
        .gpus
        .iter()
        .filter(|gpu| gpu.nvidia)
        .filter_map(|gpu| gpu.compute_capability)
        .max();
    let old_driver = plan
        .gpus
        .iter()
        .filter(|gpu| gpu.nvidia)
        .filter_map(|gpu| gpu.driver_version.as_deref())
        .any(driver_may_be_old);
    plan.reason = if nvidia {
        AccelerationReason::NvidiaDetected
    } else {
        AccelerationReason::NoNvidiaGpu
    };
    plan.tensor_rt_supported = nvidia && rank.is_none_or(|rank| rank >= 75);
    plan.tensor_rt_recommended = nvidia && rank.is_some_and(|rank| (75..89).contains(&rank));
    if nvidia {
        plan.runtime_profile = Some(AccelerationBackend::Cuda.profile_id().into());
        plan.recommended_assets = catalog
            .profile(AccelerationBackend::Cuda.profile_id())?
            .assets
            .clone();
        if plan.tensor_rt_supported {
            plan.optional_assets = vec!["windows-tensorrt".into(), "runtime-tensorrt".into()];
        }
    } else if !plan.gpus.is_empty() {
        plan.runtime_profile = Some(AccelerationBackend::OpenCl.profile_id().into());
        plan.recommended_assets = catalog
            .profile(AccelerationBackend::OpenCl.profile_id())?
            .assets
            .clone();
    }
    if !plan
        .recommended_assets
        .iter()
        .any(|id| id == "windows-opencl")
    {
        plan.optional_assets.push("windows-opencl".into());
    }
    for backend in [
        AccelerationBackend::Cuda,
        AccelerationBackend::TensorRt,
        AccelerationBackend::OpenCl,
    ] {
        let profile_id = backend.profile_id();
        // Local test catalogs can expose a single profile.
        let Some(profile) = catalog.profiles.get(profile_id) else {
            continue;
        };
        let (supported, recommended) = match backend {
            AccelerationBackend::Cuda => (nvidia, nvidia),
            AccelerationBackend::TensorRt => (plan.tensor_rt_supported, plan.tensor_rt_recommended),
            AccelerationBackend::OpenCl => (true, !nvidia && !plan.gpus.is_empty()),
        };
        let download_bytes = profile.assets.iter().try_fold(0u64, |sum, id| {
            sum.checked_add(catalog.get(id)?.size)
                .ok_or_else(|| AssetError::Catalog("runtime size overflow".into()))
        })?;
        plan.options.push(AccelerationOption {
            backend,
            profile_id: profile_id.into(),
            installed: false,
            download_bytes,
            recommended,
            supported,
            note: (old_driver && backend != AccelerationBackend::OpenCl)
                .then_some(AccelerationNote::DriverMayBeTooOld),
        });
    }
    Ok(plan)
}

pub(crate) async fn detect(catalog: &AssetCatalog) -> Result<AccelerationPlan, AssetError> {
    if !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return select_gpus("other", Vec::new(), catalog);
    }
    let mut programs = vec![std::path::PathBuf::from("nvidia-smi")];
    if let Some(root) = std::env::var_os("SystemRoot") {
        programs.push(std::path::PathBuf::from(root).join("System32/nvidia-smi.exe"));
    }
    let mut gpus = Vec::new();
    // Older drivers omit compute_cap, while still supporting driver_version.
    'queries: for (fields, compute) in [
        ("name,compute_cap,driver_version", true),
        ("name,driver_version", false),
    ] {
        for program in &programs {
            let mut command = Command::new(program);
            command.args([
                format!("--query-gpu={fields}"),
                "--format=csv,noheader,nounits".into(),
            ]);
            if let Some(output) = query(command, Duration::from_secs(3)).await {
                gpus = parse_nvidia(&output, compute);
                if !gpus.is_empty() {
                    break 'queries;
                }
            }
        }
    }
    let mut command = Command::new("powershell.exe");
    // Constant read-only command; no user input. Explicit UTF-8 preserves names.
    command.args(["-NoProfile", "-NonInteractive", "-Command",
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; Get-CimInstance Win32_VideoController | ForEach-Object Name"]);
    if let Some(output) = query(command, Duration::from_secs(5)).await {
        merge_adapters(&mut gpus, parse_display_adapters(&output));
    }
    select_gpus("windows-x64", gpus, catalog)
}

async fn query(mut command: Command, timeout: Duration) -> Option<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let deadline = tokio::time::Instant::now() + timeout;
    let mut child = tokio::time::timeout_at(deadline, engine_session::spawn_process(&mut command))
        .await
        .ok()?
        .ok()?;
    let stdout = child.stdout.take()?;
    let mut bytes = Vec::new();
    let output = async {
        stdout.take(16 * 1024 + 1).read_to_end(&mut bytes).await?;
        child.wait().await
    };
    match tokio::time::timeout_at(deadline, output).await {
        Ok(Ok(status)) if status.success() && bytes.len() <= 16 * 1024 => {
            Some(String::from_utf8_lossy(&bytes).into_owned())
        }
        _ => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            None
        }
    }
}

pub(crate) fn parse_nvidia(output: &str, compute: bool) -> Vec<AccelerationGpu> {
    output
        .lines()
        .filter_map(|line| {
            // Split from the end so adapter names can contain commas.
            let (prefix, driver) = line.rsplit_once(',')?;
            let (name, rank) = if compute {
                let (name, capability) = prefix.rsplit_once(',')?;
                (
                    name.trim(),
                    parse_capability(capability).or_else(|| infer_capability(name)),
                )
            } else {
                (prefix.trim(), infer_capability(prefix))
            };
            if !is_physical_adapter(name) {
                return None;
            }
            Some(AccelerationGpu {
                name: name.into(),
                compute_capability: rank,
                nvidia: true,
                driver_version: parse_driver(driver).map(|_| driver.trim().to_owned()),
            })
        })
        .collect()
}

pub(crate) fn parse_display_adapters(output: &str) -> Vec<AccelerationGpu> {
    let mut adapters = Vec::new();
    for name in output
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|name| is_physical_adapter(name))
    {
        if adapters
            .iter()
            .any(|gpu: &AccelerationGpu| gpu.name.eq_ignore_ascii_case(name))
        {
            continue;
        }
        let nvidia = name.to_ascii_lowercase().contains("nvidia");
        adapters.push(AccelerationGpu {
            name: name.into(),
            nvidia,
            compute_capability: nvidia.then(|| infer_capability(name)).flatten(),
            driver_version: None,
        });
    }
    adapters
}

fn merge_adapters(gpus: &mut Vec<AccelerationGpu>, adapters: Vec<AccelerationGpu>) {
    for adapter in adapters {
        if !gpus
            .iter()
            .any(|gpu| gpu.name.eq_ignore_ascii_case(&adapter.name))
        {
            gpus.push(adapter);
        }
    }
}

fn is_physical_adapter(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    !name.trim().is_empty()
        && ![
            "virtual",
            "remote",
            "basic display",
            "rdp",
            "mirror",
            "indirect display",
        ]
        .iter()
        .any(|part| name.contains(part))
}

fn parse_capability(cap: &str) -> Option<u32> {
    let (major, minor) = cap.trim().split_once('.')?;
    let major = major.parse::<u32>().ok()?;
    let minor = minor.parse::<u32>().ok()?;
    (minor < 10).then_some(major.checked_mul(10)?.checked_add(minor)?)
}

fn parse_driver(driver: &str) -> Option<(u32, u32)> {
    let mut parts = driver.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    if parts.any(|part| part.parse::<u32>().is_err()) {
        return None;
    }
    Some((major, minor))
}

pub(crate) fn driver_may_be_old(driver: &str) -> bool {
    parse_driver(driver).is_some_and(|version| version < (570, 65))
}

/// Identify bytes, never the executable's filename or its directory.
pub(super) fn current_backend(catalog: &AssetCatalog, program: &Path) -> EngineBackend {
    let digest = (|| -> Option<String> {
        let mut file = std::fs::File::open(program).ok()?;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file() || metadata.len() > 256 * 1024 * 1024 {
            return None;
        }
        let mut hash = Sha256::new();
        let mut reader = (&mut file).take(metadata.len().saturating_add(1));
        let mut total = 0;
        let mut buffer = [0; 64 * 1024];
        loop {
            let count = reader.read(&mut buffer).ok()?;
            if count == 0 {
                break;
            }
            total += count as u64;
            hash.update(&buffer[..count]);
        }
        (total == metadata.len()).then(|| format!("{:x}", hash.finalize()))
    })();
    digest.as_deref().map_or(EngineBackend::External, |digest| {
        backend_for_digest(catalog, digest)
    })
}

pub(crate) fn backend_for_digest(catalog: &AssetCatalog, digest: &str) -> EngineBackend {
    let asset = catalog.specs.iter().find(|spec| {
        spec.executable_sha256
            .as_deref()
            .is_some_and(|pin| pin.eq_ignore_ascii_case(digest))
    });
    let backend = asset
        .and_then(|spec| match spec.id.as_str() {
            "windows-nvidia" => Some("cuda"),
            "windows-tensorrt" => Some("tensorrt"),
            "windows-opencl" => Some("opencl"),
            "windows-cpu" => Some("eigen"),
            _ => None,
        })
        .map(str::to_owned)
        .or_else(|| super::catalog::backend_for_digest(digest));
    match backend.as_deref() {
        Some("eigen") => EngineBackend::Eigen,
        Some("cuda") => EngineBackend::Cuda,
        Some("tensorrt") => EngineBackend::TensorRt,
        Some("opencl") => EngineBackend::OpenCl,
        _ => EngineBackend::External,
    }
}

#[cfg(test)]
pub(crate) fn parse_gpu_output(output: &str) -> (bool, Option<u32>) {
    let gpus = parse_nvidia(
        &output
            .lines()
            .map(|line| format!("{line}, N/A"))
            .collect::<Vec<_>>()
            .join("\n"),
        true,
    );
    (
        !gpus.is_empty(),
        gpus.iter().filter_map(|gpu| gpu.compute_capability).max(),
    )
}

#[cfg(test)]
pub(crate) fn parse_gpu_names(output: &str) -> (bool, Option<u32>) {
    let gpus = parse_nvidia(
        &output
            .lines()
            .map(|line| format!("{line}, N/A"))
            .collect::<Vec<_>>()
            .join("\n"),
        false,
    );
    (
        !gpus.is_empty(),
        gpus.iter().filter_map(|gpu| gpu.compute_capability).max(),
    )
}

#[cfg(test)]
mod tests;

// Same explicit name fallback as Java NvidiaGpuDetector; never guess from an
// arbitrary model number or invent a capability for unrecognized products.
fn infer_capability(name: &str) -> Option<u32> {
    let name = name.to_ascii_uppercase();
    if name.contains("BLACKWELL") || name.contains("RTX PRO") {
        return Some(120);
    }
    if name.contains(" ADA") || name.ends_with("ADA") {
        return Some(89);
    }
    if let Some(generation) = generation(&name, "RTX") {
        match generation {
            50.. => return Some(120),
            40..=49 => return Some(89),
            30..=39 => return Some(86),
            20..=29 => return Some(75),
            _ => {}
        }
    }
    if let Some(generation) = generation(&name, "GTX") {
        match generation {
            16 => return Some(75),
            10 => return Some(61),
            9 => return Some(52),
            _ => {}
        }
    }
    if name.contains("TITAN RTX") || name.contains("QUADRO RTX") {
        return Some(75);
    }
    if name.contains("RTX A") {
        return Some(86);
    }
    for word in name.split(|c: char| !c.is_ascii_alphanumeric()) {
        match word {
            "A100" => return Some(80),
            "A40" | "A10" | "A16" | "A2" => return Some(86),
            "L4" | "L40" | "L40S" => return Some(89),
            "T4" => return Some(75),
            _ => {}
        }
    }
    None
}

fn generation(name: &str, prefix: &str) -> Option<u32> {
    for (index, _) in name.match_indices(prefix) {
        if index > 0 && name.as_bytes()[index - 1].is_ascii_alphanumeric() {
            continue;
        }
        let digits = name[index + prefix.len()..].trim_start();
        if digits.len() >= 4 && digits.as_bytes()[..4].iter().all(u8::is_ascii_digit) {
            return digits[..2].parse().ok();
        }
    }
    None
}
