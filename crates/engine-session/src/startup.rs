//! Shared startup timing for normal sessions and application trial sessions.
use std::time::Duration;
use tokio::time::Instant;

/// Recognize the real OpenCL/TensorRT progress messages, with or without a
/// KataGo timestamp. Completion and dummy-thread summaries are not progress.
pub fn startup_extension(line: &str) -> Option<Duration> {
    let line = message(line.trim());
    if line.starts_with("Performing autotuning") || line.starts_with("Creating new plan cache") {
        return Some(Duration::from_secs(600));
    }
    if line.starts_with("Tuning dummy thread numeric total") {
        return None;
    }
    let testing = line
        .strip_prefix("Testing ")
        .and_then(|line| line.strip_suffix(" different configs"))
        .is_some_and(|count| count.parse::<u64>().is_ok_and(|count| count > 0));
    (line.starts_with("Beginning GPU tuning") || line.starts_with("Tuning ") || testing)
        .then_some(Duration::from_secs(120))
}

fn message(line: &str) -> &str {
    // KataGo writes `YYYY-MM-DD HH:MM:SS[zone]: message`. The split is at the
    // first colon followed by a space, never at one of the clock's colons.
    let bytes = line.as_bytes();
    if bytes.len() >= 19
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b' '
        && bytes[..4].iter().all(u8::is_ascii_digit)
    {
        line.split_once(": ").map_or(line, |(_, message)| message)
    } else {
        line
    }
}

/// An attempt's deadline only moves forward to `now + extension`, never by
/// adding an extension to its previous value. Expired attempts cannot revive.
#[derive(Clone, Debug)]
pub struct StartupDeadline {
    deadline: Instant,
    limit: Instant,
}

impl StartupDeadline {
    pub fn new(started: Instant, timeout: Duration, maximum: Duration) -> Self {
        // SessionConfig validates durations before use. Saturating to the start
        // also makes independently constructed invalid deadlines fail closed.
        let limit = started.checked_add(maximum).unwrap_or(started);
        let deadline = started.checked_add(timeout).unwrap_or(started).min(limit);
        Self { deadline, limit }
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// True means a recognized progress line arrived before expiration, even
    /// if its grace period was already covered by the current deadline.
    pub fn observe(&mut self, line: &str, now: Instant) -> bool {
        if now >= self.deadline {
            return false;
        }
        let Some(extension) = startup_extension(line) else {
            return false;
        };
        let proposed = now.checked_add(extension).unwrap_or(self.limit);
        self.deadline = self.deadline.max(proposed).min(self.limit);
        true
    }
}

pub(crate) fn bounded_line(mut line: String) -> String {
    let mut end = line.len().min(4096);
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    line.truncate(end);
    line
}

#[cfg(test)]
mod tests;
