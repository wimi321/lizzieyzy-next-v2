use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum ZhiziErrorKind {
    InvalidAccount,
    InvalidCredentials,
    InvalidVerificationCode,
    TooFrequent,
    PasswordTooShort,
    AccountExists,
    AccountNotFound,
    LoginRequired,
    LoginExpired,
    AllocationUnavailable,
    InsufficientBalance,
    VipExpired,
    Network,
    IncompleteResponse,
    UnsafeAddress,
    Closed,
    Unknown(String),
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ZhiziError {
    #[ts(as = "Option<ZhiziErrorKind>", optional)]
    pub kind: ZhiziErrorKind,
    #[ts(as = "Option<u16>", optional)]
    pub status: u16,
    #[ts(as = "Option<String>", optional)]
    pub request_id: String,
    #[ts(as = "Option<u32>", optional)]
    pub retry_after_seconds: u32,
}

impl ZhiziError {
    pub(crate) fn new(kind: ZhiziErrorKind) -> Self {
        Self {
            kind,
            status: 0,
            request_id: String::new(),
            retry_after_seconds: 0,
        }
    }
    pub fn is_unauthorized(&self) -> bool {
        self.status == 401 || self.kind == ZhiziErrorKind::LoginExpired
    }
    pub fn is_fatal(&self) -> bool {
        matches!(self.status, 401 | 403)
            || matches!(
                self.kind,
                ZhiziErrorKind::LoginRequired
                    | ZhiziErrorKind::LoginExpired
                    | ZhiziErrorKind::InsufficientBalance
                    | ZhiziErrorKind::VipExpired
                    | ZhiziErrorKind::UnsafeAddress
            )
    }
    pub(crate) fn from_key(key: &str, status: u16) -> Self {
        use ZhiziErrorKind::*;
        let kind = match key {
            "invalid_phone" | "invalid_email" => InvalidAccount,
            "invalid_password" | "invalid_credentials" => InvalidCredentials,
            "invalid_verification_code" => InvalidVerificationCode,
            "fast_login_too_frequent" | "too_many_requests" => TooFrequent,
            "password_too_short" => PasswordTooShort,
            "duplicate_phone" | "duplicate_email" => AccountExists,
            "not_found" => AccountNotFound,
            "unauthorized" => LoginExpired,
            "failed_to_insert_socket_io_token" => AllocationUnavailable,
            "insufficient_balance" | "balance_not_enough" => InsufficientBalance,
            "vip_expired" | "membership_expired" | "not_membership" => VipExpired,
            "network_error" => Network,
            "invalid_response" => IncompleteResponse,
            _ if status == 429 => TooFrequent,
            _ => Unknown(key.to_owned()),
        };
        Self {
            status,
            ..Self::new(kind)
        }
    }
}

// Unknown server keys remain available to the UI, but are not free-text logs.
impl std::fmt::Debug for ZhiziError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZhiziError")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for ZhiziError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.kind == ZhiziErrorKind::LoginRequired {
            f.write_str("请先登录智子")
        } else {
            write!(f, "Zhizi request failed (HTTP {})", self.status)
        }
    }
}
impl std::error::Error for ZhiziError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ZhiziVerificationPurpose {
    #[default]
    FastLogin,
    ResetPassword,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ZhiziStatus {
    #[ts(as = "Option<bool>", optional)]
    pub logged_in: bool,
    #[ts(as = "Option<bool>", optional)]
    pub remembered: bool,
    #[ts(as = "Option<bool>", optional)]
    pub store_available: bool,
    /// Code login can register a previously unknown account; it is not proof
    /// that this particular response created a new account.
    #[ts(as = "Option<bool>", optional)]
    pub may_register_account: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ZhiziCodeDelivery {
    #[ts(as = "Option<u32>", optional)]
    pub retry_after_seconds: u32,
    #[ts(as = "Option<bool>", optional)]
    pub may_register_account: bool,
}

#[derive(Clone)]
pub struct ZhiziSocketToken {
    pub url: String,
    pub token: String,
}
impl std::fmt::Debug for ZhiziSocketToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ZhiziSocketToken([redacted])")
    }
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ZhiziAccountProfile {
    #[ts(as = "Option<String>", optional)]
    pub phone: String,
    #[ts(as = "Option<String>", optional)]
    pub email: String,
    #[ts(as = "Option<bool>", optional)]
    pub membership: bool,
    #[ts(optional = nullable)]
    pub membership_expires_at: Option<String>,
    #[ts(optional = nullable)]
    pub membership_auto_renew: Option<bool>,
}
impl std::fmt::Debug for ZhiziAccountProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ZhiziAccountProfile([redacted])")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ZhiziBalance {
    #[ts(as = "Option<String>", optional)]
    pub remaining_balance_yuan: String,
    #[ts(as = "Option<String>", optional)]
    pub total_cash_amount_yuan: String,
    #[ts(as = "Option<String>", optional)]
    pub yesterday_consumption_yuan: String,
    #[ts(as = "Option<String>", optional)]
    pub total_consumption_yuan: String,
    #[ts(as = "Option<f64>", optional)]
    pub total_duration_seconds: u64,
    #[ts(as = "Option<f64>", optional)]
    pub last_24_hours_share_duration_seconds: u64,
    #[ts(as = "Option<f64>", optional)]
    pub last_24_hours_vip_duration_seconds: u64,
    #[ts(as = "Option<u32>", optional)]
    pub current_connections: u32,
    #[ts(as = "Option<u32>", optional)]
    pub current_nodes: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ZhiziUsageRecord {
    #[ts(optional = nullable)]
    pub started_at: Option<String>,
    #[ts(optional = nullable)]
    pub ended_at: Option<String>,
    #[ts(as = "Option<bool>", optional)]
    pub finished: bool,
    #[ts(as = "Option<bool>", optional)]
    pub ready: bool,
    #[ts(as = "Option<f64>", optional)]
    pub duration_seconds: u64,
    #[ts(as = "Option<String>", optional)]
    pub total_cost_yuan: String,
    #[ts(as = "Option<String>", optional)]
    pub gpu_type: String,
    #[ts(as = "Option<String>", optional)]
    pub engine_type: String,
    #[ts(as = "Option<bool>", optional)]
    pub vip: bool,
    #[ts(as = "Option<bool>", optional)]
    pub shared: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ZhiziCreditRecord {
    #[ts(as = "Option<String>", optional)]
    pub credit_type: String,
    #[ts(as = "Option<String>", optional)]
    pub amount_yuan: String,
    #[ts(as = "Option<String>", optional)]
    pub source: String,
    #[ts(as = "Option<String>", optional)]
    pub product_name: String,
    #[ts(optional = nullable)]
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ZhiziUsagePage {
    #[ts(as = "Option<f64>", optional)]
    pub total: u64,
    #[ts(as = "Option<u32>", optional)]
    pub page: u32,
    #[ts(as = "Option<u32>", optional)]
    pub page_size: u32,
    #[ts(as = "Option<Vec<ZhiziUsageRecord>>", optional)]
    pub items: Vec<ZhiziUsageRecord>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ZhiziCreditPage {
    #[ts(as = "Option<f64>", optional)]
    pub total: u64,
    #[ts(as = "Option<u32>", optional)]
    pub page: u32,
    #[ts(as = "Option<u32>", optional)]
    pub page_size: u32,
    #[ts(as = "Option<Vec<ZhiziCreditRecord>>", optional)]
    pub items: Vec<ZhiziCreditRecord>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ZhiziOverview {
    #[ts(as = "Option<ZhiziAccountProfile>", optional)]
    pub account: ZhiziAccountProfile,
    #[ts(as = "Option<ZhiziBalance>", optional)]
    pub balance: ZhiziBalance,
    #[ts(as = "Option<ZhiziUsagePage>", optional)]
    pub recent_usage: ZhiziUsagePage,
}
