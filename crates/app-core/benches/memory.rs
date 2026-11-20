//! Peak resident memory of this benchmark worker, excluding its child engine.
//!
//! The worker must run only one scenario: these OS counters are lifetime high
//! water marks and cannot be reset between measurements.

use std::io;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn peak_rss_bytes() -> io::Result<u64> {
    use nix::sys::resource::{UsageWho, getrusage};

    let usage = getrusage(UsageWho::RUSAGE_SELF).map_err(io::Error::from)?;
    let peak = u64::try_from(usage.max_rss()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "getrusage returned negative RSS",
        )
    })?;
    // Darwin reports ru_maxrss in bytes; Linux reports KiB, independently of
    // the machine's page size. RUSAGE_SELF excludes the fake engine process.
    #[cfg(target_os = "linux")]
    let peak = peak.checked_mul(1024).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "getrusage RSS overflows bytes")
    })?;
    nonzero_peak(peak)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn peak_rss_bytes() -> io::Result<u64> {
    // Windows intentionally has no RSS adapter: keep unsafe forbidden and do
    // not add a third-party wrapper solely for benchmark memory measurements.
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "peak RSS is measured only on macOS and Linux; Windows 不测",
    ))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn nonzero_peak(peak: u64) -> io::Result<u64> {
    if peak == 0 {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the OS returned a zero peak RSS for the running benchmark worker",
        ))
    } else {
        Ok(peak)
    }
}
