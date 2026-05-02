//! Testes de integração B-004: geração e persistência de `.ctxc/file-map.json`.
//!
//! Critério de aceite (ADR-004 + schema):
//! - schema_version do output bate com a constante do binário;
//! - documento fechado (additionalProperties:false) — canários rejeitam extras;
//! - sensíveis: nunca path/parent_dir/extension; sempre path_redacted;
//! - summary.sensitive_ignored conta TODAS as sensíveis;
//! - escrita atômica em .ctxc/; sem artefato em erro;
//! - stdout B-002/B-003 preservado, stderr vazio em sucesso, exit 0;
//! - sem rede, sem leitura de conteúdo, symlinks não seguidos;
//! - status=ignored ⇒ ignore_category presente; status=considered ⇒ ausente;
//! - idempotência byte-a-byte da escrita atômica.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ctxc::adapters::development::file_map::{
    EntryStatus, FileMap, IgnoreCategory, FILE_MAP_SCHEMA_VERSION,
};

fn ctxc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ctxc"))
}

fn unique_tempdir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ctxc-b004-{}-{}-{}",
        label,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&p).expect("criar tempdir");
    p
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

fn run_scan_ok(repo: &Path) -> std::process::Output {
    let out = ctxc().arg("scan").arg("--repo").arg(repo).output().unwrap();
    assert!(
        out.status.success(),
        "scan falhou; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty(), "stderr não vazio em sucesso");
    out
}

fn read_file_map(repo: &Path) -> FileMap {
    let dest = repo.join(".ctxc").join("file-map.json");
    let raw = fs::read_to_string(&dest).expect("file-map.json deve existir");
    serde_json::from_str(&raw).expect("file-map.json deve desserializar contra o tipo strict")
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_repo_valido_gera_file_map_persistido() {
    let tmp = unique_tempdir("valid");
    fs::write(tmp.join("a.rs"), b"fn main(){}").unwrap();
    fs::write(tmp.join("README.md"), b"# r").unwrap();

    let _ = run_scan_ok(&tmp);

    let dest = tmp.join(".ctxc").join("file-map.json");
    assert!(dest.exists(), ".ctxc/file-map.json não foi criado");

    let map = read_file_map(&tmp);
    assert_eq!(map.summary.considered, 2);
    assert_eq!(map.summary.ignored, 0);
    assert!(map.entries.iter().all(|e| e.path.is_some()));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_schema_version_do_arquivo_bate_com_constante_do_binario() {
    let tmp = unique_tempdir("schema-version");
    fs::write(tmp.join("a.rs"), b"f").unwrap();

    let _ = run_scan_ok(&tmp);
    let map = read_file_map(&tmp);
    assert_eq!(map.schema_version, FILE_MAP_SCHEMA_VERSION);

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_repo_inexistente_nao_cria_artefato() {
    let tmp_parent = unique_tempdir("parent-of-nonexistent");
    let nonexistent = tmp_parent.join("does-not-exist");

    let out = ctxc()
        .arg("scan")
        .arg("--repo")
        .arg(&nonexistent)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!nonexistent.exists());
    assert!(!nonexistent.join(".ctxc").exists());

    cleanup(&tmp_parent);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_repo_eh_arquivo_nao_cria_artefato() {
    let tmp = unique_tempdir("file-as-repo");
    let f = tmp.join("just_a_file.txt");
    fs::write(&f, b"x").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&f).output().unwrap();
    assert!(!out.status.success());
    assert!(!tmp.join(".ctxc").exists(), ".ctxc não pode aparecer no diretório-pai");

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_entrada_sensivel_nao_expoe_path_e_conta_no_summary() {
    let tmp = unique_tempdir("sensitive");
    fs::write(tmp.join("README.md"), b"r").unwrap();
    fs::write(tmp.join(".env"), b"X=1").unwrap();
    fs::write(tmp.join(".env.production"), b"Y=2").unwrap();
    fs::write(tmp.join("server.pem"), b"k").unwrap();
    fs::write(tmp.join("id_rsa"), b"k").unwrap();

    let _ = run_scan_ok(&tmp);

    let dest = tmp.join(".ctxc").join("file-map.json");
    let raw = fs::read_to_string(&dest).unwrap();

    // Nenhum nome sensível pode aparecer cru no JSON serializado.
    for s in [".env", "env.production", "server.pem", "id_rsa"] {
        assert!(
            !raw.contains(s),
            "JSON serializado vazou substring sensível '{s}': {raw}"
        );
    }

    let map: FileMap = serde_json::from_str(&raw).unwrap();
    assert_eq!(map.summary.sensitive_ignored, 4);
    assert_eq!(map.summary.considered, 1);

    let sensitives: Vec<_> = map
        .entries
        .iter()
        .filter(|e| matches!(e.ignore_category, Some(IgnoreCategory::Sensitive)))
        .collect();
    assert_eq!(sensitives.len(), 4);
    for s in &sensitives {
        assert!(s.path.is_none(), "sensível tem path: {:?}", s);
        assert!(s.parent_dir.is_none(), "sensível tem parent_dir: {:?}", s);
        assert!(s.extension.is_none(), "sensível tem extension: {:?}", s);
        assert!(s.is_sensitive_candidate);
        let red = s.path_redacted.as_ref().expect("redacted");
        assert!(red.len() >= 8 && red.len() <= 128, "tamanho redacted: {red}");
        assert!(
            red.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "redacted fora do pattern: {red}"
        );
    }

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_status_ignored_traz_ignore_category_status_considered_nao_traz() {
    let tmp = unique_tempdir("status-rule");
    fs::write(tmp.join("a.rs"), b"f").unwrap();
    fs::write(tmp.join("logo.png"), b"p").unwrap();
    fs::write(tmp.join(".env"), b"X=1").unwrap();
    fs::write(tmp.join("payload.csv"), vec![b'x'; 2 * 1024 * 1024]).unwrap();

    let _ = run_scan_ok(&tmp);
    let map = read_file_map(&tmp);

    let mut saw_ignored = false;
    let mut saw_considered = false;
    for e in &map.entries {
        match e.status {
            EntryStatus::Ignored => {
                saw_ignored = true;
                assert!(
                    e.ignore_category.is_some(),
                    "status=ignored sem ignore_category: {e:?}"
                );
            }
            EntryStatus::Considered => {
                saw_considered = true;
                assert!(
                    e.ignore_category.is_none(),
                    "status=considered com ignore_category: {e:?}"
                );
            }
        }
    }
    assert!(saw_ignored && saw_considered, "fixture deve cobrir ambos os status");

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_idempotencia_da_escrita_atomica() {
    let tmp = unique_tempdir("idempotent");
    fs::write(tmp.join("a.rs"), b"f").unwrap();
    fs::write(tmp.join("b.md"), b"b").unwrap();
    fs::write(tmp.join(".env"), b"X=1").unwrap();

    let _ = run_scan_ok(&tmp);
    let first = fs::read(tmp.join(".ctxc").join("file-map.json")).unwrap();
    let _ = run_scan_ok(&tmp);
    let second = fs::read(tmp.join(".ctxc").join("file-map.json")).unwrap();

    assert_eq!(first, second, "scan idempotente deve gerar bytes idênticos");

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_canario_top_level_extra_eh_rejeitado() {
    let json = r#"{"schema_version":"1.1.0","repo":{"root":"/tmp"},"entries":[],"summary":{"considered":0,"ignored":0,"dirs_ignored":0,"sensitive_ignored":0,"binary_ignored":0,"large_ignored":0,"total_bytes_considered":0,"total_tokens_estimate":0},"extra_field":42}"#;
    let r: Result<FileMap, _> = serde_json::from_str(json);
    assert!(r.is_err(), "campo extra no top-level deve falhar (additionalProperties:false)");
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_canario_summary_extra_eh_rejeitado() {
    let json = r#"{"schema_version":"1.1.0","repo":{"root":"/tmp"},"entries":[],"summary":{"considered":0,"ignored":0,"dirs_ignored":0,"sensitive_ignored":0,"binary_ignored":0,"large_ignored":0,"total_bytes_considered":0,"total_tokens_estimate":0,"tokens":42}}"#;
    let r: Result<FileMap, _> = serde_json::from_str(json);
    assert!(r.is_err());
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_canario_entry_extra_eh_rejeitado() {
    let json = r#"{"schema_version":"1.1.0","repo":{"root":"/tmp"},"entries":[{"kind":"file","status":"considered","is_hidden":false,"is_sensitive_candidate":false,"path":"a.rs","ranking":42}],"summary":{"considered":1,"ignored":0,"dirs_ignored":0,"sensitive_ignored":0,"binary_ignored":0,"large_ignored":0,"total_bytes_considered":0,"total_tokens_estimate":0}}"#;
    let r: Result<FileMap, _> = serde_json::from_str(json);
    assert!(r.is_err(), "campo extra em entry deve falhar");
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_canario_repo_extra_eh_rejeitado() {
    let json = r#"{"schema_version":"1.1.0","repo":{"root":"/tmp","x":1},"entries":[],"summary":{"considered":0,"ignored":0,"dirs_ignored":0,"sensitive_ignored":0,"binary_ignored":0,"large_ignored":0,"total_bytes_considered":0,"total_tokens_estimate":0}}"#;
    let r: Result<FileMap, _> = serde_json::from_str(json);
    assert!(r.is_err());
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_stdout_compativel_com_b002_b003() {
    let tmp = unique_tempdir("stdout-compat");
    fs::write(tmp.join("a.rs"), b"f").unwrap();

    let out = run_scan_ok(&tmp);
    let stdout = String::from_utf8(out.stdout).unwrap();

    assert!(stdout.starts_with("ctxc scan\n"));
    assert!(stdout.contains("files_considered: 1"));
    assert!(stdout.contains("files_ignored: 0"));
    assert!(stdout.contains("dirs_ignored: 0"));
    assert!(stdout.contains("sensitive_ignored: 0"));
    assert!(stdout.contains("binary_ignored: 0"));
    assert!(stdout.contains("large_ignored: 0"));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_compile_inspect_eval_continuam_sem_sucesso() {
    for sub in ["compile", "inspect", "eval"] {
        let out = ctxc().arg(sub).output().unwrap();
        assert!(!out.status.success(), "ctxc {sub} retornou sucesso");
    }
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_summary_bate_com_categorias_de_b003() {
    let tmp = unique_tempdir("summary-categories");
    fs::write(tmp.join("a.rs"), b"f").unwrap();
    fs::write(tmp.join("b.md"), b"b").unwrap();
    // 1 sensível + 1 binário + 1 grande
    fs::write(tmp.join(".env"), b"X=1").unwrap();
    fs::write(tmp.join("logo.png"), b"p").unwrap();
    fs::write(tmp.join("payload.csv"), vec![b'x'; 2 * 1024 * 1024]).unwrap();
    // 1 dir interno
    fs::create_dir_all(tmp.join("node_modules")).unwrap();
    fs::write(tmp.join("node_modules").join("inside.js"), b"x").unwrap();

    let _ = run_scan_ok(&tmp);
    let map = read_file_map(&tmp);

    assert_eq!(map.summary.considered, 2);
    assert_eq!(map.summary.ignored, 3);
    assert_eq!(map.summary.dirs_ignored, 1);
    assert_eq!(map.summary.sensitive_ignored, 1);
    assert_eq!(map.summary.binary_ignored, 1);
    assert_eq!(map.summary.large_ignored, 1);

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_file_map_nao_contem_campos_proibidos_pelo_escopo() {
    let tmp = unique_tempdir("forbidden-fields");
    fs::write(tmp.join("a.rs"), b"f").unwrap();
    fs::write(tmp.join(".env"), b"X=1").unwrap();

    let _ = run_scan_ok(&tmp);
    let raw = fs::read_to_string(tmp.join(".ctxc").join("file-map.json")).unwrap();

    // Campos fora do escopo do file-map (ranking/symbol/IR/compile/loss_report).
    // Nota: B-005 introduz total_tokens_estimate em summary e bloco token_estimate
    // top-level — substring "tokens"/"token_estimate" passa a ser legítima e não
    // entra mais nesta lista.
    for bad in [
        "ranking",
        "score",
        "symbols",
        "import_graph",
        "context_ir",
        "compile",
        "loss_report",
    ] {
        assert!(!raw.contains(bad), "file-map contém campo proibido '{bad}': {raw}");
    }

    cleanup(&tmp);
}

#[cfg(unix)]
#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_symlink_continua_nao_seguido_no_file_map() {
    let outside = unique_tempdir("symlink-target-b004");
    fs::write(outside.join("real_secret.json"), b"x").unwrap();

    let inside = unique_tempdir("symlink-repo-b004");
    fs::write(inside.join("normal.rs"), b"n").unwrap();
    std::os::unix::fs::symlink(&outside, inside.join("link")).unwrap();

    let _ = run_scan_ok(&inside);
    let raw = fs::read_to_string(inside.join(".ctxc").join("file-map.json")).unwrap();
    assert!(!raw.contains("real_secret.json"));

    let map: FileMap = serde_json::from_str(&raw).unwrap();
    assert!(map
        .entries
        .iter()
        .any(|e| matches!(e.ignore_category, Some(IgnoreCategory::Symlink))));

    cleanup(&inside);
    cleanup(&outside);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_falha_em_destino_nao_escrevivel_nao_emite_stdout_de_sucesso() {
    // Simula falha de escrita criando .ctxc como ARQUIVO (não diretório).
    let tmp = unique_tempdir("dest-conflict");
    fs::write(tmp.join("a.rs"), b"f").unwrap();
    // Pré-cria .ctxc como arquivo regular — `create_dir_all` deve falhar.
    fs::write(tmp.join(".ctxc"), b"not a dir").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(!out.status.success(), "scan deveria falhar quando .ctxc existe como arquivo");
    assert!(out.stdout.is_empty(), "stdout não pode mostrar resumo se a escrita falhou");
    assert!(!out.stderr.is_empty());
    // O .ctxc original (arquivo) continua intacto, sem virar dir.
    assert!(tmp.join(".ctxc").is_file());

    cleanup(&tmp);
}

// =====================================================================
// B-004.1 — Salt persistente por repo (addendum 2026-05-01 do ADR-004).
// =====================================================================

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_1_salt_eh_criado_em_primeira_execucao_com_32_bytes() {
    let tmp = unique_tempdir("salt-create");
    fs::write(tmp.join("a.rs"), b"f").unwrap();

    let _ = run_scan_ok(&tmp);

    let salt = tmp.join(".ctxc").join(".salt");
    assert!(salt.is_file(), ".ctxc/.salt não foi criado");
    let bytes = fs::read(&salt).unwrap();
    assert_eq!(bytes.len(), 32, "salt deve ter exatamente 32 bytes");

    // Sanity: salt não vaza em stdout/stderr nem aparece em entries do file-map.
    let map = read_file_map(&tmp);
    let names: Vec<_> = map.entries.iter().filter_map(|e| e.path.clone()).collect();
    assert!(!names.iter().any(|n| n.contains(".salt")));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_1_segunda_execucao_reusa_salt_e_path_redacted_eh_estavel() {
    let tmp = unique_tempdir("salt-reuse");
    fs::write(tmp.join(".env"), b"X=1").unwrap();
    fs::write(tmp.join("id_rsa"), b"k").unwrap();
    fs::write(tmp.join("a.rs"), b"f").unwrap();

    let _ = run_scan_ok(&tmp);
    let salt1 = fs::read(tmp.join(".ctxc").join(".salt")).unwrap();
    let map1 = read_file_map(&tmp);

    let _ = run_scan_ok(&tmp);
    let salt2 = fs::read(tmp.join(".ctxc").join(".salt")).unwrap();
    let map2 = read_file_map(&tmp);

    assert_eq!(salt1, salt2, "salt deve ser reusado entre execuções");

    let mut r1: Vec<_> = map1
        .entries
        .iter()
        .filter_map(|e| e.path_redacted.clone())
        .collect();
    let mut r2: Vec<_> = map2
        .entries
        .iter()
        .filter_map(|e| e.path_redacted.clone())
        .collect();
    r1.sort();
    r2.sort();
    assert!(!r1.is_empty(), "fixture deve ter sensitives gerando path_redacted");
    assert_eq!(r1, r2, "path_redacted deve ser estável com mesmo salt");

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_1_trocar_salt_muda_path_redacted() {
    let tmp = unique_tempdir("salt-swap");
    fs::write(tmp.join(".env"), b"X=1").unwrap();
    fs::write(tmp.join("server.pem"), b"k").unwrap();

    let _ = run_scan_ok(&tmp);
    let map1 = read_file_map(&tmp);
    let mut r1: Vec<_> = map1
        .entries
        .iter()
        .filter_map(|e| e.path_redacted.clone())
        .collect();
    r1.sort();
    assert!(!r1.is_empty());

    // Substitui o salt por bytes determinísticos diferentes do random original.
    fs::write(tmp.join(".ctxc").join(".salt"), [0xABu8; 32]).unwrap();

    let _ = run_scan_ok(&tmp);
    let map2 = read_file_map(&tmp);
    let mut r2: Vec<_> = map2
        .entries
        .iter()
        .filter_map(|e| e.path_redacted.clone())
        .collect();
    r2.sort();

    assert_ne!(
        r1, r2,
        "trocar salt deve mudar path_redacted (uso real do salt no hash)"
    );

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b004_1_salt_corrompido_falha_sem_emitir_file_map() {
    let tmp = unique_tempdir("salt-corrupted");
    fs::write(tmp.join("a.rs"), b"f").unwrap();
    fs::create_dir_all(tmp.join(".ctxc")).unwrap();
    // Salt com tamanho inválido (nem 0, nem 32).
    fs::write(tmp.join(".ctxc").join(".salt"), b"too-short").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(
        !out.status.success(),
        "scan deve falhar quando .ctxc/.salt está corrompido"
    );
    assert!(out.stdout.is_empty(), "sem stdout em erro de salt");
    assert!(!out.stderr.is_empty(), "stderr deve explicar falha de salt");
    assert!(
        !tmp.join(".ctxc").join("file-map.json").exists(),
        "file-map.json não pode existir após falha de salt"
    );
    // .salt corrompido permanece intocado (não tentamos sobrescrever).
    assert_eq!(
        fs::read(tmp.join(".ctxc").join(".salt")).unwrap(),
        b"too-short"
    );

    cleanup(&tmp);
}

// =====================================================================
// B-005 — Token & Byte Estimate (ADR-005, schema 1.1.0).
// =====================================================================

use ctxc::adapters::development::file_map::{TokenEstimate, BYTES_PER_TOKEN};

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b005_empty_repo_estimativa_zero() {
    let tmp = unique_tempdir("estimate-empty");
    let _ = run_scan_ok(&tmp);
    let map = read_file_map(&tmp);
    assert_eq!(map.summary.total_bytes_considered, 0);
    assert_eq!(map.summary.total_tokens_estimate, 0);
    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b005_arquivo_1_byte_eh_1_token() {
    let tmp = unique_tempdir("estimate-1byte");
    fs::write(tmp.join("a.rs"), b"x").unwrap();
    let _ = run_scan_ok(&tmp);
    let map = read_file_map(&tmp);
    assert_eq!(map.summary.total_bytes_considered, 1);
    assert_eq!(map.summary.total_tokens_estimate, 1, "ceil(1/3) = 1");
    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b005_arquivo_3_bytes_eh_1_token() {
    let tmp = unique_tempdir("estimate-3bytes");
    fs::write(tmp.join("a.rs"), b"abc").unwrap();
    let _ = run_scan_ok(&tmp);
    let map = read_file_map(&tmp);
    assert_eq!(map.summary.total_bytes_considered, 3);
    assert_eq!(map.summary.total_tokens_estimate, 1, "ceil(3/3) = 1");
    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b005_arquivo_4_bytes_eh_2_tokens() {
    let tmp = unique_tempdir("estimate-4bytes");
    fs::write(tmp.join("a.rs"), b"abcd").unwrap();
    let _ = run_scan_ok(&tmp);
    let map = read_file_map(&tmp);
    assert_eq!(map.summary.total_bytes_considered, 4);
    assert_eq!(map.summary.total_tokens_estimate, 2, "ceil(4/3) = 2");
    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b005_sensitive_nao_soma_em_estimativa() {
    let tmp = unique_tempdir("estimate-sensitive-skip");
    // 1000 bytes em sensitive — não devem entrar.
    fs::write(tmp.join(".env"), vec![b'X'; 1000]).unwrap();
    fs::write(tmp.join("server.pem"), vec![b'Y'; 1000]).unwrap();
    fs::write(tmp.join("a.rs"), b"abc").unwrap(); // 3 → 1 token
    let _ = run_scan_ok(&tmp);
    let map = read_file_map(&tmp);
    assert_eq!(map.summary.total_bytes_considered, 3, "só a.rs entra");
    assert_eq!(map.summary.total_tokens_estimate, 1);
    assert_eq!(map.summary.sensitive_ignored, 2);
    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b005_binary_e_large_nao_somam_em_estimativa() {
    let tmp = unique_tempdir("estimate-bin-large-skip");
    fs::write(tmp.join("logo.png"), vec![b'X'; 100]).unwrap();
    fs::write(tmp.join("big.csv"), vec![b'Y'; 2 * 1024 * 1024]).unwrap();
    fs::write(tmp.join("a.rs"), b"x").unwrap();
    let _ = run_scan_ok(&tmp);
    let map = read_file_map(&tmp);
    assert_eq!(map.summary.total_bytes_considered, 1, "só a.rs entra");
    assert_eq!(map.summary.total_tokens_estimate, 1);
    assert_eq!(map.summary.binary_ignored, 1);
    assert_eq!(map.summary.large_ignored, 1);
    cleanup(&tmp);
}

#[cfg(unix)]
#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b005_dirs_e_symlinks_nao_somam_em_estimativa() {
    let tmp = unique_tempdir("estimate-dirs-symlinks-skip");
    fs::write(tmp.join("a.rs"), b"x").unwrap(); // 1
    fs::create_dir_all(tmp.join("subdir")).unwrap();
    fs::write(tmp.join("subdir/b.rs"), b"yy").unwrap(); // 2
    std::os::unix::fs::symlink("/etc/hosts", tmp.join("link")).unwrap();
    let _ = run_scan_ok(&tmp);
    let map = read_file_map(&tmp);
    assert_eq!(map.summary.total_bytes_considered, 3, "a.rs(1) + b.rs(2)");
    assert_eq!(map.summary.total_tokens_estimate, 1, "ceil(3/3) = 1");
    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b005_schema_version_eh_1_1_0() {
    let tmp = unique_tempdir("estimate-schema-1-1-0");
    fs::write(tmp.join("a.rs"), b"x").unwrap();
    let _ = run_scan_ok(&tmp);
    let map = read_file_map(&tmp);
    assert_eq!(map.schema_version, "1.1.0");
    assert_eq!(map.schema_version, FILE_MAP_SCHEMA_VERSION);
    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b005_canario_summary_extra_em_1_1_0_eh_rejeitado() {
    // Summary com 8 campos válidos + extra (deny_unknown_fields).
    let json = r#"{"schema_version":"1.1.0","repo":{"root":"/tmp"},"entries":[],"summary":{"considered":0,"ignored":0,"dirs_ignored":0,"sensitive_ignored":0,"binary_ignored":0,"large_ignored":0,"total_bytes_considered":0,"total_tokens_estimate":0,"hidden_field":1}}"#;
    let r: Result<FileMap, _> = serde_json::from_str(json);
    assert!(r.is_err());
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b005_canario_token_estimate_extra_eh_rejeitado() {
    let json = r#"{"schema_version":"1.1.0","repo":{"root":"/tmp"},"entries":[],"summary":{"considered":0,"ignored":0,"dirs_ignored":0,"sensitive_ignored":0,"binary_ignored":0,"large_ignored":0,"total_bytes_considered":0,"total_tokens_estimate":0},"token_estimate":{"method":"bytes_per_token","bytes_per_token":3,"rounding":"ceil","scope":"considered_files_only","extra":1}}"#;
    let r: Result<FileMap, _> = serde_json::from_str(json);
    assert!(r.is_err());
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b005_estimativa_eh_deterministica_entre_runs() {
    let tmp = unique_tempdir("estimate-determinism");
    fs::write(tmp.join("a.rs"), b"hello").unwrap();
    fs::write(tmp.join("b.md"), b"world!").unwrap();
    let _ = run_scan_ok(&tmp);
    let map1 = read_file_map(&tmp);
    let _ = run_scan_ok(&tmp);
    let map2 = read_file_map(&tmp);
    assert_eq!(
        map1.summary.total_bytes_considered, map2.summary.total_bytes_considered
    );
    assert_eq!(
        map1.summary.total_tokens_estimate, map2.summary.total_tokens_estimate
    );
    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b005_stdout_tem_2_linhas_novas_no_fim_em_ordem_correta() {
    let tmp = unique_tempdir("estimate-stdout");
    fs::write(tmp.join("a.txt"), b"hello world").unwrap(); // 11 bytes → 4 tokens
    let out = run_scan_ok(&tmp);
    let stdout = String::from_utf8(out.stdout).unwrap();

    // Linhas anteriores B-002/B-003/B-004 preservadas:
    assert!(stdout.starts_with("ctxc scan\n"));
    assert!(stdout.contains("files_considered: 1\n"));
    assert!(stdout.contains("large_ignored: 0\n"));

    // Após large_ignored: bytes_considered, depois estimated_tokens.
    let lines: Vec<&str> = stdout.lines().collect();
    let large_idx = lines
        .iter()
        .position(|l| l.starts_with("large_ignored:"))
        .expect("linha large_ignored");
    assert_eq!(lines[large_idx + 1], "bytes_considered: 11");
    assert_eq!(lines[large_idx + 2], "estimated_tokens: 4");

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b005_token_estimate_block_emitido_com_constantes_corretas() {
    let tmp = unique_tempdir("estimate-block");
    fs::write(tmp.join("a.rs"), b"f").unwrap();
    let _ = run_scan_ok(&tmp);
    let map = read_file_map(&tmp);
    let te: &TokenEstimate = map
        .token_estimate
        .as_ref()
        .expect("token_estimate deve estar presente");
    assert_eq!(te.method, "bytes_per_token");
    assert_eq!(te.bytes_per_token, 3);
    assert_eq!(te.bytes_per_token, BYTES_PER_TOKEN);
    assert_eq!(te.rounding, "ceil");
    assert_eq!(te.scope, "considered_files_only");
    cleanup(&tmp);
}
