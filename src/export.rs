use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;

use crate::config::ExportFormat;
use crate::error::{AppError, Result};
use crate::generator::FoundAddress;

/// Write the generated addresses to a timestamped file inside `out_dir`.
pub fn export(
    records: &[FoundAddress],
    format: ExportFormat,
    out_dir: &Path,
) -> Result<PathBuf> {
    let path = out_dir.join(default_file_name(format));
    let content = render(records, format)?;

    fs::write(&path, content).map_err(|source| AppError::Export {
        path: path.clone(),
        source,
    })?;

    Ok(path)
}

/// Render the generated addresses in the requested format.
pub fn render(records: &[FoundAddress], format: ExportFormat) -> Result<String> {
    match format {
        ExportFormat::Csv => Ok(to_csv(records)),
        ExportFormat::Txt => Ok(to_txt(records)),
        ExportFormat::Json => Ok(serde_json::to_string_pretty(records)?),
    }
}

/// Timestamped file name used for both disk exports and browser downloads.
pub fn default_file_name(format: ExportFormat) -> String {
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    format!("tron_vanity_{}.{}", timestamp, format.extension())
}

/// CSV representation including a header row.
fn to_csv(records: &[FoundAddress]) -> String {
    let mut out = String::from("index,address,private_key,found_at_attempts\n");
    for record in records {
        out.push_str(&format!(
            "{},{},{},{}\n",
            record.index, record.address, record.private_key, record.found_at_attempts
        ));
    }
    out
}

/// Plain `address,private_key` lines, convenient for import into wallets.
fn to_txt(records: &[FoundAddress]) -> String {
    let mut out = String::new();
    for record in records {
        out.push_str(&format!("{},{}\n", record.address, record.private_key));
    }
    out
}
