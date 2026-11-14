//! Account deadlines are evaluated independently of HTTP and secret-store I/O.
use crate::zhizi::{ZhiziError, ZhiziErrorKind, ZhiziOverview};
use std::{collections::HashMap, time::Duration};
use tokio::time::Instant;

#[derive(Default)]
pub(super) struct AccountTiming {
    overview: Option<(Instant, ZhiziOverview)>,
    cooldowns: HashMap<String, Instant>,
}

impl AccountTiming {
    pub(super) fn check_code(&mut self, identity: &str, now: Instant) -> Result<(), ZhiziError> {
        self.cooldowns.retain(|_, deadline| *deadline > now);
        let delay = if let Some(deadline) = self.cooldowns.get(identity) {
            Some((*deadline - now).as_secs_f64().ceil() as u32)
        } else if self.cooldowns.len() >= 128 {
            Some(600)
        } else {
            None
        };
        match delay {
            Some(delay) => Err(ZhiziError {
                retry_after_seconds: delay,
                ..ZhiziError::new(ZhiziErrorKind::TooFrequent)
            }),
            None => Ok(()),
        }
    }

    pub(super) fn remember_code(&mut self, identity: String, seconds: u32, now: Instant) {
        self.cooldowns
            .insert(identity, now + Duration::from_secs(u64::from(seconds)));
    }

    pub(super) fn cached_overview(&self, force: bool, now: Instant) -> Option<ZhiziOverview> {
        self.overview.as_ref().and_then(|(loaded, value)| {
            (!force && now.duration_since(*loaded) < Duration::from_secs(45)).then(|| value.clone())
        })
    }

    pub(super) fn cache_overview(&mut self, overview: ZhiziOverview, now: Instant) {
        self.overview = Some((now, overview));
    }

    pub(super) fn clear_overview(&mut self) {
        self.overview = None;
    }
}

#[cfg(test)]
mod tests;
