//! Shared Parquet writer; the final path appears only after the footer is closed.

use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TemporaryFile(PathBuf);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            log::warn!(
                "Unable to remove temporary dataset {}: {error}",
                self.0.display()
            );
        }
    }
}

/// Saves a batch with the historical bars' Zstandard compression setting.
///
/// # Errors
/// Returns filesystem or Parquet errors; an existing target is preserved on failure.
pub fn save_parquet(batch: &RecordBatch, path: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    write_batch(
        batch,
        path.as_ref(),
        Compression::ZSTD(ZstdLevel::try_new(3)?),
    )
}

pub(crate) fn write_batch(
    batch: &RecordBatch,
    path: &Path,
    compression: Compression,
) -> Result<(), Box<dyn Error>> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let filename = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "dataset path requires a filename",
        )
    })?;
    let (temporary, file) = loop {
        let mut name = filename.to_os_string();
        name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let candidate = parent.join(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => break (TemporaryFile(candidate), file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    let properties = WriterProperties::builder()
        .set_compression(compression)
        .build();
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(properties))?;
    writer.write(batch)?;
    writer.close()?;
    File::open(&temporary.0)?.sync_all()?;
    fs::rename(&temporary.0, path)?;
    Ok(())
}
