use std::{collections::BTreeMap, future::pending, time::Duration};
use tokio::{
    io::{AsyncWriteExt, Stdout},
    time::{Instant, sleep_until},
};

pub struct Search {
    next: Instant,
    interval: Duration,
    reports: Vec<String>,
    index: usize,
    step: u64,
}
impl Search {
    pub fn new(options: &BTreeMap<String, String>, args: &[&str]) -> Result<Self, String> {
        let interval = args
            .get(1)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(10.0)
            / 100.0;
        if !interval.is_finite() || !(0.001..=86400.0).contains(&interval) {
            return Err("invalid interval".into());
        }
        let reports = options
            .get("--analysis-script")
            .map(|path| {
                std::fs::read_to_string(path)
                    .map(|text| {
                        text.split("\n\n")
                            .filter(|v| !v.trim().is_empty())
                            .map(|v| v.trim().to_owned())
                            .collect()
                    })
                    .map_err(|e| e.to_string())
            })
            .transpose()?
            .unwrap_or_default();
        let step = options
            .get("--analysis-visits-step")
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);
        Ok(Self {
            next: Instant::now(),
            interval: Duration::from_secs_f64(interval),
            reports,
            index: 0,
            step,
        })
    }
    pub async fn emit(&mut self, out: &mut Stdout, size: (u8, u8)) -> Result<(), String> {
        self.index += 1;
        let report = if self.reports.is_empty() {
            let visits = (self.index as u64).saturating_mul(self.step);
            let ownership = vec!["0.25"; usize::from(size.0) * usize::from(size.1)].join(" ");
            format!(
                "info move A1 visits {} winrate 0.7 scoreLead 2.5 prior 0.4 order 0 pv A1 pass rootInfo visits {visits} winrate 0.65 scoreLead 2 utility 0.3 ownership {ownership}",
                visits / 2
            )
        } else {
            self.reports[self.index.saturating_sub(1).min(self.reports.len() - 1)].clone()
        };
        out.write_all(format!("{report}\n").as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        out.flush().await.map_err(|e| e.to_string())?;
        self.next = Instant::now() + self.interval;
        Ok(())
    }
}
pub async fn tick(search: &Option<Search>) {
    match search {
        Some(search) => sleep_until(search.next).await,
        None => pending().await,
    }
}
