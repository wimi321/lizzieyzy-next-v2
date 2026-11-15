//! Zhizi account boundary: credentials never enter settings or diagnostics.
mod client;
mod service;
mod store;
mod types;

pub use client::ZhiziClient;
pub use service::ZhiziService;
pub use types::*;

/// Purchasing is completed by the user in their system browser.
pub fn website_url() -> &'static str {
    "https://www.zhizigo.com/"
}
