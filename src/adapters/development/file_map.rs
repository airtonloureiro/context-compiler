use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::adapters::development::repo_scanner::{file_extension, FileClass, RawEntry, RawKind, ScanReport};

pub const FILE_MAP_SCHEMA_VERSION: &str = "1.1.0";
const SCAN_PROFILE_NAME: &str = "default";
const SCAN_PROFILE_VERSION: &str = "1.0.0";
const FILE_MAP_DIR: &str = ".ctxc";
const FILE_MAP_FILENAME: &str = "file-map.json";
const SALT_FILENAME: &str = ".salt";
const SALT_LEN: usize = 32;
pub const BYTES_PER_TOKEN: u64 = 3;
const TOKEN_ESTIMATE_METHOD: &str = "bytes_per_token";
const TOKEN_ESTIMATE_ROUNDING: &str = "ceil";
const TOKEN_ESTIMATE_SCOPE: &str = "considered_files_only";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct FileMap {
    pub schema_version: String,
    pub repo: Repo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<Policy>,
    pub summary: Summary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_estimate: Option<TokenEstimate>,
    pub entries: Vec<Entry>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub scan_profile: Option<String>,
    pub version: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub considered: u64,
    pub ignored: u64,
    pub dirs_ignored: u64,
    pub sensitive_ignored: u64,
    pub binary_ignored: u64,
    pub large_ignored: u64,
    pub total_bytes_considered: u64,
    pub total_tokens_estimate: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TokenEstimate {
    pub method: String,
    pub bytes_per_token: u64,
    pub rounding: String,
    pub scope: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind { File, Symlink }

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus { Considered, Ignored }

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IgnoreCategory { Gitignore, InternalDir, Sensitive, Binary, Large, Permission, Symlink, Other }

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub kind: EntryKind,
    pub status: EntryStatus,
    pub is_hidden: bool,
    pub is_sensitive_candidate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_redacted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_category: Option<IgnoreCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
}

#[derive(Debug)]
pub enum FileMapError { Io(io::Error), Serialize(serde_json::Error) }
impl std::fmt::Display for FileMapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileMapError::Io(e) => write!(f, "io: {e}"),
            FileMapError::Serialize(e) => write!(f, "serialize: {e}"),
        }
    }
}
impl std::error::Error for FileMapError {}
impl From<io::Error> for FileMapError { fn from(e: io::Error) -> Self { FileMapError::Io(e) } }
impl From<serde_json::Error> for FileMapError { fn from(e: serde_json::Error) -> Self { FileMapError::Serialize(e) } }

pub fn build(repo_arg: &Path, report: &ScanReport, raw_entries: &[RawEntry], salt: &[u8; SALT_LEN]) -> FileMap {
    let entries: Vec<Entry> = raw_entries.iter().map(|raw| entry_from_raw(raw, salt)).collect();
    FileMap {
        schema_version: FILE_MAP_SCHEMA_VERSION.to_string(),
        repo: Repo { root: repo_arg.to_string_lossy().to_string(), canonical_path: Some(report.repo.to_string_lossy().to_string()) },
        generated_at: None, policy: Some(Policy { scan_profile: Some(SCAN_PROFILE_NAME.to_string()), version: Some(SCAN_PROFILE_VERSION.to_string()) }),
        entries, summary: Summary {
            considered: report.files_considered, ignored: report.files_ignored, dirs_ignored: report.dirs_ignored, sensitive_ignored: report.sensitive_ignored,
            binary_ignored: report.binary_ignored, large_ignored: report.large_ignored, total_bytes_considered: report.total_bytes_considered, total_tokens_estimate: report.total_tokens_estimate,
        },
        token_estimate: Some(TokenEstimate { method: TOKEN_ESTIMATE_METHOD.to_string(), bytes_per_token: BYTES_PER_TOKEN, rounding: TOKEN_ESTIMATE_ROUNDING.to_string(), scope: TOKEN_ESTIMATE_SCOPE.to_string() }),
    }
}

pub fn load(path: &Path) -> Result<FileMap, FileMapError> {
    let bytes = fs::read(path)?;
    let map: FileMap = serde_json::from_slice(&bytes)?;
    if map.schema_version != FILE_MAP_SCHEMA_VERSION {
        return Err(FileMapError::Io(io::Error::new(io::ErrorKind::InvalidData, format!("schema_version={}, expected {}", map.schema_version, FILE_MAP_SCHEMA_VERSION))));
    }
    Ok(map)
}

fn entry_from_raw(raw: &RawEntry, salt: &[u8; SALT_LEN]) -> Entry {
    let kind = match raw.kind { RawKind::File => EntryKind::File, RawKind::Symlink => EntryKind::Symlink };
    let (status, ignore_category) = status_and_category(raw);
    let is_sensitive = matches!(ignore_category, Some(IgnoreCategory::Sensitive));
    if is_sensitive {
        Entry { kind, status, is_hidden: raw.is_hidden, is_sensitive_candidate: true, path: None, path_redacted: Some(redact_path(&raw.rel_path, salt)), ignore_category, size_bytes: raw.size, extension: None, depth: Some(raw.depth), parent_dir: None, reason_code: None }
    } else {
        Entry { kind, status, is_hidden: raw.is_hidden, is_sensitive_candidate: false, path: Some(raw.rel_path.clone()), path_redacted: None, ignore_category, size_bytes: match raw.kind { RawKind::File => raw.size, RawKind::Symlink => None }, extension: file_extension(&basename_of(&raw.rel_path)), depth: Some(raw.depth), parent_dir: parent_dir_of(&raw.rel_path), reason_code: None }
    }
}

fn status_and_category(raw: &RawEntry) -> (EntryStatus, Option<IgnoreCategory>) {
    match raw.kind {
        RawKind::Symlink => (EntryStatus::Ignored, Some(IgnoreCategory::Symlink)),
        RawKind::File => match raw.class {
            FileClass::Considered => (EntryStatus::Considered, None),
            FileClass::Sensitive => (EntryStatus::Ignored, Some(IgnoreCategory::Sensitive)),
            FileClass::Binary => (EntryStatus::Ignored, Some(IgnoreCategory::Binary)),
            FileClass::Large => (EntryStatus::Ignored, Some(IgnoreCategory::Large)),
            FileClass::Other => (EntryStatus::Ignored, Some(IgnoreCategory::Other)),
        }
    }
}

fn redact_path(path: &str, salt: &[u8; SALT_LEN]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt); hasher.update(path.as_bytes());
    hex_lower(&hasher.finalize()[..8])
}

fn hex_lower(bytes: &[u8]) -> String { bytes.iter().map(|b| format!("{:02x}", b)).collect() }
fn basename_of(path: &str) -> String { Path::new(path).file_name().unwrap_or_default().to_string_lossy().to_string() }
fn parent_dir_of(path: &str) -> Option<String> { Path::new(path).parent().filter(|p| p.as_os_str() != "").map(|p| p.to_string_lossy().to_string()) }
pub fn default_output_path(repo_root: &Path) -> PathBuf { repo_root.join(FILE_MAP_DIR).join(FILE_MAP_FILENAME) }
pub fn salt_path(repo_root: &Path) -> PathBuf { repo_root.join(FILE_MAP_DIR).join(SALT_FILENAME) }
pub fn load_or_create_salt(repo_root: &Path) -> io::Result<[u8; SALT_LEN]> {
    let p = salt_path(repo_root);
    if p.exists() {
        let b = fs::read(&p)?;
        if b.len() != SALT_LEN { return Err(io::Error::new(io::ErrorKind::InvalidData, "salt corrompido")); }
        let mut salt = [0u8; SALT_LEN]; salt.copy_from_slice(&b); Ok(salt)
    } else {
        let mut salt = [0u8; SALT_LEN];
        getrandom::getrandom(&mut salt).map_err(|e| io::Error::other(e.to_string()))?;
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Auto-injetar `.ctxc/` no .gitignore se não existir (Mitigação Cassandra B-004)
        let gitignore_path = repo_root.join(".gitignore");
        let ctxc_ignore_entry = ".ctxc/\n";
        
        if gitignore_path.exists() {
            let content = fs::read_to_string(&gitignore_path)?;
            if !content.contains(".ctxc") {
                use std::io::Write;
                let mut file = fs::OpenOptions::new().append(true).open(&gitignore_path)?;
                file.write_all(b"\n# Context Compiler\n.ctxc/\n")?;
            }
        } else {
            fs::write(&gitignore_path, ctxc_ignore_entry)?;
        }

        fs::write(&p, salt)?; Ok(salt)
    }
}

pub fn write_atomic(map: &FileMap, dest: &Path) -> Result<(), FileMapError> {
    let json = serde_json::to_string_pretty(map)?;
    let tmp = dest.with_extension("tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, dest)?;
    Ok(())
}

pub fn aggregate_estimates(entries: &[RawEntry]) -> (u64, u64) {
    let mut bytes: u64 = 0;
    for e in entries {
        if matches!(e.kind, RawKind::File) && matches!(e.class, FileClass::Considered) {
            if let Some(s) = e.size { bytes = bytes.saturating_add(s); }
        }
    }
    let tokens = bytes.div_ceil(BYTES_PER_TOKEN);
    (bytes, tokens)
}
