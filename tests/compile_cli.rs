//! Testes de integração B-006: `ctxc compile --task <desc> --repo <path>`.
//!
//! Cobre as 4 mitigações duras de Cassandra (M1, M2, M3, M4) + path traversal
//! + atomicidade + determinismo + fence collision + non-UTF8.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn ctxc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ctxc"))
}

fn unique_tempdir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ctxc-b006-{}-{}-{}",
        label,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

/// Roda `ctxc scan` para popular `.ctxc/file-map.json` no tempdir, depois
/// retorna o output dele para inspeção opcional.
fn run_scan(repo: &Path) -> std::process::Output {
    let out = ctxc().arg("scan").arg("--repo").arg(repo).output().unwrap();
    assert!(
        out.status.success(),
        "scan setup falhou: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn run_compile(repo: &Path, task: &str) -> std::process::Output {
    ctxc()
        .arg("compile")
        .arg("--task")
        .arg(task)
        .arg("--repo")
        .arg(repo)
        .output()
        .unwrap()
}

fn run_compile_args(args: &[&str]) -> std::process::Output {
    let mut c = ctxc();
    c.arg("compile");
    for a in args {
        c.arg(a);
    }
    c.output().unwrap()
}

fn write_file_map(repo: &Path, json: &str) {
    let dir = repo.join(".ctxc");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("file-map.json"), json).unwrap();
}

fn canonical(repo: &Path) -> String {
    fs::canonicalize(repo).unwrap().to_string_lossy().into_owned()
}

// ---------- 1. happy path ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_happy_path_3_files_considered() {
    let tmp = unique_tempdir("happy");
    fs::write(tmp.join("a.rs"), b"fn main(){}").unwrap();
    fs::write(tmp.join("README.md"), b"# hello").unwrap();
    fs::write(tmp.join("b.py"), b"print(1)").unwrap();
    let _ = run_scan(&tmp);

    let out = run_compile(&tmp, "fix bug");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("compiled 3 files"), "stdout: {stdout}");
    assert!(stdout.contains("-> "));
    assert!(out.stderr.is_empty());

    let md = fs::read_to_string(tmp.join(".ctxc").join("compiled-context.md")).unwrap();
    assert!(md.starts_with("# Compiled Context — fix bug"));
    assert!(md.contains("schema 1.1.0"));
    assert!(md.contains("| schema_version         | 1.1.0 |"));
    assert!(md.contains("| files_in_compile       | 3 |"));
    assert!(md.contains("| stale_entries          | 0 |"));
    assert!(md.contains("```rust\nfn main(){}"));
    assert!(md.contains("```python\nprint(1)"));
    assert!(md.contains("```md\n# hello"));

    cleanup(&tmp);
}

// ---------- 2. file-map ausente ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_file_map_ausente_erro_claro() {
    let tmp = unique_tempdir("no-file-map");
    fs::write(tmp.join("a.rs"), b"x").unwrap();
    let out = run_compile(&tmp, "task");
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("file-map.json not found"),
        "stderr: {stderr}"
    );
    assert!(!tmp.join(".ctxc").join("compiled-context.md").exists());
    cleanup(&tmp);
}

// ---------- 3 & 4. M4 — schema mismatch ----------

fn minimal_file_map_with_version(version: &str, repo_root: &str, repo_canonical: &str) -> String {
    format!(
        r#"{{"schema_version":"{version}","repo":{{"root":"{root}","canonical_path":"{can}"}},"entries":[],"summary":{{"considered":0,"ignored":0,"dirs_ignored":0,"sensitive_ignored":0,"binary_ignored":0,"large_ignored":0,"total_bytes_considered":0,"total_tokens_estimate":0}}}}"#,
        version = version,
        root = repo_root,
        can = repo_canonical
    )
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_m4_schema_version_1_0_0_eh_erro_fatal() {
    let tmp = unique_tempdir("schema-1-0-0");
    let can = canonical(&tmp);
    let json = minimal_file_map_with_version("1.0.0", &tmp.display().to_string(), &can);
    write_file_map(&tmp, &json);

    let out = run_compile(&tmp, "task");
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("schema_version=1.0.0") && stderr.contains("expected exactly 1.1.0"),
        "stderr: {stderr}"
    );
    assert!(!tmp.join(".ctxc").join("compiled-context.md").exists());
    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_m4_schema_version_1_2_0_eh_erro_fatal() {
    let tmp = unique_tempdir("schema-1-2-0");
    let can = canonical(&tmp);
    let json = minimal_file_map_with_version("1.2.0", &tmp.display().to_string(), &can);
    write_file_map(&tmp, &json);

    let out = run_compile(&tmp, "task");
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("schema_version=1.2.0") && stderr.contains("expected exactly 1.1.0"),
        "stderr: {stderr}"
    );
    cleanup(&tmp);
}

// ---------- 5. canário deny_unknown_fields ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_canario_campo_extra_em_file_map_eh_rejeitado() {
    let tmp = unique_tempdir("extra-field");
    let can = canonical(&tmp);
    let json = format!(
        r#"{{"schema_version":"1.1.0","repo":{{"root":"{root}","canonical_path":"{can}"}},"entries":[],"summary":{{"considered":0,"ignored":0,"dirs_ignored":0,"sensitive_ignored":0,"binary_ignored":0,"large_ignored":0,"total_bytes_considered":0,"total_tokens_estimate":0}},"forbidden_field":42}}"#,
        root = tmp.display(),
        can = can
    );
    write_file_map(&tmp, &json);

    let out = run_compile(&tmp, "task");
    assert!(!out.status.success(), "campo extra deve ser rejeitado");
    cleanup(&tmp);
}

// ---------- 6. dir/symlink/other não entram ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_kind_dir_symlink_other_nao_entram_no_markdown() {
    let tmp = unique_tempdir("kinds");
    fs::write(tmp.join("real.rs"), b"x").unwrap();
    let _ = run_scan(&tmp);

    // Manualmente injeta uma entry kind=dir e outra kind=other no file-map.
    let path = tmp.join(".ctxc").join("file-map.json");
    let mut map: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let entries = map["entries"].as_array_mut().unwrap();
    entries.push(serde_json::json!({
        "kind": "dir",
        "status": "considered",
        "is_hidden": false,
        "is_sensitive_candidate": false,
        "path": "should_not_be_read_dir",
        "depth": 1
    }));
    entries.push(serde_json::json!({
        "kind": "other",
        "status": "considered",
        "is_hidden": false,
        "is_sensitive_candidate": false,
        "path": "should_not_be_read_other",
        "depth": 1
    }));
    fs::write(&path, serde_json::to_string(&map).unwrap()).unwrap();

    let out = run_compile(&tmp, "task");
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let md = fs::read_to_string(tmp.join(".ctxc").join("compiled-context.md")).unwrap();
    assert!(!md.contains("should_not_be_read_dir"));
    assert!(!md.contains("should_not_be_read_other"));
    assert!(md.contains("real.rs"));

    cleanup(&tmp);
}

// ---------- 7. sensitive (status=ignored) não entra ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_sensitive_ignored_nao_entra_no_markdown() {
    let tmp = unique_tempdir("sensitive-skip");
    fs::write(tmp.join("a.rs"), b"x").unwrap();
    fs::write(tmp.join(".env"), b"X=1").unwrap();
    let _ = run_scan(&tmp);

    let out = run_compile(&tmp, "task");
    assert!(out.status.success());
    let md = fs::read_to_string(tmp.join(".ctxc").join("compiled-context.md")).unwrap();
    assert!(!md.contains(".env"), "markdown não pode citar .env");
    assert!(md.contains("a.rs"));

    cleanup(&tmp);
}

// ---------- 8 & 9. M1: scanner bug detection ----------

fn file_map_with_one_entry(repo: &Path, entry: serde_json::Value) -> String {
    let can = canonical(repo);
    let summary = serde_json::json!({
        "considered": 1,
        "ignored": 0,
        "dirs_ignored": 0,
        "sensitive_ignored": 0,
        "binary_ignored": 0,
        "large_ignored": 0,
        "total_bytes_considered": 0,
        "total_tokens_estimate": 0
    });
    serde_json::json!({
        "schema_version": "1.1.0",
        "repo": {"root": repo.display().to_string(), "canonical_path": can},
        "entries": [entry],
        "summary": summary
    })
    .to_string()
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_m1_considered_com_ignore_category_sensitive_aborta() {
    let tmp = unique_tempdir("m1-sensitive-cat");
    fs::write(tmp.join("a.rs"), b"x").unwrap();
    let json = file_map_with_one_entry(
        &tmp,
        serde_json::json!({
            "kind": "file",
            "status": "considered",
            "is_hidden": false,
            "is_sensitive_candidate": false,
            "ignore_category": "sensitive",
            "path": "a.rs",
            "depth": 1
        }),
    );
    write_file_map(&tmp, &json);

    let out = run_compile(&tmp, "task");
    assert!(!out.status.success(), "M1 deve abortar");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("scanner bug") && stderr.contains("sensitive"),
        "stderr: {stderr}"
    );
    assert!(!tmp.join(".ctxc").join("compiled-context.md").exists());
    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_m1_considered_com_is_sensitive_candidate_true_aborta() {
    let tmp = unique_tempdir("m1-sensitive-flag");
    fs::write(tmp.join("a.rs"), b"x").unwrap();
    let json = file_map_with_one_entry(
        &tmp,
        serde_json::json!({
            "kind": "file",
            "status": "considered",
            "is_hidden": false,
            "is_sensitive_candidate": true,
            "path": "a.rs",
            "depth": 1
        }),
    );
    write_file_map(&tmp, &json);

    let out = run_compile(&tmp, "task");
    assert!(!out.status.success(), "M1 deve abortar");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("scanner bug") && stderr.contains("sensitive"), "stderr: {stderr}");
    cleanup(&tmp);
}

// ---------- 10. entry com path inexistente ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_entry_path_inexistente_eh_erro_fatal() {
    let tmp = unique_tempdir("missing-entry");
    let json = file_map_with_one_entry(
        &tmp,
        serde_json::json!({
            "kind": "file",
            "status": "considered",
            "is_hidden": false,
            "is_sensitive_candidate": false,
            "path": "ghost.rs",
            "depth": 1
        }),
    );
    write_file_map(&tmp, &json);

    let out = run_compile(&tmp, "task");
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("ghost.rs") && stderr.contains("missing"), "stderr: {stderr}");
    assert!(!tmp.join(".ctxc").join("compiled-context.md").exists());
    cleanup(&tmp);
}

// ---------- 11 & 12. M2: stale entry warnings ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_m2_arquivo_cresceu_apos_scan_emit_stale_delta_positivo() {
    let tmp = unique_tempdir("m2-grow");
    fs::write(tmp.join("a.rs"), b"x").unwrap();
    let _ = run_scan(&tmp);
    // Cresce de 1 para 5 bytes após scan.
    fs::write(tmp.join("a.rs"), b"hello").unwrap();

    let out = run_compile(&tmp, "task");
    assert!(out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("stale entry \"a.rs\"")
            && stderr.contains("file-map=1B")
            && stderr.contains("current=5B")
            && stderr.contains("delta=+4B"),
        "stderr: {stderr}"
    );

    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("(stale=1)"), "stdout: {stdout}");
    let md = fs::read_to_string(tmp.join(".ctxc").join("compiled-context.md")).unwrap();
    assert!(md.contains("| stale_entries          | 1 |"));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_m2_arquivo_encolheu_apos_scan_emit_stale_delta_negativo() {
    let tmp = unique_tempdir("m2-shrink");
    fs::write(tmp.join("a.rs"), b"hello world").unwrap(); // 11 bytes
    let _ = run_scan(&tmp);
    fs::write(tmp.join("a.rs"), b"hi").unwrap(); // 2 bytes

    let out = run_compile(&tmp, "task");
    assert!(out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("delta=-9B"),
        "esperava delta=-9B; stderr: {stderr}"
    );

    cleanup(&tmp);
}

// ---------- 13 & 14. M3: token volume warning ----------

fn file_map_with_token_estimate(repo: &Path, total_tokens: u64) -> String {
    let can = canonical(repo);
    let summary = serde_json::json!({
        "considered": 0,
        "ignored": 0,
        "dirs_ignored": 0,
        "sensitive_ignored": 0,
        "binary_ignored": 0,
        "large_ignored": 0,
        "total_bytes_considered": total_tokens.saturating_mul(3),
        "total_tokens_estimate": total_tokens
    });
    serde_json::json!({
        "schema_version": "1.1.0",
        "repo": {"root": repo.display().to_string(), "canonical_path": can},
        "entries": [],
        "summary": summary
    })
    .to_string()
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_m3_estimativa_acima_de_50k_emit_warning() {
    let tmp = unique_tempdir("m3-warn");
    let json = file_map_with_token_estimate(&tmp, 50_001);
    write_file_map(&tmp, &json);

    let out = run_compile(&tmp, "task");
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("estimated_tokens=50001") && stderr.contains("exceeds 50000"),
        "stderr: {stderr}"
    );
    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_m3_estimativa_no_limite_50000_sem_warning() {
    let tmp = unique_tempdir("m3-boundary");
    let json = file_map_with_token_estimate(&tmp, 50_000);
    write_file_map(&tmp, &json);

    let out = run_compile(&tmp, "task");
    assert!(out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        !stderr.contains("estimated_tokens"),
        "boundary 50000 NÃO pode disparar M3; stderr: {stderr}"
    );
    cleanup(&tmp);
}

// ---------- 15. non-UTF8 ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_arquivo_non_utf8_vira_placeholder_sem_abortar() {
    let tmp = unique_tempdir("non-utf8");
    fs::write(tmp.join("good.rs"), b"x").unwrap();
    fs::write(tmp.join("bad.txt"), [0xFF, 0xFE, 0x00, 0x80, 0x81]).unwrap();
    let _ = run_scan(&tmp);

    let out = run_compile(&tmp, "task");
    assert!(out.status.success());
    let md = fs::read_to_string(tmp.join(".ctxc").join("compiled-context.md")).unwrap();
    assert!(md.contains("[non-UTF-8 content elided, 5 bytes]"));
    assert!(md.contains("| non_utf8_elided        | 1 |"));

    cleanup(&tmp);
}

// ---------- 16. fence collision ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_fence_collision_4_backticks_usa_fence_5() {
    let tmp = unique_tempdir("fence-4");
    let content = b"prefix\n````\nbody\n````\nsuffix";
    fs::write(tmp.join("doc.md"), content).unwrap();
    let _ = run_scan(&tmp);

    let out = run_compile(&tmp, "task");
    assert!(out.status.success());
    let md = fs::read_to_string(tmp.join(".ctxc").join("compiled-context.md")).unwrap();
    assert!(md.contains("`````md\n"), "fence-5 esperado: {md}");
    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_fence_collision_8_backticks_vira_placeholder() {
    let tmp = unique_tempdir("fence-8");
    let content = b"````````\nx\n````````";
    fs::write(tmp.join("doc.md"), content).unwrap();
    let _ = run_scan(&tmp);

    let out = run_compile(&tmp, "task");
    assert!(out.status.success());
    let md = fs::read_to_string(tmp.join(".ctxc").join("compiled-context.md")).unwrap();
    assert!(md.contains("[content elided: contains markdown fence collision]"));
    cleanup(&tmp);
}

// ---------- 17 & 18. --task validation ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_task_com_newline_eh_rejeitado() {
    let tmp = unique_tempdir("task-newline");
    fs::write(tmp.join("a.rs"), b"x").unwrap();
    let _ = run_scan(&tmp);

    let out = run_compile_args(&[
        "--task",
        "first\nsecond",
        "--repo",
        &tmp.display().to_string(),
    ]);
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("invalid --task"), "stderr: {stderr}");
    assert!(!tmp.join(".ctxc").join("compiled-context.md").exists());
    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_task_257_chars_eh_rejeitado() {
    let tmp = unique_tempdir("task-long");
    fs::write(tmp.join("a.rs"), b"x").unwrap();
    let _ = run_scan(&tmp);
    let long = "x".repeat(257);

    let out = run_compile_args(&["--task", &long, "--repo", &tmp.display().to_string()]);
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("exceeds 256"), "stderr: {stderr}");
    cleanup(&tmp);
}

// ---------- 19. path traversal ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_path_traversal_eh_erro_fatal_antes_de_abrir() {
    let tmp = unique_tempdir("traversal");
    let json = file_map_with_one_entry(
        &tmp,
        serde_json::json!({
            "kind": "file",
            "status": "considered",
            "is_hidden": false,
            "is_sensitive_candidate": false,
            "path": "../etc/passwd",
            "depth": 1
        }),
    );
    write_file_map(&tmp, &json);

    let out = run_compile(&tmp, "task");
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("path traversal"), "stderr: {stderr}");
    assert!(!tmp.join(".ctxc").join("compiled-context.md").exists());
    cleanup(&tmp);
}

// ---------- 20. determinismo byte-a-byte ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_determinismo_byte_identico_entre_2_runs() {
    let tmp = unique_tempdir("determinism");
    fs::write(tmp.join("a.rs"), b"fn x(){}").unwrap();
    fs::write(tmp.join("b.md"), b"# title\n\nbody").unwrap();
    fs::write(tmp.join(".env"), b"X=1").unwrap(); // sensitive — não deve afetar
    let _ = run_scan(&tmp);

    let _ = run_compile(&tmp, "fix");
    let first = fs::read(tmp.join(".ctxc").join("compiled-context.md")).unwrap();
    let _ = run_compile(&tmp, "fix");
    let second = fs::read(tmp.join(".ctxc").join("compiled-context.md")).unwrap();

    assert_eq!(first, second, "compile precisa ser determinístico");

    cleanup(&tmp);
}

// ---------- 21. atomicidade: destino não-escrevível ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_falha_de_write_nao_deixa_compiled_parcial() {
    let tmp = unique_tempdir("atomic");
    fs::write(tmp.join("a.rs"), b"x").unwrap();
    let _ = run_scan(&tmp);

    // Pré-cria compiled-context.md como diretório → write_atomic falha em rename.
    let dest = tmp.join(".ctxc").join("compiled-context.md");
    let _ = fs::remove_file(&dest);
    fs::create_dir_all(&dest).unwrap();
    // Coloca um arquivo dentro pra create_dir_all do tmp não falhar, e rename
    // contra um diretório não-vazio falha de forma confiável em todos os FS.
    fs::write(dest.join("guard"), b"keep").unwrap();

    let out = run_compile(&tmp, "task");
    assert!(!out.status.success(), "deveria falhar com destino dir");
    // O diretório original permanece intacto (rename não substituiu).
    assert!(dest.is_dir());
    assert!(dest.join("guard").is_file());

    cleanup(&tmp);
}

// ---------- regressão: subcomandos ainda preservados ----------

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b006_inspect_e_eval_continuam_sem_sucesso() {
    for sub in ["inspect", "eval"] {
        let out = ctxc().arg(sub).output().unwrap();
        assert!(!out.status.success(), "ctxc {sub} retornou sucesso");
    }
}
