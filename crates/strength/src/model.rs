//! Synchronous inference for the Java application's bundled XGBoost20Tun model.

use serde::Deserialize;
use thiserror::Error;

pub const SIGMA: f64 = 0.92;
pub const FEATURE_INDICES: [usize; 20] = [
    13, 5, 2, 19, 10, 20, 26, 3, 14, 11, 28, 8, 18, 7, 12, 23, 27, 22, 25, 0,
];
pub const FULL_FEATURE_NAMES: [&str; 29] = [
    "first_choice_rate",
    "top3_rate",
    "top5_rate",
    "average_ai_rank_fit",
    "excellent_rate",
    "good_move_rate",
    "non_inaccuracy_rate",
    "match_rate",
    "non_mistake_rate",
    "non_blunder_rate",
    "weighted_loss_fit",
    "average_loss_fit",
    "median_loss_fit",
    "p75_loss_fit",
    "p90_loss_fit",
    "p95_loss_fit",
    "max_loss_fit",
    "loss_stability_fit",
    "difficulty_fit",
    "opening_loss_fit",
    "middlegame_loss_fit",
    "endgame_loss_fit",
    "opening_good_move_rate",
    "middlegame_good_move_rate",
    "endgame_good_move_rate",
    "first_choice_x_difficulty",
    "good_move_x_difficulty",
    "match_x_difficulty",
    "top5_x_difficulty",
];

const BOOSTER: &str = include_str!("../models/xgboost20tun_booster.json");
const CALIBRATOR: &str = include_str!("../models/xgboost20tun_residual_calibrator.json");

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("invalid strength model JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid strength model: {0}")]
    Invalid(&'static str),
}

#[derive(Debug)]
pub struct XGBoostStrengthModel {
    base_score: f64,
    feature_count: usize,
    trees: Vec<Tree>,
    calibrator: Option<ResidualCalibrator>,
}

impl XGBoostStrengthModel {
    pub fn bundled() -> Result<Self, ModelError> {
        let model = Self::from_json(BOOSTER, Some(CALIBRATOR))?;
        if model.feature_count != FEATURE_INDICES.len() {
            return Err(ModelError::Invalid("bundled feature count"));
        }
        Ok(model)
    }

    pub fn from_json(booster: &str, calibrator: Option<&str>) -> Result<Self, ModelError> {
        let root: Booster = serde_json::from_str(booster)?;
        let params = root.learner.learner_model_param;
        let base = params.base_score.trim();
        let base = base
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .unwrap_or(base);
        let base_score: f64 = base
            .trim()
            .parse()
            .map_err(|_| ModelError::Invalid("base score"))?;
        let feature_count: usize = params
            .num_feature
            .parse()
            .map_err(|_| ModelError::Invalid("feature count"))?;
        if !base_score.is_finite() || feature_count == 0 {
            return Err(ModelError::Invalid("base score or feature count"));
        }
        let trees = root.learner.gradient_booster.model.trees;
        for tree in &trees {
            tree.validate(feature_count)?;
        }
        let calibrator = calibrator
            .map(serde_json::from_str::<CalibratorFile>)
            .transpose()?
            .map(ResidualCalibrator::from);
        Ok(Self {
            base_score,
            feature_count,
            trees,
            calibrator,
        })
    }

    /// The raw tree sum. Absent entries and NaN use each node's default branch.
    pub fn predict(&self, features: &[f64]) -> f64 {
        self.trees
            .iter()
            .fold(self.base_score, |sum, tree| sum + tree.predict(features))
    }

    /// Java's full-29 feature selection, rank clamp, and residual calibration.
    pub fn predict_rank(&self, full_features: &[f64; 29]) -> f64 {
        let selected = FEATURE_INDICES.map(|index| full_features[index]);
        let raw = self.predict(&selected).clamp(-1.0, 12.0);
        self.calibrate(raw, full_features)
    }

    pub fn calibrate(&self, prediction: f64, full_features: &[f64; 29]) -> f64 {
        match &self.calibrator {
            Some(calibrator) => calibrator.apply(prediction, full_features),
            None => prediction.clamp(-1.0, 12.0),
        }
    }
}

#[derive(Deserialize)]
struct Booster {
    learner: Learner,
}

#[derive(Deserialize)]
struct Learner {
    learner_model_param: LearnerParams,
    gradient_booster: GradientBooster,
}

#[derive(Deserialize)]
struct LearnerParams {
    base_score: String,
    num_feature: String,
}

#[derive(Deserialize)]
struct GradientBooster {
    model: Trees,
}

#[derive(Deserialize)]
struct Trees {
    trees: Vec<Tree>,
}

#[derive(Debug, Deserialize)]
struct Tree {
    left_children: Vec<i32>,
    right_children: Vec<i32>,
    split_indices: Vec<usize>,
    split_conditions: Vec<f64>,
    default_left: Vec<u8>,
}

impl Tree {
    fn validate(&self, feature_count: usize) -> Result<(), ModelError> {
        let count = self.left_children.len();
        if count == 0
            || self.right_children.len() != count
            || self.split_indices.len() != count
            || self.split_conditions.len() != count
            || self.default_left.len() != count
        {
            return Err(ModelError::Invalid("tree array lengths"));
        }
        for node in 0..count {
            let left = self.left_children[node];
            let right = self.right_children[node];
            if left == -1 && right == -1 {
                continue;
            }
            if left < 0
                || right < 0
                || left as usize >= count
                || right as usize >= count
                || self.split_indices[node] >= feature_count
            {
                return Err(ModelError::Invalid("tree child or feature index"));
            }
        }
        // Validate the reachable graph without recursion, so malformed input cannot loop.
        let mut states = vec![0; count];
        let mut stack = vec![(0, false)];
        while let Some((node, leaving)) = stack.pop() {
            if leaving {
                states[node] = 2;
            } else if states[node] == 1 {
                return Err(ModelError::Invalid("cyclic tree"));
            } else if states[node] == 0 {
                states[node] = 1;
                stack.push((node, true));
                if self.left_children[node] != -1 {
                    stack.push((self.right_children[node] as usize, false));
                    stack.push((self.left_children[node] as usize, false));
                }
            }
        }
        Ok(())
    }

    fn predict(&self, features: &[f64]) -> f64 {
        let mut node = 0;
        while self.left_children[node] != -1 {
            let value = features
                .get(self.split_indices[node])
                .copied()
                .unwrap_or(f64::NAN);
            let left = if value.is_nan() {
                self.default_left[node] != 0
            } else {
                value < self.split_conditions[node]
            };
            node = if left {
                self.left_children[node] as usize
            } else {
                self.right_children[node] as usize
            };
        }
        self.split_conditions[node]
    }
}

#[derive(Deserialize)]
struct CalibratorFile {
    final_calibrator: ResidualCalibrator,
    #[serde(default)]
    quality_gate_thresholds: QualityGates,
}

#[derive(Debug, Deserialize)]
struct ResidualCalibrator {
    spec: CalibrationSpec,
    feature_order: Vec<String>,
    scaler_mean: Vec<f64>,
    scaler_scale: Vec<f64>,
    intercept: f64,
    coefficients: Vec<f64>,
    #[serde(skip)]
    gates: QualityGates,
}

#[derive(Debug, Deserialize)]
struct CalibrationSpec {
    gate_start: f64,
    gate_full: f64,
    #[serde(default = "default_correction_min")]
    correction_min: f64,
    #[serde(default = "default_correction_max")]
    correction_max: f64,
}

fn default_correction_min() -> f64 {
    -0.5
}

fn default_correction_max() -> f64 {
    1.0
}

#[derive(Debug, Default, Deserialize)]
struct QualityGates {
    match_rate: Option<Vec<f64>>,
    top5_rate: Option<Vec<f64>>,
    weighted_loss_fit: Option<Vec<f64>>,
}

impl From<CalibratorFile> for ResidualCalibrator {
    fn from(file: CalibratorFile) -> Self {
        let mut calibrator = file.final_calibrator;
        calibrator.gates = file.quality_gate_thresholds;
        calibrator
    }
}

impl ResidualCalibrator {
    fn apply(&self, prediction: f64, features: &[f64; 29]) -> f64 {
        let count = self.coefficients.len();
        if self.feature_order.len() != count
            || self.scaler_mean.len() != count
            || self.scaler_scale.len() != count
        {
            return prediction.clamp(-1.0, 12.0);
        }
        let mut correction = self.intercept;
        for index in 0..count {
            let value = feature_value(&self.feature_order[index], prediction, features);
            if !value.is_finite() {
                return prediction.clamp(-1.0, 12.0);
            }
            let scale = if self.scaler_scale[index] == 0.0 {
                1.0
            } else {
                self.scaler_scale[index]
            };
            let standardized = (value - self.scaler_mean[index]) / scale;
            correction += self.coefficients[index] * standardized;
        }
        let rank_gate = ((prediction - self.spec.gate_start)
            / (self.spec.gate_full - self.spec.gate_start).max(1e-9))
        .clamp(0.0, 1.0);
        let quality_gate = gate(features[7], &self.gates.match_rate, &[0.52, 0.60])
            .max(gate(features[2], &self.gates.top5_rate, &[0.68, 0.80]))
            .max(gate(
                features[10],
                &self.gates.weighted_loss_fit,
                &[0.32, 0.50],
            ));
        let correction = correction
            .min(self.spec.correction_max)
            .max(self.spec.correction_min);
        (prediction + rank_gate * quality_gate * correction).clamp(-1.0, 12.0)
    }
}

fn feature_value(name: &str, prediction: f64, features: &[f64; 29]) -> f64 {
    match name {
        "base_prediction" => prediction,
        "first_choice_rate" => features[0],
        "top5_rate" => features[2],
        "match_rate" => features[7],
        "weighted_loss_fit" => features[10],
        "difficulty_fit" => features[18],
        _ => name
            .strip_prefix("hinge_above_")
            .and_then(|threshold| threshold.parse::<f64>().ok())
            .filter(|threshold| threshold.is_finite())
            .map_or(f64::NAN, |threshold| (prediction - threshold).max(0.0)),
    }
}

fn gate(value: f64, threshold: &Option<Vec<f64>>, fallback: &[f64; 2]) -> f64 {
    let threshold = threshold.as_deref().unwrap_or(fallback);
    if threshold.len() < 2 {
        return 0.0;
    }
    ((value - threshold[0]) / (threshold[1] - threshold[0]).max(1e-9)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests;
