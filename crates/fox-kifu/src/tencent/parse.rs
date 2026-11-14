use chrono::NaiveDateTime;
use serde_json::Value;

use super::{TencentError, TencentGame, TencentPage, TencentPlayer, TencentRank};
use crate::{FoxError, parse, validate_game_id};

pub(super) fn success(value: &Value) -> Result<(), TencentError> {
    let result = parse::integer(value, "result")?;
    let ret = value
        .get("ret")
        .map(|_| parse::integer(value, "ret"))
        .transpose()?;
    if result != 0 || ret.is_some_and(|ret| ret != 0) {
        return Err(TencentError::Api {
            result,
            ret,
            message: value
                .get("resultstr")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        });
    }
    Ok(())
}

pub(super) fn page(value: &Value, cursor: &str) -> Result<TencentPage, TencentError> {
    success(value)?;
    let rows = value
        .get("chesslist")
        .and_then(Value::as_array)
        .filter(|rows| rows.len() <= 100)
        .ok_or(FoxError::InvalidResponse("chesslist"))?;
    let games = rows.iter().map(game).collect::<Result<Vec<_>, _>>()?;
    // Recorded pages contained 50 games despite fetchnum=100. A short page
    // must still advance; only an empty response establishes the end.
    let next_cursor = games.last().map(|game| game.id.clone());
    if next_cursor.as_deref() == Some(cursor) {
        return Err(FoxError::InvalidResponse("non-advancing cursor").into());
    }
    Ok(TencentPage { games, next_cursor })
}

fn game(value: &Value) -> Result<TencentGame, TencentError> {
    let id = parse::required_text(value, "chessid")?;
    validate_game_id(&id).map_err(|_| FoxError::InvalidResponse("chessid"))?;
    Ok(TencentGame {
        id,
        start_time: start_time(value),
        black: player(value, true)?,
        white: player(value, false)?,
        result: parse::result(value)?,
        move_count: u32::try_from(parse::integer(value, "movenum")?)
            .map_err(|_| FoxError::InvalidResponse("movenum"))?,
    })
}

fn player(value: &Value, black: bool) -> Result<TencentPlayer, TencentError> {
    let (uid, rank, names) = if black {
        (
            "blackuid",
            "blackdan",
            ["blacknick", "blackenname", "blackname"],
        )
    } else {
        (
            "whiteuid",
            "whitedan",
            ["whitenick", "whiteenname", "whitename"],
        )
    };
    Ok(TencentPlayer {
        uid: parse::text(value, uid).unwrap_or_default(),
        nickname: parse::first_text(value, &names).unwrap_or_default(),
        rank: TencentRank::from_raw(
            i32::try_from(parse::integer(value, rank)?)
                .map_err(|_| FoxError::InvalidResponse(rank))?,
        ),
    })
}

fn seconds(value: &Value) -> Option<i64> {
    let value = value.as_i64().or_else(|| value.as_str()?.parse().ok())?;
    // Reject timestamps outside the calendar range; negative Unix seconds are valid.
    chrono::DateTime::from_timestamp(value, 0)
        .is_some()
        .then_some(value)
}

fn start_time(value: &Value) -> Option<i64> {
    if let Some(time) = value.get("gamestarttime").and_then(seconds) {
        return Some(time);
    }
    let raw = value.get("starttime")?;
    if let Some(time) = seconds(raw) {
        return Some(time);
    }
    let raw = raw.as_str()?;
    // This wire string is Beijing local time, independent of the host timezone.
    if raw.len() != 19
        || !raw.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            10 => byte == b' ',
            13 | 16 => byte == b':',
            _ => byte.is_ascii_digit(),
        })
    {
        return None;
    }
    NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
        .ok()?
        .and_utc()
        .timestamp()
        .checked_sub(8 * 60 * 60)
}
