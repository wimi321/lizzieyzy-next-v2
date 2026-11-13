use std::{process::Stdio, time::Duration};

use tokio::{io::AsyncReadExt, process::Command};

use super::{AccelerationPlan, AccelerationReason, AssetError, catalog::AssetCatalog};

// Java NvidiaGpuDetector keeps CUDA as the default, including RTX 40/50.
// TensorRT requires SM >= 7.5; its optional recommendation is SM 7.5..8.8.
pub(crate) fn select(
    platform: &str,
    detected: bool,
    compute_rank: Option<u32>,
    catalog: &AssetCatalog,
) -> Result<AccelerationPlan, AssetError> {
    let mut plan = AccelerationPlan {
        archives_only: false,
        ..Default::default()
    };
    if platform != "windows-x64" {
        return Ok(plan);
    }
    if !detected {
        plan.reason = AccelerationReason::NoNvidiaGpu;
        return Ok(plan);
    }
    plan.reason = AccelerationReason::NvidiaDetected;
    plan.runtime_profile = catalog.get("windows-nvidia")?.runtime_profile.clone();
    plan.recommended_assets = catalog.profile("cuda12.8-cudnn9")?.assets.clone();
    // Java permits manual TensorRT on detected NVIDIA with unknown capability.
    plan.tensor_rt_supported = compute_rank.is_none_or(|rank| rank >= 75);
    plan.tensor_rt_recommended = compute_rank.is_some_and(|rank| (75..89).contains(&rank));
    if plan.tensor_rt_supported {
        plan.optional_assets = vec!["windows-tensorrt".into(), "runtime-tensorrt".into()];
    }
    for id in plan.recommended_assets.iter().chain(&plan.optional_assets) {
        catalog.get(id)?;
    }
    Ok(plan)
}

pub(crate) async fn detect(catalog: &AssetCatalog) -> Result<AccelerationPlan, AssetError> {
    if !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return select("other", false, None, catalog);
    }
    let mut programs = vec![std::path::PathBuf::from("nvidia-smi")];
    if let Some(root) = std::env::var_os("SystemRoot") {
        programs.push(std::path::PathBuf::from(root).join("System32/nvidia-smi.exe"));
    }
    let mut successful_empty = false;
    // Java retries without compute_cap for drivers whose nvidia-smi does not
    // support that column, and also checks the fixed Windows system path.
    for fields in ["name,compute_cap", "name"] {
        for program in &programs {
            if let Some(output) = query(program, fields).await {
                let (detected, rank) = if fields == "name" {
                    parse_gpu_names(&output)
                } else {
                    parse_gpu_output(&output)
                };
                if detected {
                    return select("windows-x64", true, rank, catalog);
                }
                successful_empty |= output.trim().is_empty();
            }
        }
    }
    if successful_empty {
        select("windows-x64", false, None, catalog)
    } else {
        Ok(failed())
    }
}

async fn query(program: &std::path::Path, fields: &str) -> Option<String> {
    // Fixed read-only query, no shell or user-controlled executable/arguments.
    let mut command = Command::new(program);
    command
        .args([
            format!("--query-gpu={fields}"),
            "--format=csv,noheader,nounits".into(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
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
    let status = tokio::time::timeout_at(deadline, output).await;
    match status {
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

fn failed() -> AccelerationPlan {
    AccelerationPlan {
        reason: AccelerationReason::DetectionFailed,
        archives_only: false,
        ..Default::default()
    }
}

pub(crate) fn parse_gpu_output(output: &str) -> (bool, Option<u32>) {
    let mut detected = false;
    let mut best = None;
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let Some((name, cap)) = line.rsplit_once(',') else {
            continue;
        };
        if name.trim().is_empty() {
            continue;
        }
        detected = true;
        let rank = cap.trim().split_once('.').and_then(|(major, minor)| {
            let major = major.parse::<u32>().ok()?;
            let minor = minor.parse::<u32>().ok()?;
            (minor < 10).then_some(major.checked_mul(10)?.checked_add(minor)?)
        });
        best = best.max(rank.or_else(|| infer_capability(name)));
    }
    (detected, best)
}

pub(crate) fn parse_gpu_names(output: &str) -> (bool, Option<u32>) {
    let names: Vec<_> = output
        .lines()
        .filter(|name| !name.trim().is_empty())
        .collect();
    (
        !names.is_empty(),
        names.into_iter().filter_map(infer_capability).max(),
    )
}

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
