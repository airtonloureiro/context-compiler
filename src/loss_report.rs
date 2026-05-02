use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs;
use std::io;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileLoss {
    pub path: String,
    pub mode: String,
    pub tokens_saved: u64,
    pub elided_symbols: Vec<String>,
    pub skeletonized_symbols: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LossSummary {
    pub original_tokens: u64,
    pub final_tokens: u64,
    pub reduction_ratio: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LossReport {
    pub schema_version: String,
    pub summary: LossSummary,
    pub files: Vec<FileLoss>,
}

impl LossReport {
    pub fn write_atomic(repo_root: &Path, report: &LossReport) -> io::Result<()> {
        let path = repo_root.join(".ctxc").join("loss-report.json");
        let json = serde_json::to_string_pretty(report)?;
        let dir = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No parent dir"))?;
        fs::create_dir_all(dir)?;
        
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}
