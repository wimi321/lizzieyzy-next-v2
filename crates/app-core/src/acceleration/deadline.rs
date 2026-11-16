//! Backend-specific base limits; extension policy belongs to engine-session.
use crate::AccelerationBackend;
use std::time::Duration;

pub(crate) fn timeout(backend: AccelerationBackend, config: &crate::AppConfig) -> Duration {
    #[cfg(test)]
    if let Some(timeout) = config.acceleration_timeout {
        return timeout;
    }
    #[cfg(not(test))]
    let _ = config;
    match backend {
        AccelerationBackend::Cuda => Duration::from_secs(180),
        AccelerationBackend::TensorRt | AccelerationBackend::OpenCl => Duration::from_secs(600),
    }
}

#[cfg(test)]
mod tests;
