//! Exact-query checkpoints and idempotent CSV publication.

use super::{
    config::{DownloadConfig, validate_component},
    windows::DownloadWindow,
};
use crate::market::{error::MarketError, storage::csv::rebuild_from_parts};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, error::Error, fs, path::PathBuf};

/// Every option that determines the records and destination of a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetIdentity {
    /// Record kind, either quotes or trades.
    pub kind: String,
    /// Requested symbol.
    pub symbol: String,
    /// Feed name.
    pub feed: String,
    /// Exact requested start.
    pub start: String,
    /// Exact requested end.
    pub end: String,
    /// Optional quote currency.
    pub currency: Option<String>,
    /// Optional quote mapping date.
    pub asof: Option<String>,
    /// Optional absolute Parquet output root; None means CSV.
    pub parquet_output_dir: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
struct State {
    version: u32,
    identity: DatasetIdentity,
    completed_days: BTreeSet<String>,
}

/// A single-writer download checkpoint with retained CSV window artifacts.
pub struct Checkpoint {
    state: State,
    path: PathBuf,
    parts: PathBuf,
    output: PathBuf,
    lock: Option<PathBuf>,
}

impl Checkpoint {
    /// Open a checkpoint, invalidating completion when the exact query changes.
    /// A corrupt checkpoint is reported instead of silently restarting an append.
    pub fn open(
        config: &DownloadConfig,
        mut identity: DatasetIdentity,
    ) -> Result<Self, Box<dyn Error>> {
        validate_component(&identity.symbol)?;
        validate_component(&identity.kind)?;
        config.validate()?;
        fs::create_dir_all(&config.output_dir)?;
        if let Some(root) = &identity.parquet_output_dir {
            fs::create_dir_all(root)?;
            identity.parquet_output_dir = Some(fs::canonicalize(root)?);
        }
        let state_dir = config
            .output_dir
            .join(format!(".historical_{}_state", identity.kind));
        fs::create_dir_all(&state_dir)?;
        let stem = format!("{}_{}", identity.symbol, identity.feed);
        let path = state_dir.join(format!("{stem}_windows.json"));
        let parts = state_dir.join(format!("{stem}_parts"));
        fs::create_dir_all(&parts)?;
        let output = config.output_dir.join(format!(
            "{}_historical_{}_{}.csv",
            identity.symbol, identity.kind, identity.feed
        ));
        let lock = state_dir.join(format!("{stem}.lock"));
        let mut checkpoint = Self {
            state: State {
                version: 1,
                identity,
                completed_days: BTreeSet::new(),
            },
            path,
            parts,
            output,
            lock: None,
        };
        // Holding a lock prevents concurrent runs from replacing the same parts.
        fs::OpenOptions::new().write(true).create_new(true).open(&lock)
            .map_err(|error| MarketError::InvalidConfig(format!("cannot acquire download lock {}: {error}; remove a stale lock only after confirming its writer stopped", lock.display())))?;
        checkpoint.lock = Some(lock);
        if checkpoint.path.exists() {
            let stored: State = serde_json::from_slice(&fs::read(&checkpoint.path)?)?;
            if stored.version != 1 {
                return Err(
                    MarketError::InvalidData("unsupported checkpoint version".into()).into(),
                );
            }
            if stored.identity == checkpoint.state.identity {
                checkpoint.state = stored;
            }
        }
        checkpoint.save()?;
        if checkpoint.state.identity.parquet_output_dir.is_none()
            && !checkpoint.state.completed_days.is_empty()
        {
            checkpoint.rebuild_csv()?;
        }
        Ok(checkpoint)
    }

    /// Temporary path used while paging one window.
    pub fn temporary_part(&self, window: &DownloadWindow) -> PathBuf {
        self.parts.join(format!("{}.csv.tmp", window.day))
    }

    /// Whether the requested window has a checkpoint and its published artifact.
    pub fn is_complete(&self, window: &DownloadWindow) -> bool {
        if !self.state.completed_days.contains(&window.day.to_string()) {
            return false;
        }
        match &self.state.identity.parquet_output_dir {
            Some(root) => root
                .join(format!("symbol={}", self.state.identity.symbol))
                .join(format!("date={}", window.day))
                .join(format!("{}.parquet", self.state.identity.kind))
                .is_file(),
            None => self.parts.join(format!("{}.csv", window.day)).is_file(),
        }
    }

    /// Publish an already completed CSV part and persist completion before rebuilding output.
    pub fn complete_csv(&mut self, window: &DownloadWindow) -> Result<(), Box<dyn Error>> {
        fs::rename(
            self.temporary_part(window),
            self.parts.join(format!("{}.csv", window.day)),
        )?;
        self.state.completed_days.insert(window.day.to_string());
        self.save()?;
        self.rebuild_csv()
    }

    /// Record completion after the Parquet writer published the window.
    pub fn complete_parquet(&mut self, window: &DownloadWindow) -> Result<(), Box<dyn Error>> {
        self.state.completed_days.insert(window.day.to_string());
        self.save()?;
        fs::remove_file(self.temporary_part(window))?;
        Ok(())
    }

    fn save(&self) -> Result<(), Box<dyn Error>> {
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&self.state)?)?;
        fs::rename(temporary, &self.path)?;
        Ok(())
    }

    fn rebuild_csv(&self) -> Result<(), Box<dyn Error>> {
        let parts: Vec<_> = self
            .state
            .completed_days
            .iter()
            .map(|day| self.parts.join(format!("{day}.csv")))
            .collect();
        rebuild_from_parts(&parts, &self.output)
    }
}

impl Drop for Checkpoint {
    fn drop(&mut self) {
        if let Some(lock) = &self.lock {
            if let Err(error) = fs::remove_file(lock) {
                log::warn!("could not remove download lock {}: {error}", lock.display());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> (DownloadConfig, DatasetIdentity, DownloadWindow) {
        let root = std::env::temp_dir().join(format!(
            "download-checkpoint-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let config = DownloadConfig {
            output_dir: root,
            parquet_output_dir: None,
            feed: "sip".into(),
            limit: 100,
        };
        let identity = DatasetIdentity {
            kind: "quotes".into(),
            symbol: "SPY".into(),
            feed: "sip".into(),
            start: "2024-01-01T09:00:00Z".into(),
            end: "2024-01-01T10:00:00Z".into(),
            currency: Some("USD".into()),
            asof: None,
            parquet_output_dir: None,
        };
        let window = DownloadWindow {
            start: identity.start.clone(),
            end: identity.end.clone(),
            day: "2024-01-01".parse().unwrap(),
        };
        (config, identity, window)
    }

    #[test]
    fn resume_repairs_output_without_duplicate_rows() {
        let (config, identity, window) = fixture();
        {
            let mut checkpoint = Checkpoint::open(&config, identity.clone()).unwrap();
            fs::write(
                checkpoint.temporary_part(&window),
                "timestamp,price\n1,10\n",
            )
            .unwrap();
            checkpoint.complete_csv(&window).unwrap();
            fs::write(&checkpoint.output, "interrupted publication").unwrap();
        }
        {
            let checkpoint = Checkpoint::open(&config, identity).unwrap();
            assert!(checkpoint.is_complete(&window));
            assert_eq!(
                fs::read_to_string(&checkpoint.output).unwrap(),
                "timestamp,price\n1,10\n"
            );
        }
        fs::remove_dir_all(config.output_dir).unwrap();
    }

    #[test]
    fn changed_query_options_do_not_reuse_completed_day() {
        let (config, identity, window) = fixture();
        for changed in ["start", "currency", "asof", "output"] {
            {
                let mut checkpoint = Checkpoint::open(&config, identity.clone()).unwrap();
                fs::write(
                    checkpoint.temporary_part(&window),
                    "timestamp,price\n1,10\n",
                )
                .unwrap();
                checkpoint.complete_csv(&window).unwrap();
            }
            let mut other = identity.clone();
            match changed {
                "start" => other.start = "2024-01-01T08:00:00Z".into(),
                "currency" => other.currency = Some("EUR".into()),
                "asof" => other.asof = Some("2024-01-02".into()),
                "output" => other.parquet_output_dir = Some(config.output_dir.join("parquet")),
                _ => unreachable!(),
            }
            let checkpoint = Checkpoint::open(&config, other).unwrap();
            assert!(!checkpoint.is_complete(&window), "{changed}");
        }
        fs::remove_dir_all(config.output_dir).unwrap();
    }
}
