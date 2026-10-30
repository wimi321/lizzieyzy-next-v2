use serde_json::Value;

use crate::{
    FoxAccount, FoxColor, FoxError, FoxGame, FoxPage, FoxPlayer, FoxResult, FoxResultKind,
    FoxScoreUnit,
};

pub(crate) fn status(value: &Value) -> Result<i64, FoxError> {
    if value.get("result").is_some() {
        integer(value, "result")
    } else {
        integer(value, "errcode")
    }
}

pub(crate) fn success(value: &Value) -> Result<(), FoxError> {
    let code = status(value)?;
    if code == 0 {
        Ok(())
    } else {
        Err(FoxError::Api { code })
    }
}

pub(crate) fn account(value: &Value, query: &str) -> Result<FoxAccount, FoxError> {
    let code = status(value)?;
    let uid = text(value, "uid").unwrap_or_default();
    if code != 0 {
        // 101201 is the missing-account response captured by the Java tests.
        // Other failures with no UID can be outages, so keep them as API errors.
        return if code == 101201 && (uid.is_empty() || uid == "0") {
            Err(FoxError::UserNotFound)
        } else {
            Err(FoxError::Api { code })
        };
    }
    if uid == "0" || crate::validate_uid(&uid).is_err() {
        return Err(FoxError::InvalidResponse("uid"));
    }
    let nickname =
        first_text(value, &["username", "name", "englishname"]).unwrap_or_else(|| query.to_owned());
    Ok(FoxAccount { uid, nickname })
}

pub(crate) fn page(value: &Value, cursor: &str) -> Result<FoxPage, FoxError> {
    success(value)?;
    let rows = value
        .get("chesslist")
        .and_then(Value::as_array)
        .ok_or(FoxError::InvalidResponse("chesslist"))?;
    let games = rows.iter().map(game).collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if rows.len() >= 100 {
        games.last().map(|game| game.id.clone())
    } else {
        None
    };
    if next_cursor.as_deref() == Some(cursor) {
        return Err(FoxError::InvalidResponse("non-advancing cursor"));
    }
    Ok(FoxPage { games, next_cursor })
}

fn game(value: &Value) -> Result<FoxGame, FoxError> {
    let id = required_text(value, "chessid")?;
    crate::validate_game_id(&id).map_err(|_| FoxError::InvalidResponse("chessid"))?;
    Ok(FoxGame {
        id,
        start_time: required_text(value, "starttime")?,
        black: player(value, true)?,
        white: player(value, false)?,
        result: result(value)?,
        move_count: u32::try_from(integer(value, "movenum")?)
            .map_err(|_| FoxError::InvalidResponse("movenum"))?,
    })
}

fn player(value: &Value, black: bool) -> Result<FoxPlayer, FoxError> {
    let (uid_key, rank_key, name_keys) = if black {
        (
            "blackuid",
            "blackdan",
            ["blacknick", "blacknickname", "blackname", "blackenname"],
        )
    } else {
        (
            "whiteuid",
            "whitedan",
            ["whitenick", "whitenickname", "whitename", "whiteenname"],
        )
    };
    Ok(FoxPlayer {
        uid: text(value, uid_key).unwrap_or_default(),
        nickname: first_text(value, &name_keys).unwrap_or_default(),
        rank: format_rank(
            i32::try_from(integer(value, rank_key)?)
                .map_err(|_| FoxError::InvalidResponse(rank_key))?,
        ),
    })
}

/// Fox's online rank offset differs from its professional-rank encoding.
pub fn format_rank(raw: i32) -> String {
    let rank = i64::from(raw);
    if rank >= 100 {
        format!("P{}段", rank - 99)
    } else if rank > 17 {
        format!("{}段", rank - 17)
    } else {
        format!("{}级", 18 - rank)
    }
}

fn result(value: &Value) -> Result<FoxResult, FoxError> {
    let winner = match integer(value, "winner")? {
        1 => Some(FoxColor::Black),
        2 => Some(FoxColor::White),
        _ => None,
    };
    let point = integer(value, "point")?;
    let rule = integer(value, "rule")?;
    let kind = if winner.is_none() {
        FoxResultKind::Other
    } else {
        match point {
            -1 => FoxResultKind::Resignation,
            -2 => FoxResultKind::Timeout,
            0.. => FoxResultKind::Score,
            _ => FoxResultKind::OtherWin,
        }
    };
    let scored = kind == FoxResultKind::Score;
    Ok(FoxResult {
        winner,
        kind,
        score: scored.then_some(point as f64 / 100.0),
        unit: if scored {
            match rule {
                0 => Some(FoxScoreUnit::Points),
                1 => Some(FoxScoreUnit::Stones),
                _ => None,
            }
        } else {
            None
        },
    })
}

fn integer(value: &Value, key: &'static str) -> Result<i64, FoxError> {
    value
        .get(key)
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        .ok_or(FoxError::InvalidResponse(key))
}

fn required_text(value: &Value, key: &'static str) -> Result<String, FoxError> {
    text(value, key)
        .filter(|s| !s.is_empty())
        .ok_or(FoxError::InvalidResponse(key))
}

fn text(value: &Value, key: &str) -> Option<String> {
    match value.get(key)? {
        Value::String(text) => Some(text.trim().to_owned()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn first_text(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| text(value, key))
        .find(|text| !text.is_empty())
}
