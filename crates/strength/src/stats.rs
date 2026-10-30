use crate::{MoveCategory, Sample, XGBoostStrengthModel};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StrengthReport {
    pub black: SideReport,
    pub white: SideReport,
    pub overall: SideReport,
    pub has_enough_data: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PhaseReport {
    pub sample_count: usize,
    pub average_winrate_loss: f64,
    pub average_score_loss: Option<f64>,
    pub weighted_score_loss: f64,
    pub good_move_rate: f64,
    pub first_choice_rate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SideReport {
    pub sample_count: usize,
    pub score_sample_count: usize,
    /// Null only when no analyzed moves exist; short reports retain a low confidence estimate.
    pub rank_value: Option<f64>,
    pub rank_label_zh: String,
    pub rank_label_en: String,
    pub prediction_sigma: Option<f64>,
    pub quality_score: f64,
    pub average_winrate_loss: f64,
    pub average_score_loss: Option<f64>,
    pub median_score_loss: Option<f64>,
    pub weighted_score_loss: f64,
    pub average_score_equivalent_loss: f64,
    pub match_rate: f64,
    pub first_choice_rate: f64,
    pub top3_rate: f64,
    pub top5_rate: f64,
    pub good_move_rate: f64,
    pub move_rank_good_move_rate: f64,
    pub move_rank_counts: [u32; 6],
    /// Exclusive display tiers: only strict first choices occupy Best.
    pub tier_counts: [u32; 6],
    pub bad_move_rate: f64,
    pub mistake_rate: f64,
    pub blunder_rate: f64,
    pub average_difficulty: f64,
    pub strength_band: String,
    pub confidence: Confidence,
    pub ai_likelihood: f64,
    pub features: [f64; 29],
    pub opening: PhaseReport,
    pub middlegame: PhaseReport,
    pub endgame: PhaseReport,
    pub samples: Vec<Sample>,
}

pub(crate) fn summarize(samples: &[Sample], model: &XGBoostStrengthModel) -> SideReport {
    let count = samples.len();
    let denominator = count.max(1) as f64;
    let rate =
        |f: fn(&Sample) -> bool| samples.iter().filter(|s| f(s)).count() as f64 / denominator;
    let scores: Vec<_> = samples.iter().filter_map(|s| s.score_loss).collect();
    let losses: Vec<_> = samples.iter().map(|s| s.score_equivalent_loss).collect();
    let avg_loss = losses.iter().sum::<f64>() / denominator;
    let weighted = weighted_loss(samples, avg_loss);
    let first = rate(|s| s.first_choice);
    let top3 = rate(|s| s.ai_rank < 3);
    let top5 = rate(|s| s.ai_rank < 5);
    let good = rate(|s| s.category.is_good());
    let mistakes = rate(|s| s.category.is_mistake());
    let blunders = rate(|s| s.category == MoveCategory::Blunder);
    let match_rate = (0.45 * first + 0.45 * good + 0.10 * (1.0 - mistakes)).clamp(0.0, 1.0);
    let difficulty = samples.iter().map(|s| s.complexity).sum::<f64>() * 100.0 / denominator;
    let median = if scores.is_empty() {
        avg_loss
    } else {
        percentile(&scores, 0.5)
    };
    let phase = |low, high| {
        let part: Vec<_> = samples
            .iter()
            .filter(|s| s.move_number >= low && s.move_number <= high)
            .cloned()
            .collect();
        phase_report(&part, weighted, good)
    };
    let opening = phase(1, 60);
    let middlegame = phase(61, 160);
    let endgame = phase(161, u32::MAX);
    let average_ai_rank = samples
        .iter()
        .map(|s| {
            if s.ai_rank >= 999 {
                10.0
            } else {
                (s.ai_rank as f64 + 1.0).min(10.0)
            }
        })
        .sum::<f64>()
        / denominator;
    let difficulty_fit = ((difficulty - 25.0) / 35.0).clamp(0.0, 1.0);
    let variance = losses.iter().map(|v| (v - avg_loss).powi(2)).sum::<f64>() / denominator;
    let p90 = percentile(&losses, 0.90);
    let features = if count == 0 {
        [0.0; 29]
    } else {
        [
            first,
            top3,
            top5,
            1.0 / (1.0 + average_ai_rank.clamp(0.0, 10.0)),
            rate(|s| s.category == MoveCategory::Excellent),
            good,
            1.0 - rate(|s| s.category == MoveCategory::Inaccuracy),
            match_rate,
            1.0 - mistakes,
            1.0 - blunders,
            loss_fit(weighted, 50.0),
            loss_fit(avg_loss, 50.0),
            loss_fit(median, 50.0),
            loss_fit(percentile(&losses, 0.75), 50.0),
            loss_fit(p90, 80.0),
            loss_fit(percentile(&losses, 0.95), 100.0),
            loss_fit(losses.iter().copied().fold(0.0, f64::max), 120.0),
            loss_fit(variance.sqrt(), 50.0),
            difficulty_fit,
            loss_fit(opening.weighted_score_loss, 50.0),
            loss_fit(middlegame.weighted_score_loss, 50.0),
            loss_fit(endgame.weighted_score_loss, 50.0),
            opening.good_move_rate,
            middlegame.good_move_rate,
            endgame.good_move_rate,
            first * difficulty_fit,
            good * difficulty_fit,
            match_rate * difficulty_fit,
            top5 * difficulty_fit,
        ]
    };
    let rank = (count > 0).then(|| model.predict_rank(&features));
    let mut counts = [0; 6];
    let mut tiers = [0; 6];
    for s in samples {
        counts[s.move_rank.min(5) as usize] += 1;
        let tier = if s.first_choice {
            0
        } else {
            s.move_rank.clamp(1, 5)
        };
        tiers[tier as usize] += 1;
    }
    let confidence = if count >= 40 && scores.len() as f64 >= count as f64 * 0.7 {
        Confidence::High
    } else if count >= 16 {
        Confidence::Medium
    } else {
        Confidence::Low
    };
    let quality = rank.map_or(0.0, |v| ((v + 18.0) * 100.0 / 30.0).clamp(0.0, 100.0));
    let band = if count == 0 {
        "-".into()
    } else {
        crate::rank::strength_band(
            count,
            rank.unwrap_or(0.0),
            weighted,
            avg_loss,
            median,
            p90,
            first,
            good,
            mistakes,
            match_rate,
            difficulty,
        )
        .into()
    };
    let likelihood = if count == 0 {
        0.0
    } else {
        (0.35 * ((match_rate - 0.72) / 0.22).clamp(0.0, 1.0)
            + 0.30 * ((first - 0.50) / 0.18).clamp(0.0, 1.0)
            + 0.25 * ((3.2 - weighted) / 3.2).clamp(0.0, 1.0)
            + 0.10 * (1.0 - (mistakes / 0.05).clamp(0.0, 1.0)))
        .clamp(0.0, 1.0)
    };
    SideReport {
        sample_count: count,
        score_sample_count: scores.len(),
        rank_value: rank,
        rank_label_zh: crate::format_rank(rank.unwrap_or(f64::NAN), crate::RankLocale::ZhCn),
        rank_label_en: crate::format_rank(rank.unwrap_or(f64::NAN), crate::RankLocale::En),
        prediction_sigma: rank.map(|_| crate::model::SIGMA),
        quality_score: quality,
        average_winrate_loss: samples.iter().map(|s| s.winrate_loss).sum::<f64>() / denominator,
        average_score_loss: (!scores.is_empty())
            .then(|| scores.iter().sum::<f64>() / scores.len() as f64),
        median_score_loss: (!scores.is_empty()).then(|| percentile(&scores, 0.5)),
        weighted_score_loss: weighted,
        average_score_equivalent_loss: avg_loss,
        match_rate: if count == 0 { 0.0 } else { match_rate },
        first_choice_rate: first,
        top3_rate: top3,
        top5_rate: top5,
        good_move_rate: good,
        move_rank_good_move_rate: rate(|s| s.move_rank <= 1),
        move_rank_counts: counts,
        tier_counts: tiers,
        bad_move_rate: rate(|s| !s.category.is_good()),
        mistake_rate: mistakes,
        blunder_rate: blunders,
        average_difficulty: difficulty,
        strength_band: band,
        confidence,
        ai_likelihood: likelihood,
        features,
        opening,
        middlegame,
        endgame,
        samples: samples.to_vec(),
    }
}

fn loss_fit(loss: f64, cap: f64) -> f64 {
    1.0 / (1.0 + loss.clamp(0.0, cap))
}
fn weighted_loss(samples: &[Sample], fallback: f64) -> f64 {
    let sum: f64 = samples.iter().map(|s| s.adjusted_weight).sum();
    if sum == 0.0 {
        fallback
    } else {
        samples
            .iter()
            .map(|s| s.score_equivalent_loss * s.adjusted_weight)
            .sum::<f64>()
            / sum
    }
}
fn percentile(values: &[f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let position = fraction * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let weight = position - lower as f64;
    sorted[lower] * (1.0 - weight) + sorted[upper] * weight
}
fn phase_report(samples: &[Sample], fallback_loss: f64, fallback_good: f64) -> PhaseReport {
    let count = samples.len();
    let scores: Vec<_> = samples.iter().filter_map(|s| s.score_loss).collect();
    PhaseReport {
        sample_count: count,
        average_winrate_loss: samples.iter().map(|s| s.winrate_loss).sum::<f64>()
            / count.max(1) as f64,
        average_score_loss: (!scores.is_empty())
            .then(|| scores.iter().sum::<f64>() / scores.len() as f64),
        weighted_score_loss: weighted_loss(samples, fallback_loss),
        good_move_rate: if count == 0 {
            fallback_good
        } else {
            samples.iter().filter(|s| s.category.is_good()).count() as f64 / count as f64
        },
        first_choice_rate: samples.iter().filter(|s| s.first_choice).count() as f64
            / count.max(1) as f64,
    }
}
