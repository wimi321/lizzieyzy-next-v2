//! Startup log interpretation is independent of the platform and process I/O.
use crate::EngineBackend;
use std::collections::VecDeque;

const TAIL_LINES: usize = 20;
const LINE_BYTES: usize = 4096;

/// Only known startup messages extend the OpenCL tuning deadline.
pub(crate) fn is_tuning_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "performing autotuning",
        "tuning xgemmdirect",
        "started opencl sgemm",
        "this will take a while",
        "long time",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// This recognizes what KataGo reported, independently of the selected asset.
pub fn reported_backend(line: &str) -> Option<EngineBackend> {
    let lower = line.to_ascii_lowercase();
    if lower.contains("tensorrt backend thread") {
        Some(EngineBackend::TensorRt)
    } else if lower.contains("cuda backend thread") && lower.contains("found gpu") {
        Some(EngineBackend::Cuda)
    } else if lower.contains("opencl backend thread") || lower.contains("using opencl device") {
        Some(EngineBackend::OpenCl)
    } else if lower.contains("eigen (cpu)") {
        Some(EngineBackend::Eigen)
    } else {
        None
    }
}

/// Bounded diagnostics: lines and bytes are both capped, including Unicode.
#[derive(Default)]
pub(crate) struct OutputTail {
    lines: VecDeque<String>,
}

impl OutputTail {
    pub(crate) fn push(&mut self, line: &str) {
        for line in line.lines() {
            let mut end = line.len().min(LINE_BYTES);
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            if self.lines.len() == TAIL_LINES {
                self.lines.pop_front();
            }
            self.lines.push_back(line[..end].to_owned());
        }
    }

    pub(crate) fn lines(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests;
