use crate::{Error, Evidence, MAX_PROMPT_CHARACTERS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    ZhCn,
    ZhTw,
    En,
    Ja,
    Ko,
    Th,
}

impl Language {
    pub(crate) fn is_chinese(self) -> bool {
        matches!(self, Self::ZhCn | Self::ZhTw)
    }

    fn name(self) -> &'static str {
        match self {
            Self::ZhCn => "简体中文",
            Self::ZhTw => "繁體中文",
            Self::En => "English",
            Self::Ja => "Japanese",
            Self::Ko => "Korean",
            Self::Th => "Thai",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    NextMove,
    Range,
    WholeGame,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Balanced,
    Rigorous,
    Patient,
    Strict,
    Humorous,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Terminology {
    Few,
    #[default]
    Medium,
    Many,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Detail {
    Brief,
    #[default]
    Standard,
    Detailed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Variations {
    Few,
    #[default]
    Moderate,
    Many,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub rank: String,
    pub style: Style,
    pub terminology: Terminology,
    pub detail: Detail,
    pub variations: Variations,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            rank: "5k".into(),
            style: Style::default(),
            terminology: Terminology::default(),
            detail: Detail::default(),
            variations: Variations::default(),
        }
    }
}

impl Preferences {
    pub fn validate(&self) -> Result<(), Error> {
        let rank = self.rank.as_bytes();
        let Some((&suffix, digits)) = rank.split_last() else {
            return Err(Error::InvalidPreferences);
        };
        let number = std::str::from_utf8(digits)
            .ok()
            .and_then(|text| text.parse::<u8>().ok());
        if digits.is_empty()
            || digits[0] == b'0'
            || !digits.iter().all(u8::is_ascii_digit)
            || !matches!((suffix, number), (b'k', Some(1..=18)) | (b'd', Some(1..=9)))
        {
            return Err(Error::InvalidPreferences);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub role: &'static str,
    pub content: String,
}

#[derive(Clone, Copy, Debug)]
pub struct FollowUp<'a> {
    pub previous_answer: &'a str,
    pub question: &'a str,
}

fn persona(preferences: &Preferences, language: Language) -> String {
    if language.is_chinese() {
        let style = match preferences.style {
            Style::Balanced => "平衡",
            Style::Rigorous => "严谨",
            Style::Patient => "耐心",
            Style::Strict => "严格",
            Style::Humorous => "幽默",
        };
        let terms = match preferences.terminology {
            Terminology::Few => "少",
            Terminology::Medium => "中",
            Terminology::Many => "多",
        };
        let detail = match preferences.detail {
            Detail::Brief => "简略",
            Detail::Standard => "标准",
            Detail::Detailed => "详细",
        };
        let variations = match preferences.variations {
            Variations::Few => "少",
            Variations::Moderate => "适中",
            Variations::Many => "多",
        };
        format!(
            "用户水平={}；风格={style}；术语={terms}；详略={detail}；变化={variations}。只用水平调整表达，不在回答中评定或提及段位。",
            preferences.rank
        )
    } else {
        format!(
            "User level={}; style={:?}; terminology={:?}; detail={:?}; variations={:?}. Adapt the explanation to this level without mentioning or judging the person's rank.",
            preferences.rank,
            preferences.style,
            preferences.terminology,
            preferences.detail,
            preferences.variations
        )
    }
}

fn system(language: Language, preferences: &Preferences) -> String {
    let instructions = if language.is_chinese() {
        "帮助用户理解 KataGo 的判断，不用个人偏好替代它。只使用给出的依据，不编造坐标、变化、胜率、目差、意图和结果；证据不足就明确说明。禁止指控作弊、评定段位或自称老师、教练。事实与解读分开。\n目差 ≤0.5 视为相当，0.5–1.5 略好，>1.5 值得解释；胜率是次要的。胜率与目差都已经换成各局面轮到的一方的视角，不要再次翻转。\n每个局面讲一个具体的决定，联系关联棋块的具体棋子与已重放的变化。先讲棋盘上的道理，再讲胜率；最多一个数字，不罗列候选统计；2–4 个短段落，最多一条短变化。没有实际着手时解释计划，不编造错误。范围和整盘只讲选中的局面，不声称略去部分已经讲解。\n块和气只是几何事实，不证明死活、先手、征子或强制杀棋；实战后续不是最佳防守，候选变化不是强制顺序。只叙述成功重放的前缀。只有依据着手和明确关联棋块的子可作为回答坐标；不要把未点名的远处棋子或单纯气点当作推荐。损失未知时不制造数字。"
    } else {
        "Explain KataGo's judgment without replacing it with personal preferences. Use only the supplied evidence; never invent coordinates, variations, winrates, score leads, intentions or results. State missing evidence plainly. Do not accuse anyone of cheating, assess rank, or call yourself a teacher or coach. Separate facts from interpretation.\nTreat score differences <=0.5 as equivalent, 0.5–1.5 as a small preference, and >1.5 as worth explaining. Winrate is secondary. Winrates and score leads are already from the side-to-play perspective explicitly named for each position; do not flip them again.\nExplain one concrete decision per position, anchored to named stones/groups and the replay. Lead with board reasoning before winrate. Use at most one number; do not enumerate candidate statistics. Write 2–4 short paragraphs, with at most one short illustrative line. Without an actual move, explain a plan without inventing a mistake. Review only selected positions; do not pretend omitted positions were explained.\nGroups and liberties are geometry, not proof of life/death, sente, ladders or forced kills. Recorded continuation is not necessarily best defense; candidate variations are not forced sequences. Narrate only successfully replayed prefixes. Use coordinates only from supplied moves or explicitly named groups, not unrelated board stones or mere liberties. Unknown loss must remain unknown."
    };
    format!(
        "{instructions}\n{}\n{}{}.",
        persona(preferences, language),
        if language.is_chinese() {
            "全程使用"
        } else {
            "Reply in "
        },
        language.name()
    )
}

fn bounded(messages: Vec<Message>) -> Result<Vec<Message>, Error> {
    if messages
        .iter()
        .map(|message| message.content.chars().count())
        .sum::<usize>()
        > MAX_PROMPT_CHARACTERS
    {
        Err(Error::PromptTooLong)
    } else {
        Ok(messages)
    }
}

pub fn build_prompt(
    evidence: &Evidence,
    mode: Mode,
    language: Language,
    preferences: &Preferences,
    follow_up: Option<FollowUp<'_>>,
) -> Result<Vec<Message>, Error> {
    preferences.validate()?;
    let mode_text = match (language.is_chinese(), mode) {
        (true, Mode::NextMove) => "解释下一手的一个关键决定。",
        (true, Mode::Range) => "解释所选范围的关键局面，合并重复的要点。",
        (true, Mode::WholeGame) => "按时间顺序解释整盘选中的关键局面，说明覆盖范围。",
        (false, Mode::NextMove) => "Explain one important decision for the next move.",
        (false, Mode::Range) => "Review the selected range and combine recurring lessons.",
        (false, Mode::WholeGame) => {
            "Review the selected key moments of the whole game chronologically and state coverage."
        }
    };
    let mut messages = vec![
        Message {
            role: "system",
            content: system(language, preferences),
        },
        Message {
            role: "user",
            content: format!("{mode_text}\n\n[KataGo evidence]\n{}", evidence.text),
        },
    ];
    if let Some(follow_up) = follow_up {
        let previous = follow_up.previous_answer.trim();
        let start = previous
            .char_indices()
            .rev()
            .nth(11_999)
            .map_or(0, |(index, _)| index);
        messages.push(Message {
            role: "assistant",
            content: previous[start..].into(),
        });
        messages.push(Message {
            role: "user",
            content: format!(
                "{}\n\n{}",
                if language.is_chinese() {
                    "用同一份依据回答追问；缺少依据时说明缺什么。"
                } else {
                    "Answer the follow-up using the same evidence; state any missing evidence."
                },
                follow_up.question.trim()
            ),
        });
    }
    bounded(messages)
}

/// One repair request; the caller owns the one-attempt policy.
pub fn build_correction(
    original: &[Message],
    draft: &str,
    issues: &[String],
    language: Language,
) -> Result<Vec<Message>, Error> {
    let mut messages = original.to_vec();
    messages.push(Message {
        role: "assistant",
        content: draft.into(),
    });
    let prefix = if language.is_chinese() {
        "只改正以下问题，保留有依据的讲解，返回完整更正稿："
    } else {
        "Correct only the following issues, preserve supported explanation, and return the complete corrected answer:"
    };
    messages.push(Message {
        role: "user",
        content: format!(
            "{prefix}\n{}",
            issues
                .iter()
                .take(8)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        ),
    });
    bounded(messages)
}
