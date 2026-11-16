use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

const TOKEN_LENGTH: usize = 22;

pub(crate) fn random_token() -> Result<String, super::WebBoardError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| super::WebBoardError::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// Compare every byte of the fixed-size credential, without prefix short circuiting.
pub(crate) fn token_matches(expected: &str, supplied: &str) -> bool {
    let Ok(expected): Result<&[u8; TOKEN_LENGTH], _> = expected.as_bytes().try_into() else {
        return false;
    };
    let Ok(supplied): Result<&[u8; TOKEN_LENGTH], _> = supplied.as_bytes().try_into() else {
        return false;
    };
    let mut difference = 0_u8;
    for index in 0..TOKEN_LENGTH {
        difference |= expected[index] ^ supplied[index];
    }
    difference == 0
}

/// Only canonical, unescaped resource paths are accepted. Percent escapes are
/// unnecessary for the generated ASCII asset names and are rejected wholesale.
pub(super) fn resource_path<'a>(path: &'a str, token: &str) -> Option<&'a str> {
    if path.contains("..") || path.contains(['\\', '%']) {
        return None;
    }
    let (supplied, rest) = path.strip_prefix('/')?.split_once('/')?;
    if !token_matches(token, supplied) {
        return None;
    }
    match rest {
        "" => Some("web.html"),
        "ws" => Some("ws"),
        value if value.starts_with("assets/") && !value.ends_with('/') => {
            if value.split('/').any(str::is_empty) {
                None
            } else {
                Some(value)
            }
        }
        _ => None,
    }
}
