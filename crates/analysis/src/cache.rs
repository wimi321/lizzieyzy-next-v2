use std::{collections::VecDeque, num::NonZeroUsize};

use document::AnalysisKey;
use katago_protocol::AnalysisResponse;

struct Entry {
    key: AnalysisKey,
    fingerprint: String,
    response: AnalysisResponse,
}

/// Actor-owned, bounded LRU. Reading or refreshing an entry marks it as used.
pub(crate) struct Cache {
    capacity: NonZeroUsize,
    entries: VecDeque<Entry>,
}

impl Cache {
    pub(crate) fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            entries: VecDeque::new(),
        }
    }

    pub(crate) fn get(&mut self, key: AnalysisKey, fingerprint: &str) -> Option<AnalysisResponse> {
        let index = self.find(key, fingerprint)?;
        let entry = self.entries.remove(index)?;
        let response = entry.response.clone();
        self.entries.push_back(entry);
        Some(response)
    }

    pub(crate) fn insert(
        &mut self,
        key: AnalysisKey,
        fingerprint: &str,
        response: AnalysisResponse,
    ) {
        let previous = self
            .find(key, fingerprint)
            .and_then(|index| self.entries.remove(index));
        let response = match previous {
            Some(entry) if entry.response.root_info.visits > response.root_info.visits => {
                entry.response
            }
            _ => response,
        };
        if self.entries.len() == self.capacity.get() {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry {
            key,
            fingerprint: fingerprint.to_owned(),
            response,
        });
    }

    fn find(&self, key: AnalysisKey, fingerprint: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.key == key && entry.fingerprint == fingerprint)
    }
}

#[cfg(test)]
mod tests;
