/// The supported application locales. Formatting preserves Java's continuous rank values.
#[derive(Debug, Clone, Copy)]
pub enum RankLocale {
    ZhCn,
    En,
}

pub fn format_rank(rank: f64, locale: RankLocale) -> String {
    if !rank.is_finite() {
        return "-".into();
    }
    let value = rank.clamp(-18.0, 12.0);
    let zh = matches!(locale, RankLocale::ZhCn);
    // Java Formatter uses half-up for decimal ties; Rust formatting alone uses ties-even.
    let numeric = (value * 10.0).round() / 10.0;
    if value >= 12.0 {
        format!(
            "{numeric:.1} {}",
            if zh { "半神/AI" } else { "Semi-god/AI" }
        )
    } else if value >= 11.0 {
        format!(
            "{numeric:.1} {}",
            if zh {
                "一线职业"
            } else {
                "Top professional"
            }
        )
    } else if value >= 10.0 {
        format!("{numeric:.1} {}", if zh { "职业" } else { "Professional" })
    } else if value >= 1.0 {
        if zh {
            format!("业余{numeric:.1}段")
        } else {
            format!("{numeric:.1} dan")
        }
    } else {
        let kyu = (((2.0 - value).max(1.0)) * 10.0).round() / 10.0;
        if zh {
            format!("业余{kyu:.1}级")
        } else {
            format!("{kyu:.1} kyu")
        }
    }
}

const TIERS: [(f64, f64, f64); 5] = [
    (-18.0, 1.0, 0.22),
    (1.0, 10.0, 0.40),
    (10.0, 11.0, 0.126),
    (11.0, 12.0, 0.127),
    (12.0, 13.0, 0.127),
];
/// Normalized, monotonic Java StrengthScale ruler position, for future UI rendering.
pub fn scale_position(value: f64) -> f64 {
    let clamped = value.clamp(-18.0, 13.0);
    let mut position = 0.0;
    for (index, &(low, high, width)) in TIERS.iter().enumerate() {
        if clamped < high || index == TIERS.len() - 1 {
            return position + width * ((clamped - low) / (high - low)).clamp(0.0, 1.0);
        }
        position += width;
    }
    position
}
pub fn scale_tier(value: f64) -> u8 {
    TIERS
        .iter()
        .rposition(|&(low, _, _)| value >= low)
        .unwrap_or(0) as u8
}

const BANDS: [&str; 14] = [
    "Beginner",
    "11-15k",
    "6-10k",
    "3-5k",
    "1-2k",
    "1-2d",
    "3-4d",
    "5-6d",
    "7d",
    "8d",
    "9d",
    "10d职业",
    "11d一线职业",
    "12d AI",
];
const FIRST: [(f64, usize); 11] = [
    (0.62, 13),
    (0.50, 12),
    (0.46, 11),
    (0.42, 10),
    (0.38, 9),
    (0.34, 8),
    (0.30, 7),
    (0.24, 7),
    (0.18, 6),
    (0.12, 5),
    (0.06, 4),
];
const GOOD: [(f64, usize); 12] = [
    (0.96, 13),
    (0.88, 12),
    (0.82, 11),
    (0.78, 10),
    (0.74, 9),
    (0.68, 8),
    (0.62, 7),
    (0.56, 6),
    (0.50, 5),
    (0.42, 4),
    (0.32, 3),
    (0.22, 2),
];
const MISTAKES: [(f64, usize); 12] = [
    (0.005, 13),
    (0.050, 12),
    (0.060, 11),
    (0.070, 10),
    (0.100, 9),
    (0.140, 8),
    (0.180, 7),
    (0.240, 6),
    (0.310, 5),
    (0.400, 4),
    (0.520, 3),
    (0.660, 2),
];
const MEDIAN: [(f64, usize); 12] = [
    (0.05, 13),
    (0.25, 12),
    (0.50, 11),
    (0.80, 10),
    (1.10, 9),
    (1.50, 8),
    (2.00, 7),
    (2.60, 6),
    (3.40, 5),
    (4.60, 4),
    (6.50, 3),
    (9.00, 2),
];
const MATCH: [(f64, usize); 11] = [
    (0.80, 13),
    (0.70, 12),
    (0.62, 11),
    (0.58, 10),
    (0.54, 9),
    (0.49, 8),
    (0.43, 7),
    (0.37, 6),
    (0.30, 5),
    (0.22, 4),
    (0.14, 3),
];

fn at_least(value: f64, thresholds: &[(f64, usize)], fallback: usize) -> usize {
    thresholds
        .iter()
        .find(|&&(v, _)| value >= v)
        .map_or(fallback, |&(_, level)| level)
}
fn at_most(value: f64, thresholds: &[(f64, usize)]) -> usize {
    thresholds
        .iter()
        .find(|&&(v, _)| value <= v)
        .map_or(1, |&(_, level)| level)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn strength_band(
    count: usize,
    rank: f64,
    weighted: f64,
    average: f64,
    median: f64,
    p90: f64,
    first: f64,
    good: f64,
    mistake: f64,
    matched: f64,
    difficulty: f64,
) -> &'static str {
    let base = at_least(
        rank,
        &[
            (11.5, 13),
            (10.5, 12),
            (9.5, 11),
            (8.5, 10),
            (7.5, 9),
            (6.5, 8),
            (4.5, 7),
            (2.5, 6),
            (0.0, 5),
            (-2.75, 4),
            (-6.0, 3),
            (-10.5, 2),
            (-15.5, 1),
        ],
        0,
    );
    let elite = if first >= 0.95 && good >= 0.96 && mistake <= 0.01 && weighted <= 1.0 {
        13
    } else if first >= 0.50 && good >= 0.86 && mistake <= 0.05 && weighted <= 3.2 {
        12
    } else if first >= 0.45 && good >= 0.86 && mistake <= 0.06 && weighted <= 3.0 {
        11
    } else if first >= 0.44 && good >= 0.78 && matched >= 0.62 && mistake <= 0.03 && weighted <= 2.0
    {
        10
    } else {
        0
    };
    let mut level = base
        .max(elite)
        .min(at_least(first, &FIRST, 6))
        .min(at_least(good, &GOOD, 1))
        .min(at_least(matched, &MATCH, 2))
        .min(at_most(mistake, &MISTAKES))
        .min(at_most(median, &MEDIAN));
    if (good < 0.50 && first < 0.22)
        || (good < 0.62 && first < 0.30)
        || (weighted > 7.50 && first < 0.26)
    {
        level = level.min(4);
    }
    if weighted >= 2.90 && first < 0.34 && good < 0.78 {
        level = level.min(6);
    }
    if p90 > 8.0 && average > 2.8 && matched < 0.55 {
        level = level.min(7);
    }
    if p90 > 5.5 && median > 1.0 && matched < 0.50 {
        level = level.min(6);
    }
    if weighted > 4.8 && average > 3.0 && matched < 0.45 {
        level = level.min(4);
    }
    if level >= 9 && difficulty < 25.0 && first < 0.35 && good < 0.74 {
        level = level.min(7);
    }
    if count < 6 && level == 0 && weighted <= 10.0 {
        level = 1;
    }
    BANDS[level]
}
