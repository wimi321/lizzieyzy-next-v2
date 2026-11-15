use super::{Response, Value};
use crate::zhizi::*;

fn text(v: &Value, name: &str) -> String {
    optional_text(v, name).unwrap_or_default()
}
fn optional_text(v: &Value, name: &str) -> Option<String> {
    v.get(name).and_then(Value::as_str).map(str::to_owned)
}
fn boolean(v: &Value, name: &str) -> bool {
    v.get(name).and_then(Value::as_bool).unwrap_or(false)
}
fn integer(v: &Value, name: &str) -> u64 {
    v.get(name).and_then(Value::as_u64).unwrap_or(0)
}
fn decimal(v: &Value, name: &str) -> String {
    match v.get(name) {
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::String(number)) if number.parse::<f64>().is_ok_and(f64::is_finite) => {
            number.clone()
        }
        _ => "0".to_owned(),
    }
}
pub(super) fn account(r: &Response) -> Result<ZhiziAccountProfile, ZhiziError> {
    let v = &r.value;
    if v.get("phone").and_then(Value::as_str).is_none()
        && v.get("email").and_then(Value::as_str).is_none()
    {
        return Err(r.incomplete());
    }
    Ok(ZhiziAccountProfile {
        phone: text(v, "phone"),
        email: text(v, "email"),
        membership: boolean(v, "isMembership"),
        membership_expires_at: optional_text(v, "membershipExpiresAt"),
        membership_auto_renew: v.get("membershipAutoRenew").and_then(Value::as_bool),
    })
}
pub(super) fn balance(r: &Response) -> Result<ZhiziBalance, ZhiziError> {
    let v = &r.value;
    if v.get("remainingBalance").is_none() {
        return Err(r.incomplete());
    }
    Ok(ZhiziBalance {
        remaining_balance_yuan: decimal(v, "remainingBalance"),
        total_cash_amount_yuan: decimal(v, "totalCashAmount"),
        yesterday_consumption_yuan: decimal(v, "yesterdayConsumption"),
        total_consumption_yuan: decimal(v, "totalConsumption"),
        total_duration_seconds: integer(v, "totalDuration"),
        last_24_hours_share_duration_seconds: integer(v, "last24HrsShareDuration"),
        last_24_hours_vip_duration_seconds: integer(v, "last24HrsVIPShareDuration"),
        current_connections: integer(v, "currentNumOfMyConnections").min(u64::from(u32::MAX))
            as u32,
        current_nodes: integer(v, "currentNumOfNodes").min(u64::from(u32::MAX)) as u32,
    })
}
pub(super) fn usages(
    r: &Response,
    page: u32,
    page_size: u32,
) -> Result<ZhiziUsagePage, ZhiziError> {
    let values = r
        .value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| r.incomplete())?;
    if values.len() > page_size as usize || values.iter().any(|v| !v.is_object()) {
        return Err(r.incomplete());
    }
    let items = values
        .iter()
        .map(|v| ZhiziUsageRecord {
            started_at: optional_text(v, "startedAt"),
            ended_at: optional_text(v, "endedAt"),
            finished: boolean(v, "finished"),
            ready: boolean(v, "ready"),
            duration_seconds: integer(v, "duration"),
            total_cost_yuan: decimal(v, "totalCost"),
            gpu_type: text(v, "gpuType"),
            engine_type: text(v, "engineType"),
            vip: boolean(v, "vip"),
            shared: boolean(v, "share"),
        })
        .collect();
    Ok(ZhiziUsagePage {
        total: integer(&r.value, "total"),
        page,
        page_size,
        items,
    })
}
pub(super) fn credits(r: &Response, page: u32) -> Result<ZhiziCreditPage, ZhiziError> {
    let values = r
        .value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| r.incomplete())?;
    if values.len() > 20 || values.iter().any(|v| !v.is_object()) {
        return Err(r.incomplete());
    }
    let items = values
        .iter()
        .map(|v| ZhiziCreditRecord {
            credit_type: text(v, "creditType"),
            amount_yuan: decimal(v, "amount"),
            source: text(v, "source"),
            product_name: text(v, "productName"),
            created_at: optional_text(v, "createdAt"),
        })
        .collect();
    Ok(ZhiziCreditPage {
        total: integer(&r.value, "total"),
        page,
        page_size: 20,
        items,
    })
}
