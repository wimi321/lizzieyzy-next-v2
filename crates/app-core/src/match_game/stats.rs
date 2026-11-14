use super::*;
fn elo(p: f64) -> Option<f64> {
    (p > 0.0 && p < 1.0).then(|| -400.0 * (1.0 / p - 1.0).log10())
}
impl EloEstimate {
    pub fn from_results(wins: u32, losses: u32, draws: u32) -> Option<Self> {
        let n = f64::from(wins) + f64::from(losses) + f64::from(draws);
        if n == 0.0 {
            return None;
        }
        let p = (f64::from(wins) + 0.5 * f64::from(draws)) / n;
        let estimate = elo(p)?;
        let sigma = (p * (1.0 - p) / n).sqrt();
        Some(Self {
            elo: estimate,
            lower: elo(p - 2.0 * sigma),
            upper: elo(p + 2.0 * sigma),
        })
    }
}
fn record(
    a: &mut MatchWins,
    b: &mut MatchWins,
    draws: &mut u32,
    unscored: &mut BTreeMap<String, u32>,
    a_black: bool,
    result: Option<&str>,
    reason: &str,
) {
    let winner = match result {
        Some("0") | Some("Draw") | Some("Jigo") => {
            *draws += 1;
            return;
        }
        Some(result) if result.starts_with("B+") => Some(true),
        Some(result) if result.starts_with("W+") => Some(false),
        _ => None,
    };
    if let Some(black) = winner {
        let wins = if black == a_black { a } else { b };
        wins.total += 1;
        if black {
            wins.black += 1
        } else {
            wins.white += 1
        }
    } else {
        *unscored.entry(reason.to_owned()).or_default() += 1;
    }
}
impl MatchView {
    pub(crate) fn record(&mut self, result: Option<&str>, reason: &str) {
        self.completed += 1;
        record(
            &mut self.a,
            &mut self.b,
            &mut self.draws,
            &mut self.unscored,
            self.a_is_black,
            result,
            reason,
        );
        if let Some(opening) = &self.current_opening {
            let index = self
                .openings
                .iter()
                .position(|item| item.opening == *opening)
                .unwrap_or_else(|| {
                    self.openings.push(OpeningStanding {
                        opening: opening.clone(),
                        ..Default::default()
                    });
                    self.openings.len() - 1
                });
            let item = &mut self.openings[index];
            item.completed += 1;
            record(
                &mut item.a,
                &mut item.b,
                &mut item.draws,
                &mut item.unscored,
                self.a_is_black,
                result,
                reason,
            );
        }
        self.elo = EloEstimate::from_results(self.a.total, self.b.total, self.draws);
    }
}
pub(crate) fn file_component(text: &str) -> String {
    let value: String = text
        .chars()
        .take(60)
        .map(|c| {
            if c.is_control() || "/\\<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let value = value.trim_matches([' ', '.']);
    if value.is_empty() {
        "Unknown".into()
    } else {
        value.into()
    }
}
pub(crate) fn match_file_name(
    number: u32,
    black: &str,
    white: &str,
    result: Option<&str>,
    reason: &str,
) -> String {
    format!(
        "{number}_{}_vs_{}_{}.sgf",
        file_component(black),
        file_component(white),
        file_component(result.unwrap_or(reason))
    )
}
