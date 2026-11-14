//! Java-compatible independently replaceable SGF C-property commentary blocks.

use crate::{Error, MAX_ANSWER_CHARACTERS};

pub const BEGIN: &str = "[LizzieYzy AI Commentary BEGIN]";
pub const END: &str = "[LizzieYzy AI Commentary END]";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentaryBlock {
    pub generated_at: String,
    pub model: String,
    /// Marker escape spellings remain literal, as in the Java reader.
    pub body: String,
}

fn normalize(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn escape(value: &str) -> String {
    value
        .replace(BEGIN, "[LizzieYzy AI Commentary BEGIN escaped]")
        .replace(END, "[LizzieYzy AI Commentary END escaped]")
}

fn metadata(value: &str) -> String {
    escape(&normalize(value))
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .trim()
        .into()
}

pub fn encode(body: &str, model: &str, generated_at: &str) -> Result<String, Error> {
    let body = escape(&normalize(body));
    let body = body.trim();
    if body.is_empty() {
        return Err(Error::EmptyCommentary);
    }
    if body.chars().count() > MAX_ANSWER_CHARACTERS {
        return Err(Error::CommentaryTooLong);
    }
    Ok(format!(
        "{BEGIN}\ngeneratedAt={}\nmodel={}\n\n{body}\n{END}",
        metadata(generated_at),
        metadata(model)
    ))
}

/// Reads the last complete block; unfinished user text is not silently removed.
pub fn decode(comment: &str) -> Option<CommentaryBlock> {
    let source = normalize(comment);
    let mut search_from = source.len();
    while search_from > 0 {
        let start = source[..search_from].rfind(BEGIN)?;
        let after_start = start + BEGIN.len();
        if let Some(end) = source[after_start..].find(END) {
            let block = source[after_start..after_start + end].trim();
            let (header, body) = block.split_once("\n\n").unwrap_or(("", block));
            let body = body.trim();
            if body.is_empty() || body.chars().count() > MAX_ANSWER_CHARACTERS {
                return None;
            }
            let mut decoded = CommentaryBlock {
                generated_at: String::new(),
                model: String::new(),
                body: body.into(),
            };
            for line in header.lines() {
                if let Some(value) = line.strip_prefix("generatedAt=") {
                    decoded.generated_at = value.into();
                }
                if let Some(value) = line.strip_prefix("model=") {
                    decoded.model = value.into();
                }
            }
            return Some(decoded);
        }
        search_from = start;
    }
    None
}

/// Preserves non-AI text byte-for-byte, including unfinished marker text and CRLFs.
pub fn remove_blocks(comment: &str) -> String {
    let mut remaining = comment.to_owned();
    let mut search_from = remaining.len();
    while search_from > 0 {
        let Some(start) = remaining[..search_from].rfind(BEGIN) else {
            break;
        };
        let after_start = start + BEGIN.len();
        let Some(end) = remaining[after_start..].find(END) else {
            search_from = start;
            continue;
        };
        let end = after_start + end + END.len();
        let remove_start = if remaining[..start].ends_with("\n\n") {
            start - 2
        } else {
            start
        };
        remaining.replace_range(remove_start..end, "");
        search_from = remove_start;
    }
    remaining
}

pub fn upsert(
    original: &str,
    body: &str,
    model: &str,
    generated_at: &str,
) -> Result<String, Error> {
    let block = encode(body, model, generated_at)?;
    let clean = remove_blocks(original);
    Ok(if clean.is_empty() {
        block
    } else {
        format!("{clean}\n\n{block}")
    })
}

/// Appends within the same AI block, retaining the original explanation and earlier follow-ups.
pub fn append_follow_up(
    original: &str,
    question: &str,
    answer: &str,
    model: &str,
    generated_at: &str,
) -> Result<String, Error> {
    let previous = decode(original).ok_or(Error::MissingCommentary)?;
    if answer.trim().is_empty() {
        return Err(Error::EmptyCommentary);
    }
    let body = format!(
        "{}\n\n追问：{}\n\n{}",
        previous.body,
        question.trim(),
        answer.trim()
    );
    upsert(original, &body, model, generated_at)
}
