//! `ctxc compile` — geração determinística de `.ctxc/compiled-context.md`
//! a partir de `.ctxc/file-map.json` v1.1.0 (B-006, ADR-006 ACEITAR-COM-MUDANCAS).
//!
//! Esta é a **primeira** leitura de conteúdo de arquivo no produto. Todas as
//! 4 mitigações duras de Cassandra (Gate 3 addendum) estão materializadas:
//!
//! - **M1** double-check defensivo anti-sensitive antes de abrir cada arquivo
//!   (status, ignore_category, is_sensitive_candidate). Falha em `considered`
//!   → bug crítico do scanner → abort fatal.
//! - **M2** warning de delta em stderr para entries stale (mismatch entre
//!   `entry.size_bytes` e `metadata.len()` atual). Não aborta.
//! - **M3** warning de volume em stderr quando `total_tokens_estimate >
//!   WARN_TOKEN_THRESHOLD`. Não aborta, não trunca.
//! - **M4** schema_version comparado por string-equality EXATA contra
//!   `file_map::FILE_MAP_SCHEMA_VERSION`.
//!
//! Path traversal protegido por checagem lexical (`..`, absolute) + canonical
//! prefix check antes de abrir qualquer arquivo. Atomicidade via tempfile +
//! rename + cleanup do tmp em erro.

use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::adapters::development::file_map::{
    self, Entry, EntryKind, EntryStatus, FileMap, IgnoreCategory, FILE_MAP_SCHEMA_VERSION,
};
use crate::loss_report::{FileLoss, LossReport, LossSummary};
use crate::planner::Planner;
use crate::ranker::SelectionMode;
use crate::adapters::development::symbol_map::SymbolMap;
use crate::skim::{SkimClient, SkimInput, SkimClassification, SkimOutput};

/// M3: warning de volume — não aborta nem trunca, apenas avisa.
pub const WARN_TOKEN_THRESHOLD: u64 = 50_000;

/// Comprimento máximo de `--task` para não poluir o header.
pub const MAX_TASK_LEN: usize = 256;

const COMPILED_FILENAME: &str = "compiled-context.md";

/// Resultado interno do render de uma entrada.
enum BlockBody {
    /// Conteúdo UTF-8 emitido com fence de N backticks.
    Fenced { fence: usize, lang: &'static str, body: String },
    /// `[non-UTF-8 content elided, N bytes]`
    NonUtf8 { bytes: usize },
    /// `[content elided: contains markdown fence collision]`
    FenceCollision,
    /// `[content elided: security lock / stale large file]`
    Elided { reason: String },
}

#[derive(Debug)]
pub enum CompileError {
    Io(io::Error),
    FileMap(file_map::FileMapError),
    InvalidTask(String),
    SchemaMismatch { found: String, expected: String },
    SensitiveLeak { path: String },
    PathTraversal { path: String },
    EntryMissing { path: String },
    OutputConflict(String),
    Skim(String),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::Io(e) => write!(f, "io: {e}"),
            CompileError::FileMap(e) => write!(f, "{e}"),
            CompileError::InvalidTask(msg) => write!(f, "invalid --task: {msg}"),
            CompileError::SchemaMismatch { found, expected } => write!(
                f,
                "file-map schema_version={found}, expected exactly {expected}; regenerate with ctxc scan"
            ),
            CompileError::SensitiveLeak { path } => write!(
                f,
                "scanner bug: entry '{path}' marked considered but classified sensitive; refusing to read content"
            ),
            CompileError::PathTraversal { path } => write!(
                f,
                "path traversal: entry '{path}' escapes repo canonical_path; refusing to read"
            ),
            CompileError::EntryMissing { path } => write!(
                f,
                "file-map entry '{path}' missing on disk; regenerate with ctxc scan"
            ),
            CompileError::OutputConflict(msg) => write!(f, "{msg}"),
            CompileError::Skim(msg) => write!(f, "skim error: {msg}"),
        }
    }
}

impl From<io::Error> for CompileError {
    fn from(e: io::Error) -> Self {
        CompileError::Io(e)
    }
}

impl From<file_map::FileMapError> for CompileError {
    fn from(e: file_map::FileMapError) -> Self {
        CompileError::FileMap(e)
    }
}

/// Entry point chamado pela CLI.
pub fn run_compile(
    task: &str,
    log: Option<&str>,
    repo: &Path,
    output: Option<&Path>,
    skim: bool,
    skim_model: &str,
) -> i32 {
    match try_compile(task, log, repo, output, skim, skim_model) {
        Ok((output_path, files_in_compile, bytes, tokens, stale_entries, skeletonized_files)) => {
            let stale_suffix = if stale_entries > 0 {
                format!(" (stale={stale_entries})")
            } else {
                String::new()
            };
            let skeleton_signal = if skeletonized_files > 0 {
                format!(" ({} [S])", skeletonized_files)
            } else {
                String::new()
            };
            println!(
                "compiled {} files{} ({}B, ~{}tok) -> {}{}",
                files_in_compile,
                skeleton_signal,
                bytes,
                tokens,
                output_path.display(),
                stale_suffix
            );
            0
        }
        Err(e) => {
            eprintln!("ctxc compile: {e}");
            64
        }
    }
}

fn try_compile(
    task: &str,
    log: Option<&str>,
    repo: &Path,
    output: Option<&Path>,
    skim_enabled: bool,
    skim_model: &str,
) -> Result<(PathBuf, u64, u64, u64, u64, u64), CompileError> {
    validate_task(task)?;

    let skim_client = if skim_enabled {
        Some(SkimClient::new(skim_model))
    } else {
        None
    };

    let repo_meta = fs::symlink_metadata(repo).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            CompileError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("--repo not found: {}", repo.display()),
            ))
        } else {
            CompileError::Io(e)
        }
    })?;
    if !repo_meta.is_dir() {
        return Err(CompileError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("--repo must be a directory: {}", repo.display()),
        )));
    }

    let repo_canonical = fs::canonicalize(repo)?;
    let file_map_path = repo_canonical.join(".ctxc").join("file-map.json");
    if !file_map_path.exists() {
        return Err(CompileError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                ".ctxc/file-map.json not found, run 'ctxc scan' first ({})",
                file_map_path.display()
            ),
        )));
    }

    // M4: schema strict via file_map::load (string-exact 1.1.0).
    let map: FileMap = file_map::load(&file_map_path).map_err(|e| match e {
        file_map::FileMapError::Io(io_err)
            if io_err.kind() == io::ErrorKind::InvalidData
                && io_err.to_string().contains("schema_version=") =>
        {
            // Re-parse bytes to extract `schema_version` for a clean error.
            let bytes = fs::read(&file_map_path).unwrap_or_default();
            let v: serde_json::Value =
                serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
            let found = v
                .get("schema_version")
                .and_then(|x| x.as_str())
                .unwrap_or("<unknown>")
                .to_string();
            CompileError::SchemaMismatch {
                found,
                expected: FILE_MAP_SCHEMA_VERSION.to_string(),
            }
        }
        other => CompileError::FileMap(other),
    })?;

    let symbol_map_path = repo_canonical.join(".ctxc").join("symbol-map.json");
    let symbol_map = if symbol_map_path.exists() {
        SymbolMap::load(&symbol_map_path).ok()
    } else {
        None
    };

    // Resolve output path; default = <repo>/.ctxc/compiled-context.md.
    let output_path = output.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        repo_canonical.join(".ctxc").join(COMPILED_FILENAME)
    });

    // Defensiva: output não pode ser o file-map.json.
    if let (Ok(o), Ok(f)) = (
        fs::canonicalize(output_path.parent().unwrap_or(Path::new(".")))
            .map(|p| p.join(output_path.file_name().unwrap_or_default())),
        fs::canonicalize(&file_map_path),
    ) {
        if o == f {
            return Err(CompileError::OutputConflict(
                "--output cannot be the file-map.json itself".to_string(),
            ));
        }
    }
    if output_path == file_map_path {
        return Err(CompileError::OutputConflict(
            "--output cannot be the file-map.json itself".to_string(),
        ));
    }

    // M3: warning de volume.
    if map.summary.total_tokens_estimate > WARN_TOKEN_THRESHOLD {
        eprintln!(
            "warning: estimated_tokens={} exceeds {}; output may overflow LLM context budgets",
            map.summary.total_tokens_estimate, WARN_TOKEN_THRESHOLD
        );
    }

    let repo_root_for_resolve: PathBuf = match map.repo.canonical_path.as_ref() {
        Some(p) => PathBuf::from(p),
        None => repo_canonical.clone(),
    };

    // Iterar entries em ordem do array (determinismo herdado do file-map).
    let mut blocks: Vec<(String, u64, BlockBody)> = Vec::new();
    let mut non_utf8_elided: u64 = 0;
    let mut stale_entries: u64 = 0;
    let mut skeletonized_files: u64 = 0;
    let mut file_losses: Vec<FileLoss> = Vec::new();
    let mut total_final_tokens: u64 = 0;
    let mut total_original_tokens: u64 = 0;
    let mut all_selections: Vec<crate::ranker::SelectedSymbol> = Vec::new();

    // BATCHING SPRINT 5: Coleta de símbolos para classificação paralela
    let mut skim_results: std::collections::HashMap<(String, String), SkimOutput> = std::collections::HashMap::new();
    
    if let Some(ref client) = skim_client {
        let mut skim_inputs = Vec::new();
        let max_skim_calls = 50;

        for entry in &map.entries {
            if !matches!(entry.status, EntryStatus::Considered) || !matches!(entry.kind, EntryKind::File) {
                continue;
            }
            let rel_path = entry.path.as_ref().unwrap();
            let joined = repo_root_for_resolve.join(rel_path);
            let resolved = match fs::canonicalize(&joined) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let meta = match fs::symlink_metadata(&resolved) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let actual_size = meta.len();
            let final_mode = determine_selection_mode(entry, task, log, actual_size);
            
            if final_mode == SelectionMode::Skeleton || final_mode == SelectionMode::Full {
                let path_lower = rel_path.to_lowercase();
                let file_name = path_lower.split('/').next_back().unwrap_or("");
                let task_lower = task.to_lowercase();
                let log_lower = log.map(|l| l.to_lowercase());
                let matches_task = !file_name.is_empty() && task_lower.contains(file_name);
                let matches_log = log_lower.as_ref().is_some_and(|l| l.contains(file_name) && !file_name.is_empty());
                let is_critical = file_name.contains("config") || file_name.contains("main") || matches_task || matches_log;

                if !is_critical {
                    if let Some(ref sm) = symbol_map {
                        if let Some(fs_syms) = sm.files.iter().find(|f| f.path == *rel_path) {
                            if let Ok(bytes) = fs::read(&resolved) {
                                let content = String::from_utf8_lossy(&bytes);
                                for s in &fs_syms.symbols {
                                    if skim_inputs.len() >= max_skim_calls { break; }
                                    
                                    // Safety Net: Pular Skim para imports/exports (sempre mantidos)
                                    if s.kind == "import" || s.kind == "export" {
                                        continue;
                                    }

                                    skim_inputs.push(SkimInput {
                                        task: task.to_string(),
                                        file: rel_path.clone(),
                                        symbol_name: s.name.clone(),
                                        symbol_kind: s.kind.clone(),
                                        content: content[s.range.start_byte..s.range.end_byte].to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            if skim_inputs.len() >= max_skim_calls { break; }
        }

        if !skim_inputs.is_empty() {
            eprintln!("ctxc: skim classifying {} symbols in parallel...", skim_inputs.len());
            let results = client.classify_bulk(skim_inputs);
            for (input, res) in results {
                if let Ok(output) = res {
                    skim_results.insert((input.file, input.symbol_name), output);
                }
            }
        }
    }

    for entry in &map.entries {
        // Filtro inicial: apenas Considered + File entram no compile.
        if !matches!(entry.status, EntryStatus::Considered) {
            continue;
        }
        if !matches!(entry.kind, EntryKind::File) {
            continue;
        }

        // M1: double-check defensivo anti-sensitive.
        if entry.is_sensitive_candidate {
            return Err(CompileError::SensitiveLeak {
                path: entry
                    .path
                    .clone()
                    .unwrap_or_else(|| "<no path>".to_string()),
            });
        }
        if matches!(entry.ignore_category, Some(IgnoreCategory::Sensitive)) {
            return Err(CompileError::SensitiveLeak {
                path: entry
                    .path
                    .clone()
                    .unwrap_or_else(|| "<no path>".to_string()),
            });
        }

        let rel_path = match entry.path.as_ref() {
            Some(p) => p.clone(),
            None => {
                return Err(CompileError::SensitiveLeak {
                    path: "<no path on considered entry>".to_string(),
                });
            }
        };

        // Path traversal protection — lexical first.
        check_relative_path(&rel_path)?;

        // Resolve + canonicalize + prefix check.
        let joined = repo_root_for_resolve.join(&rel_path);
        let resolved = fs::canonicalize(&joined).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                CompileError::EntryMissing {
                    path: rel_path.clone(),
                }
            } else {
                CompileError::Io(e)
            }
        })?;
        if !resolved.starts_with(&repo_root_for_resolve) {
            return Err(CompileError::PathTraversal {
                path: rel_path.clone(),
            });
        }

        // Sanity: arquivo regular, não symlink.
        let meta = fs::symlink_metadata(&resolved)?;
        if !meta.is_file() {
            return Err(CompileError::EntryMissing {
                path: rel_path.clone(),
            });
        }

        // M2: stale entry warning.
        let actual_size = meta.len();
        if let Some(declared) = entry.size_bytes {
            if actual_size != declared {
                let delta: i128 = actual_size as i128 - declared as i128;
                let sign = if delta >= 0 { "+" } else { "-" };
                eprintln!(
                    "warning: stale entry \"{}\": file-map={}B, current={}B, delta={}{}B",
                    rel_path,
                    declared,
                    actual_size,
                    sign,
                    delta.abs()
                );
                stale_entries = stale_entries.saturating_add(1);
            }
        }

        let bytes = fs::read(&resolved)?;

        let mode = determine_selection_mode(entry, task, log, actual_size);
        let is_stale = entry.size_bytes.is_some_and(|s| s != actual_size);
        let final_mode = if is_stale {
            if actual_size > 60_000 {
                SelectionMode::Elided
            } else {
                SelectionMode::Full
            }
        } else {
            mode
        };

        let content_utf8 = std::str::from_utf8(&bytes).ok();

        let (content_to_use, mode_str, elided, skeletonized) = match (final_mode, content_utf8, &symbol_map) {
            (m, Some(content), Some(sm)) if m == SelectionMode::Skeleton || m == SelectionMode::Full => {
                let mut file_selections = Vec::new();
                if let Some(fs) = sm.files.iter().find(|f| f.path == rel_path) {
                    for s in &fs.symbols {
                        let mut symbol_mode = if m == SelectionMode::Full { SelectionMode::Full } else { SelectionMode::Skeleton };
                        let mut reason = if m == SelectionMode::Full { "file_rank_full".to_string() } else { "file_rank_skeleton".to_string() };
                        let mut score = if m == SelectionMode::Full { 1.0 } else { 0.5 };

                        // Safety Net: Manter imports e exports sempre (Sprint 5)
                        let is_import_export = s.kind == "import" || s.kind == "export";
                        
                        if is_import_export {
                            symbol_mode = SelectionMode::Full;
                            reason = "safety_net: import/export preservation".to_string();
                            score = 1.0;
                        } else if let Some(output) = skim_results.get(&(rel_path.clone(), s.name.clone())) {
                            // Aplicar resultados do Skim apenas se não for import/export
                            match output.classification {
                                SkimClassification::Keep => {
                                    symbol_mode = SelectionMode::Full;
                                    reason = format!("skim_keep: {}", output.reason);
                                    score = 0.9;
                                },
                                SkimClassification::Drop => {
                                    symbol_mode = SelectionMode::Elided;
                                    reason = format!("skim_drop: {}", output.reason);
                                    score = 0.1;
                                },
                                SkimClassification::Summarize => {
                                    symbol_mode = SelectionMode::Skeleton;
                                    reason = format!("skim_summarize: {}", output.reason);
                                    score = 0.5;
                                }
                            }
                        }

                        let symbol_sel = crate::ranker::SelectedSymbol {
                            file_path: rel_path.clone(),
                            symbol_name: s.name.clone(),
                            mode: symbol_mode,
                            reason,
                            score,
                        };
                        file_selections.push(symbol_sel.clone());
                        all_selections.push(symbol_sel);
                    }
                }
                
                let file_planner = Planner::new(sm, file_selections);
                let res = file_planner.skeletonize_file(&rel_path, content);
                if m == SelectionMode::Skeleton || !res.elided_symbols.is_empty() || !res.skeletonized_symbols.is_empty() {
                    if m == SelectionMode::Skeleton { skeletonized_files += 1; }
                    (
                        res.content.into_bytes(),
                        if m == SelectionMode::Full { "Full (Pruned)".to_string() } else { "Skeleton".to_string() },
                        res.elided_symbols,
                        res.skeletonized_symbols,
                    )
                } else {
                    (bytes.clone(), "Full".to_string(), Vec::new(), Vec::new())
                }
            }
            (SelectionMode::Elided, _, _) => {
                eprintln!("ctxc: safety lock: eliding stale large file '{}'", rel_path);
                (
                    Vec::new(),
                    "Elided".to_string(),
                    vec!["[whole file elided: security lock]".to_string()],
                    Vec::new(),
                )
            }
            _ => {
                if let Some(ref sm) = symbol_map {
                    if let Some(fs) = sm.files.iter().find(|f| f.path == rel_path) {
                        for s in &fs.symbols {
                            all_selections.push(crate::ranker::SelectedSymbol {
                                file_path: rel_path.clone(),
                                symbol_name: s.name.clone(),
                                mode: SelectionMode::Full,
                                reason: "file_rank_full".to_string(),
                                score: 1.0,
                            });
                        }
                    }
                }
                (bytes.clone(), "Full".to_string(), Vec::new(), Vec::new())
            }
        };

        let final_size = content_to_use.len() as u64;
        let final_tokens = final_size.div_ceil(3);
        total_final_tokens += final_tokens;

        let original_tokens = actual_size.div_ceil(3);
        total_original_tokens += original_tokens;
        let tokens_saved = original_tokens.saturating_sub(final_tokens);

        file_losses.push(FileLoss {
            path: rel_path.clone(),
            mode: mode_str,
            tokens_saved,
            elided_symbols: elided,
            skeletonized_symbols: skeletonized,
        });

        let block = if final_mode == SelectionMode::Elided {
            BlockBody::Elided {
                reason: "security lock / stale large file".to_string(),
            }
        } else {
            build_block(&rel_path, &content_to_use, &mut non_utf8_elided)
        };
        blocks.push((rel_path, actual_size, block));
    }

    // B-012: Gerar Loss Report
    let reduction_ratio = if total_original_tokens > 0 {
        (total_original_tokens as f32 - total_final_tokens as f32) / total_original_tokens as f32
    } else {
        0.0
    };

    let report = LossReport {
        schema_version: "1.0.0".to_string(),
        summary: LossSummary {
            original_tokens: total_original_tokens,
            final_tokens: total_final_tokens,
            reduction_ratio,
        },
        files: file_losses,
    };
    let _ = LossReport::write_atomic(&repo_canonical, &report);

    // B-010: Gerar Context IR v0 (consolidado pós-ranking)
    if let Some(ref sm) = symbol_map {
        let planner = Planner::new(sm, all_selections);
        let ir = planner.generate_ir(task);
        let ir_path = repo_canonical.join(".ctxc").join("context.ir.json");
        if let Ok(ir_json) = serde_json::to_string_pretty(&ir) {
            let _ = write_atomic(&ir_path, ir_json.as_bytes());
        }
    }

    let markdown = render_markdown(
        task,
        &map,
        &blocks,
        non_utf8_elided,
        stale_entries,
    );

    write_atomic(&output_path, markdown.as_bytes())?;

    Ok((
        output_path,
        blocks.len() as u64,
        map.summary.total_bytes_considered,
        map.summary.total_tokens_estimate,
        stale_entries,
        skeletonized_files,
    ))
}

fn determine_selection_mode(
    entry: &Entry,
    task: &str,
    log: Option<&str>,
    size_bytes: u64,
) -> SelectionMode {
    let task_lower = task.to_lowercase();
    let log_lower = log.map(|l| l.to_lowercase());
    let rel_path = entry.path.as_deref().unwrap_or("");
    let path_lower = rel_path.to_lowercase();
    let file_name = path_lower.split('/').next_back().unwrap_or("");

    // 1. FULL: Arquivos 'config', 'main' ou match com '--task' (ou logs).
    let is_config_or_main = file_name.contains("config") || file_name.contains("main");
    let matches_task = !file_name.is_empty() && task_lower.contains(file_name);
    let matches_log = log_lower
        .as_ref()
        .is_some_and(|l| l.contains(file_name) && !file_name.is_empty());

    if is_config_or_main || matches_task || matches_log {
        return SelectionMode::Full;
    }

    // 2. PROMOÇÃO: Se a task tiver 'test', 'fail' ou 'error', promova arquivos de teste para FULL.
    let task_has_error_kw = task_lower.contains("test")
        || task_lower.contains("fail")
        || task_lower.contains("error")
        || log_lower
            .as_ref()
            .is_some_and(|l| l.contains("fail") || l.contains("error"));
    let is_test_file = file_name.contains("test");

    if task_has_error_kw && is_test_file {
        return SelectionMode::Full;
    }

    // 3. SKELETON: Arquivos 'test', 'doc' ou > 10k tokens (sem match).
    let is_doc_file = file_name.contains("doc");
    let exceeds_token_limit = size_bytes > 30_000;

    if is_test_file || is_doc_file || exceeds_token_limit {
        return SelectionMode::Skeleton;
    }

    SelectionMode::Full
}

fn validate_task(task: &str) -> Result<(), CompileError> {
    if task.is_empty() {
        return Err(CompileError::InvalidTask("--task must not be empty".to_string()));
    }
    if task.len() > MAX_TASK_LEN {
        return Err(CompileError::InvalidTask(format!(
            "--task exceeds {} chars (got {})",
            MAX_TASK_LEN,
            task.len()
        )));
    }
    for c in task.chars() {
        match c {
            '\n' => return Err(CompileError::InvalidTask("contains newline".to_string())),
            '\r' => return Err(CompileError::InvalidTask("contains carriage return".to_string())),
            '\0' => return Err(CompileError::InvalidTask("contains NUL".to_string())),
            _ => {}
        }
    }
    Ok(())
}

fn check_relative_path(path: &str) -> Result<(), CompileError> {
    if path.is_empty() {
        return Err(CompileError::PathTraversal {
            path: path.to_string(),
        });
    }
    // Reject absolute paths (POSIX or Windows).
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(CompileError::PathTraversal {
            path: path.to_string(),
        });
    }
    let p = Path::new(path);
    for c in p.components() {
        match c {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CompileError::PathTraversal {
                    path: path.to_string(),
                });
            }
            _ => {}
        }
    }
    Ok(())
}

fn build_block(rel_path: &str, bytes: &[u8], non_utf8_counter: &mut u64) -> BlockBody {
    match std::str::from_utf8(bytes) {
        Ok(s) => match pick_fence(s) {
            Some(fence) => BlockBody::Fenced {
                fence,
                lang: lang_from_path(rel_path),
                body: s.to_string(),
            },
            None => BlockBody::FenceCollision,
        },
        Err(_) => {
            *non_utf8_counter = non_utf8_counter.saturating_add(1);
            BlockBody::NonUtf8 { bytes: bytes.len() }
        }
    }
}

/// Conta o maior número de backticks consecutivos em `s`.
fn max_consecutive_backticks(s: &str) -> usize {
    let mut max = 0usize;
    let mut cur = 0usize;
    for ch in s.chars() {
        if ch == '`' {
            cur += 1;
            if cur > max {
                max = cur;
            }
        } else {
            cur = 0;
        }
    }
    max
}

/// Escolhe `fence = max(3, max_consecutive + 1)` para o conteúdo. Se o conteúdo
/// tiver 8+ backticks consecutivos, retorna `None` para sinalizar fence
/// collision (placeholder).
fn pick_fence(content: &str) -> Option<usize> {
    let max = max_consecutive_backticks(content);
    if max >= 8 {
        return None;
    }
    Some(std::cmp::max(3, max + 1))
}

fn lang_from_path(rel_path: &str) -> &'static str {
    let ext = match rel_path.rsplit('.').next() {
        Some(e) if !e.is_empty() && e != rel_path => e.to_ascii_lowercase(),
        _ => return "text",
    };
    match ext.as_str() {
        "rs" => "rust",
        "py" => "python",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "go" => "go",
        "java" => "java",
        "c" => "c",
        "cpp" | "cc" | "cxx" => "cpp",
        "h" => "c",
        "hpp" => "cpp",
        "md" => "md",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "sh" | "bash" => "bash",
        "html" => "html",
        "css" => "css",
        _ => "text",
    }
}

fn render_markdown(
    task: &str,
    map: &FileMap,
    blocks: &[(String, u64, BlockBody)],
    non_utf8_elided: u64,
    stale_entries: u64,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Compiled Context — {task}\n\n"));
    out.push_str("> Generated by Context Compiler · schema 1.1.0 · deterministico\n\n");
    out.push_str("| campo                  | valor             |\n");
    out.push_str("|------------------------|-------------------|\n");
    out.push_str(&format!("| repo                   | {} |\n", map.repo.root));
    out.push_str(&format!(
        "| schema_version         | {} |\n",
        map.schema_version
    ));
    out.push_str(&format!("| files_in_compile       | {} |\n", blocks.len()));
    out.push_str(&format!(
        "| total_bytes_considered | {} |\n",
        map.summary.total_bytes_considered
    ));
    out.push_str(&format!(
        "| total_tokens_estimate  | {} |\n",
        map.summary.total_tokens_estimate
    ));
    out.push_str(&format!("| non_utf8_elided        | {} |\n", non_utf8_elided));
    out.push_str(&format!("| stale_entries          | {} |\n", stale_entries));
    out.push_str("| generated_at           | n/a |\n");
    out.push_str("\n---\n\n");

    for (rel_path, size, body) in blocks {
        out.push_str(&format!("## `{rel_path}`\n\n"));
        out.push_str(&format!("`{size} bytes`\n\n"));
        match body {
            BlockBody::Fenced { fence, lang, body } => {
                let bar = "`".repeat(*fence);
                out.push_str(&format!("{bar}{lang}\n"));
                out.push_str(body);
                if !body.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&format!("{bar}\n\n"));
            }
            BlockBody::NonUtf8 { bytes } => {
                out.push_str("```text\n");
                out.push_str(&format!("[non-UTF-8 content elided, {bytes} bytes]\n"));
                out.push_str("```\n\n");
            }
            BlockBody::FenceCollision => {
                out.push_str("```text\n");
                out.push_str("[content elided: contains markdown fence collision]\n");
                out.push_str("```\n\n");
            }
            BlockBody::Elided { reason } => {
                out.push_str("```text\n");
                out.push_str(&format!("[content elided: {}]\n", reason));
                out.push_str("```\n\n");
            }
        }
        out.push_str("---\n\n");
    }

    out
}

fn write_atomic(dest: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = dest
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "destino sem diretório pai"))?;
    fs::create_dir_all(dir)?;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(
        ".compiled-context.md.tmp.{}.{}",
        std::process::id(),
        nanos
    ));

    let write_result: io::Result<()> = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        Ok(())
    })();
    if let Err(e) = write_result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    if let Err(e) = fs::rename(&tmp, dest) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_task_aceita_basico_e_rejeita_controle() {
        assert!(validate_task("debug error in foo()").is_ok());
        assert!(validate_task("").is_err());
        assert!(validate_task("contains\nnewline").is_err());
        assert!(validate_task("contains\rCR").is_err());
        assert!(validate_task("contains\0NUL").is_err());
        let long = "x".repeat(MAX_TASK_LEN + 1);
        assert!(validate_task(&long).is_err());
        let exact = "x".repeat(MAX_TASK_LEN);
        assert!(validate_task(&exact).is_ok());
    }

    #[test]
    fn check_relative_path_rejeita_absoluto_e_dotdot() {
        assert!(check_relative_path("/etc/passwd").is_err());
        assert!(check_relative_path("../etc/passwd").is_err());
        assert!(check_relative_path("a/../b").is_err());
        assert!(check_relative_path("").is_err());
        assert!(check_relative_path("a/b.txt").is_ok());
        assert!(check_relative_path("README.md").is_ok());
    }

    #[test]
    fn pick_fence_default_e_adapta() {
        assert_eq!(pick_fence(""), Some(3));
        assert_eq!(pick_fence("hello"), Some(3));
        assert_eq!(pick_fence("```"), Some(4));
        assert_eq!(pick_fence("````"), Some(5));
        assert_eq!(pick_fence("```````"), Some(8));
        assert_eq!(pick_fence("````````"), None); // 8+ → placeholder
    }

    #[test]
    fn lang_from_path_cobre_extensoes_principais() {
        assert_eq!(lang_from_path("a.rs"), "rust");
        assert_eq!(lang_from_path("a.py"), "python");
        assert_eq!(lang_from_path("a.ts"), "typescript");
        assert_eq!(lang_from_path("a.tsx"), "typescript");
        assert_eq!(lang_from_path("a.js"), "javascript");
        assert_eq!(lang_from_path("a.go"), "go");
        assert_eq!(lang_from_path("a.md"), "md");
        assert_eq!(lang_from_path("a.unknown"), "text");
        assert_eq!(lang_from_path("noext"), "text");
        assert_eq!(lang_from_path("Makefile"), "text");
    }

    #[test]
    fn max_consecutive_backticks_conta_corretamente() {
        assert_eq!(max_consecutive_backticks(""), 0);
        assert_eq!(max_consecutive_backticks("hello"), 0);
        assert_eq!(max_consecutive_backticks("```"), 3);
        assert_eq!(max_consecutive_backticks("`a``b```c"), 3);
        assert_eq!(max_consecutive_backticks("```` x ``"), 4);
    }
}
