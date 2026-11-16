//! Scripted startup diagnostics. Only the fake process reads these test knobs.
use crate::{RunError, cli::Options};
pub(crate) fn backend(options: &Options) -> Result<(), RunError> {
    let line = match options.backend.as_deref() {
        Some("cuda") => "Cuda backend thread 0: Found GPU Fake NVIDIA GPU",
        Some("tensorrt") => "TensorRT backend thread 0: Found GPU Fake NVIDIA GPU",
        Some("opencl") => "OpenCL backend thread 0: Using OpenCL Device Fake GPU",
        Some("eigen") => "Compiled with Eigen (CPU)",
        _ => return Ok(()),
    };
    eprintln!("{line}");
    Ok(())
}

#[derive(Default)]
pub(crate) struct Tuning {
    emitted: u64,
}

impl Tuning {
    pub(crate) fn finished(&self, options: &Options) -> bool {
        self.emitted >= options.tuning_lines
    }

    pub(crate) fn next(&self, options: &Options) -> f64 {
        if self.finished(options) {
            f64::INFINITY
        } else {
            (self.emitted + 1) as f64 * options.tuning_interval_ms as f64 / 1000.0
        }
    }

    pub(crate) fn advance(&mut self, options: &Options, now: f64) -> Result<(), RunError> {
        let messages = [
            "Performing autotuning",
            "Beginning GPU tuning for Fake GPU modelVersion 15 channels 512",
            "Tuning xGemmDirect for 1x1 convolutions and matrix mult",
            "Testing 55 different configs",
            "Tuning 40/55 ...",
        ];
        while !self.finished(options) && now >= self.next(options) {
            let line = if self.emitted == 0 && options.backend.as_deref() == Some("tensorrt") {
                "Creating new plan cache"
            } else {
                messages[self.emitted as usize % messages.len()]
            };
            eprintln!("{line} (step {})", self.emitted + 1);
            self.emitted += 1;
        }
        Ok(())
    }
}

/// An installed fake package can behave differently from the original CPU
/// executable without changing the EngineProfile's preserved extra_args.
/// Missing sidecars intentionally keep the ordinary fake CPU behavior.
pub(crate) fn load_script(options: &mut Options) -> Result<(), RunError> {
    let Some(relative) = &options.startup_script_relative else {
        return Ok(());
    };
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or_else(|| RunError::Startup("fake executable has no directory".into()))?;
    let bytes = match std::fs::read(directory.join(relative)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let script: serde_json::Value = serde_json::from_slice(&bytes)?;
    let mut args = vec!["analysis".to_owned()];
    for (field, flag) in [
        ("backend", "--backend"),
        ("tuning_lines", "--tuning-lines"),
        ("tuning_interval_ms", "--tuning-interval-ms"),
        ("startup_error", "--startup-error"),
        ("startup_gate_file", "--startup-gate-file"),
    ] {
        if let Some(value) = script.get(field) {
            args.push(flag.into());
            args.push(
                value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_owned),
            );
        }
    }
    let parsed = Options::parse(args)
        .map_err(|error| RunError::Startup(error.to_string()))?
        .ok_or_else(|| RunError::Startup("empty startup script".into()))?;
    if script.get("backend").is_some() {
        options.backend = parsed.backend;
    }
    if script.get("tuning_lines").is_some() {
        options.tuning_lines = parsed.tuning_lines;
    }
    if script.get("tuning_interval_ms").is_some() {
        options.tuning_interval_ms = parsed.tuning_interval_ms;
    }
    if script.get("startup_error").is_some() {
        options.startup_error = parsed.startup_error;
    }
    if script.get("startup_gate_file").is_some() {
        options.startup_gate_file = parsed.startup_gate_file;
    }
    Ok(())
}
