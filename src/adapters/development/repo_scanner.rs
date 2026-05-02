//! Scanner determinístico (B-002 + hardening B-003) com saída para file-map (B-004).
//!
//! Política interna por categorias, sem contrato externo:
//! - .gitignore aplicado pelo walker;
//! - diretórios internos (build/cache/IDE/Python/JVM) podados sem descer;
//! - arquivos sensíveis (env/keys/dumps/dbs/credenciais) classificados sem
//!   listar path no stdout;
//! - binários e mídias classificados por extensão / nome-ruído;
//! - arquivos grandes acima de 1 MiB classificados por metadata, sem ler
//!   conteúdo.
//!
//! O ponto de entrada run_scan agora também gera e persiste atomicamente
//! .ctxc/file-map.json (B-004) ao terminar o traversal. Stdout permanece o
//! resumo agregado de B-002/B-003.
//!
//! B-008: Agora também gera .ctxc/symbol-map.json para arquivos Rust, TypeScript e Python.

use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::fs;

use ignore::WalkBuilder;

use crate::adapters::development::file_map;
use crate::adapters::development::symbol_map::{self, SymbolMap, FileSymbols};

const LARGE_FILE_LIMIT_BYTES: u64 = 1024 * 1024; // 1 MiB

const INTERNAL_DIR_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    ".next",
    "target",
    "coverage",
    "vendor",
    "bower_components",
    ".turbo",
    ".parcel-cache",
    ".cache",
    "out",
    "tmp",
    "temp",
    ".tmp",
    ".temp",
    ".fastembed_cache",
    ".venv",
    "venv",
    "env",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".tox",
    ".gradle",
    ".idea",
    ".classpath",
    ".settings",
];

const BINARY_NOISE_FILENAMES: &[&str] = &[".DS_Store", "Thumbs.db"];

const BINARY_EXTENSIONS: &[&str] = &[
    "exe", "dll", "so", "dylib", "a", "o", "obj", "class", "jar", "war",
    "png", "jpg", "jpeg", "gif", "webp", "ico", "pdf", "mp4", "mov", "mp3",
    "wav", "flac", "woff", "woff2", "ttf", "otf", "eot",
    "zip", "tar", "tgz", "gz", "bz2", "xz", "7z", "rar",
];

const SENSITIVE_DUMP_OR_DB_EXTENSIONS: &[&str] = &[
    "sql", "dump", "bak", "backup", "sqlite", "sqlite3", "db",
];

const SENSITIVE_CRED_EXTENSIONS: &[&str] = &[
    "pem", "key", "pfx", "p12", "pkcs12", "crt", "cer",
];

const SENSITIVE_EXACT_FILENAMES: &[&str] = &[
    ".env",
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".dockercfg",
    ".htpasswd",
    "credentials",
    "secrets",
];

const SENSITIVE_NAME_TOKENS: &[&str] = &["token", "apikey", "api_key", "private_key"];

const SENSITIVE_NAME_TOKEN_EXTENSIONS: &[&str] = &[
    "json", "yaml", "yml", "toml", "env", "conf", "cfg", "ini", "properties",
    "xml", "csv", "tsv", "txt",
];

#[derive(Debug)]
pub enum ScanError {
    NotFound(PathBuf),
    NotDirectory(PathBuf),
    Unreadable(PathBuf, String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::NotFound(p) => {
                write!(f, "ctxc scan: --repo não existe: {}", p.display())
            }
            ScanError::NotDirectory(p) => write!(
                f,
                "ctxc scan: --repo precisa apontar para um diretório local (não-symlink): {}",
                p.display()
            ),
            ScanError::Unreadable(p, msg) => write!(
                f,
                "ctxc scan: --repo não pôde ser lido: {}: {}",
                p.display(),
                msg
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScanReport {
    pub repo: PathBuf,
    pub files_considered: u64,
    pub files_ignored: u64,
    pub dirs_ignored: u64,
    pub sensitive_ignored: u64,
    pub binary_ignored: u64,
    pub large_ignored: u64,
    pub total_bytes_considered: u64,
    pub total_tokens_estimate: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileClass {
    Considered,
    Sensitive,
    Binary,
    Large,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawKind {
    File,
    Symlink,
}

#[derive(Debug, Clone)]
pub struct RawEntry {
    pub rel_path: String,
    pub kind: RawKind,
    pub class: FileClass,
    pub size: Option<u64>,
    pub depth: u32,
    pub is_hidden: bool,
}

pub fn run_scan(repo_arg: &Path) -> i32 {
    let (report, raw_entries) = match scan_with_entries(repo_arg) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            return 64;
        }
    };

    let dest = file_map::default_output_path(&report.repo);
    let ctxc_dir = match dest.parent() {
        Some(d) => d.to_path_buf(),
        None => {
            eprintln!("ctxc scan: destino do file-map sem diretório pai válido");
            return 64;
        }
    };

    if let Err(e) = std::fs::create_dir_all(&ctxc_dir) {
        eprintln!("ctxc scan: falha ao preparar .ctxc/: {e}");
        return 64;
    }

    let salt = match file_map::load_or_create_salt(&ctxc_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ctxc scan: falha ao carregar/criar .ctxc/.salt: {e}");
            return 64;
        }
    };

    let map = file_map::build(repo_arg, &report, &raw_entries, &salt);
    if let Err(e) = file_map::write_atomic(&map, &dest) {
        eprintln!("ctxc scan: falha ao escrever file-map: {e}");
        return 64;
    }

    use rayon::prelude::*;
    let file_symbols: Vec<_> = raw_entries.par_iter().filter_map(|entry| {
        if matches!(entry.class, FileClass::Considered) && matches!(entry.kind, RawKind::File) {
            let path = report.repo.join(&entry.rel_path);
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if matches!(ext, "rs" | "py" | "ts" | "tsx") {
                if let Ok(content) = fs::read_to_string(&path) {
                    Some(symbol_map::extract_symbols(Path::new(&entry.rel_path), &content))
                } else {
                    Some(FileSymbols {
                        path: entry.rel_path.clone(),
                        symbols: Vec::new(),
                        parsing_error: Some("io_error".to_string()),
                    })
                }
            } else {
                None
            }
        } else {
            None
        }
    }).collect();

    let symbol_map = SymbolMap {
        schema_version: "1.0.0".to_string(),
        files: file_symbols,
    };

    let symbol_map_path = ctxc_dir.join("symbol-map.json");
    if let Ok(json) = serde_json::to_vec_pretty(&symbol_map) {
        let _ = write_atomic_bytes(&json, &symbol_map_path);
    }

    let knowledge_graph = crate::adapters::development::graph_builder::GraphBuilder::build(&symbol_map);
    let kg_path = ctxc_dir.join("knowledge-graph.json");
    if let Ok(json) = serde_json::to_vec_pretty(&knowledge_graph) {
        let _ = write_atomic_bytes(&json, &kg_path);
    }

    print!("{}", render(&report));
    0
}

fn write_atomic_bytes(bytes: &[u8], dest: &Path) -> io::Result<()> {
    let dir = dest.parent().unwrap();
    let tmp = dir.join(format!(".tmp.{}", std::process::id()));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, dest)?;
    Ok(())
}

pub fn scan(repo: &Path) -> Result<ScanReport, ScanError> {
    scan_with_entries(repo).map(|(r, _)| r)
}

pub fn scan_with_entries(repo: &Path) -> Result<(ScanReport, Vec<RawEntry>), ScanError> {
    let meta = std::fs::symlink_metadata(repo).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            ScanError::NotFound(repo.to_path_buf())
        } else {
            ScanError::Unreadable(repo.to_path_buf(), e.to_string())
        }
    })?;

    if meta.file_type().is_symlink() {
        return Err(ScanError::NotDirectory(repo.to_path_buf()));
    }
    if !meta.is_dir() {
        return Err(ScanError::NotDirectory(repo.to_path_buf()));
    }

    let canonical = std::fs::canonicalize(repo)
        .map_err(|e| ScanError::Unreadable(repo.to_path_buf(), e.to_string()))?;

    let dirs_ignored = Arc::new(AtomicU64::new(0));
    let dirs_ignored_filter = Arc::clone(&dirs_ignored);

    let walker = WalkBuilder::new(&canonical)
        .follow_links(false)
        .git_global(false)
        .git_exclude(false)
        .git_ignore(true)
        .ignore(false)
        .hidden(false)
        .parents(false)
        .require_git(false)
        .sort_by_file_path(|a, b| a.cmp(b))
        .filter_entry(move |entry| {
            if entry.depth() == 0 {
                return true;
            }
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if !is_dir {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            if name == ".ctxc" {
                return false;
            }
            if INTERNAL_DIR_NAMES.contains(&&*name) {
                dirs_ignored_filter.fetch_add(1, Ordering::Relaxed);
                return false;
            }
            true
        })
        .build();

    let mut files_considered: u64 = 0;
    let mut sensitive_ignored: u64 = 0;
    let mut binary_ignored: u64 = 0;
    let mut large_ignored: u64 = 0;
    let mut other_ignored: u64 = 0;
    let mut raw_entries: Vec<RawEntry> = Vec::new();

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(_) => {
                other_ignored += 1;
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }
        let ft = match entry.file_type() {
            Some(t) => t,
            None => continue,
        };

        let path = entry.path();
        let rel_path = match path.strip_prefix(&canonical) {
            Ok(rel) => to_posix_relative(rel),
            Err(_) => continue,
        };
        if rel_path.is_empty() {
            continue;
        }
        let depth = entry.depth() as u32;
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_hidden = name.starts_with('.');

        if ft.is_symlink() {
            other_ignored += 1;
            raw_entries.push(RawEntry {
                rel_path,
                kind: RawKind::Symlink,
                class: FileClass::Other,
                size: None,
                depth,
                is_hidden,
            });
            continue;
        }
        if ft.is_dir() {
            continue;
        }
        if !ft.is_file() {
            other_ignored += 1;
            continue;
        }

        let size = std::fs::symlink_metadata(path).ok().map(|m| m.len());
        let class = classify_with_size(&name, size);

        match class {
            FileClass::Considered => files_considered += 1,
            FileClass::Sensitive => sensitive_ignored += 1,
            FileClass::Binary => binary_ignored += 1,
            FileClass::Large => large_ignored += 1,
            FileClass::Other => other_ignored += 1,
        }

        raw_entries.push(RawEntry {
            rel_path,
            kind: RawKind::File,
            class,
            size,
            depth,
            is_hidden,
        });
    }

    let files_ignored =
        sensitive_ignored + binary_ignored + large_ignored + other_ignored;

    let (total_bytes_considered, total_tokens_estimate) =
        file_map::aggregate_estimates(&raw_entries);

    Ok((
        ScanReport {
            repo: canonical,
            files_considered,
            files_ignored,
            dirs_ignored: dirs_ignored.load(Ordering::Relaxed),
            sensitive_ignored,
            binary_ignored,
            large_ignored,
            total_bytes_considered,
            total_tokens_estimate,
        },
        raw_entries,
    ))
}

pub fn render(report: &ScanReport) -> String {
    format!(
"ctxc scan
repo: {}
files_considered: {}
files_ignored: {}
dirs_ignored: {}
sensitive_ignored: {}
binary_ignored: {}
large_ignored: {}
bytes_considered: {}
estimated_tokens: {}
",
        report.repo.display(),
        report.files_considered,
        report.files_ignored,
        report.dirs_ignored,
        report.sensitive_ignored,
        report.binary_ignored,
        report.large_ignored,
        report.total_bytes_considered,
        report.total_tokens_estimate,
    )
}

fn classify_with_size(name: &str, size: Option<u64>) -> FileClass {
    if is_sensitive_filename(name) {
        return FileClass::Sensitive;
    }
    if is_binary_noise_name(name) {
        return FileClass::Binary;
    }
    if is_binary_extension(name) {
        return FileClass::Binary;
    }
    match size {
        Some(s) if s > LARGE_FILE_LIMIT_BYTES => FileClass::Large,
        Some(_) => FileClass::Considered,
        None => FileClass::Other,
    }
}

pub fn is_sensitive_filename(name: &str) -> bool {
    if SENSITIVE_EXACT_FILENAMES.contains(&name) {
        return true;
    }
    if name.starts_with(".env.") {
        return true;
    }
    if name.starts_with("credentials.") || name.starts_with("credential.") {
        return true;
    }
    if name.starts_with("secrets.") || name.starts_with("secret.") {
        return true;
    }
    if let Some(ext) = file_extension(name) {
        if SENSITIVE_CRED_EXTENSIONS.contains(&ext.as_str()) {
            return true;
        }
        if SENSITIVE_DUMP_OR_DB_EXTENSIONS.contains(&ext.as_str()) {
            return true;
        }
        if matches_combined_token_rule(name, &ext) {
            return true;
        }
    }
    false
}

fn is_binary_noise_name(name: &str) -> bool {
    BINARY_NOISE_FILENAMES.contains(&name)
}

fn is_binary_extension(name: &str) -> bool {
    match file_extension(name) {
        Some(ext) => BINARY_EXTENSIONS.contains(&ext.as_str()),
        None => false,
    }
}

fn matches_combined_token_rule(name: &str, ext: &str) -> bool {
    if !SENSITIVE_NAME_TOKEN_EXTENSIONS.contains(&ext) {
        return false;
    }
    let stem = name.strip_suffix(&format!(".{ext}")).unwrap_or(name);
    let stem_lower = stem.to_ascii_lowercase();
    SENSITIVE_NAME_TOKENS
        .iter()
        .any(|tok| stem_lower.contains(tok))
}

pub fn file_extension(name: &str) -> Option<String> {
    let dot = name.rfind('.')?;
    if dot == 0 {
        return None;
    }
    let ext = &name[dot + 1..];
    if ext.is_empty() {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

fn to_posix_relative(p: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in p.components() {
        if let Component::Normal(s) = c {
            parts.push(s.to_string_lossy().into_owned());
        }
    }
    parts.join("/")
}
