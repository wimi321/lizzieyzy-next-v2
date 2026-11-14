use super::GameError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum GtpTime {
    PerMove(u32),
    Byoyomi {
        main: u32,
        period: u32,
        stones: u32,
    },
    Fischer {
        main: u32,
        increment: u32,
    },
    Absolute {
        main: u32,
    },
    #[default]
    EngineDefault,
    Custom(Vec<String>),
}
impl GtpTime {
    pub fn validate(&self) -> Result<(), GameError> {
        let integer = |value: u32| value <= i32::MAX as u32;
        let valid = match self {
            Self::PerMove(seconds) => *seconds > 0 && integer(*seconds),
            Self::Byoyomi {
                main,
                period,
                stones,
            } => {
                *period > 0 && *stones > 0 && integer(*main) && integer(*period) && integer(*stones)
            }
            Self::Fischer { main, increment } => {
                (*main > 0 || *increment > 0) && integer(*main) && integer(*increment)
            }
            Self::Absolute { main } => *main > 0 && integer(*main),
            Self::EngineDefault => true,
            Self::Custom(lines) => {
                lines.len() <= 256
                    && lines.iter().all(|line| {
                        !line.trim().is_empty()
                            && line.len() <= 8192
                            && !line.contains(['\r', '\n', '\0'])
                    })
            }
        };
        if valid {
            Ok(())
        } else {
            Err(GameError::InvalidSetup("invalid GTP time control".into()))
        }
    }
    pub(crate) fn commands(&self, kata: bool) -> Result<Vec<String>, GameError> {
        self.validate()?;
        Ok(match self {
            Self::PerMove(seconds) => vec![format!("time_settings 0 {seconds} 1")],
            Self::Byoyomi {
                main,
                period,
                stones,
            } => vec![format!("time_settings {main} {period} {stones}")],
            Self::Fischer { main, increment } if kata => {
                vec![format!("kata-time_settings fischer {main} {increment}")]
            }
            Self::Absolute { main } if kata => vec![format!("kata-time_settings absolute {main}")],
            Self::Fischer { .. } | Self::Absolute { .. } => {
                return Err(GameError::GtpUnsupportedTime);
            }
            Self::EngineDefault => Vec::new(),
            Self::Custom(lines) => lines.clone(),
        })
    }
}

/// Each side owns its clock even when both sides share one process.
#[derive(Clone)]
pub(crate) struct GtpClock {
    pub mode: GtpTime,
    main: f64,
    overtime: f64,
    stones: u32,
}
impl GtpClock {
    pub fn new(mode: GtpTime) -> Self {
        let (main, overtime, stones) = match mode {
            GtpTime::Byoyomi {
                main,
                period,
                stones,
            } => (main as f64, period as f64, stones),
            GtpTime::Fischer { main, .. } | GtpTime::Absolute { main } => (main as f64, 0.0, 0),
            _ => (0.0, 0.0, 0),
        };
        Self {
            mode,
            main,
            overtime,
            stones,
        }
    }
    pub fn time_left(&self, color: go_core::Color) -> Option<String> {
        let color = if color == go_core::Color::Black {
            "black"
        } else {
            "white"
        };
        let (seconds, stones) = match self.mode {
            GtpTime::Byoyomi { .. } if self.main == 0.0 => (self.overtime, self.stones),
            GtpTime::Byoyomi { .. } | GtpTime::Fischer { .. } | GtpTime::Absolute { .. } => {
                (self.main, 0)
            }
            _ => return None,
        };
        Some(format!(
            "time_left {color} {} {stones}",
            seconds.floor().clamp(0.0, i32::MAX as f64) as u32
        ))
    }
    pub fn response_timeout(&self, configured: std::time::Duration) -> std::time::Duration {
        let seconds = match self.mode {
            GtpTime::PerMove(seconds) => seconds as f64,
            GtpTime::Byoyomi { .. } => self.main + self.overtime,
            GtpTime::Fischer { .. } | GtpTime::Absolute { .. } => self.main,
            GtpTime::EngineDefault | GtpTime::Custom(_) => return configured,
        };
        configured.max(std::time::Duration::from_secs_f64(seconds + 5.0))
    }
    pub fn spend(&mut self, seconds: f64, completed: bool) {
        let main_spent = self.main.min(seconds);
        self.main -= main_spent;
        match self.mode {
            GtpTime::Fischer { increment, .. } if completed => {
                self.main = (self.main + increment as f64).min(i32::MAX as f64)
            }
            GtpTime::Byoyomi { period, stones, .. } if self.main == 0.0 => {
                self.overtime = (self.overtime - (seconds - main_spent)).max(0.0);
                if completed {
                    self.stones = self.stones.saturating_sub(1);
                    if self.stones == 0 {
                        self.stones = stones;
                        self.overtime = period as f64;
                    }
                }
            }
            _ => {}
        }
    }
}
