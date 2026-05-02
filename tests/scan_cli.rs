//! Testes de integração para B-002 (`ctxc scan --repo <path>`).
//!
//! Critério de aceite (ADR-002):
//! - apenas `ctxc scan --repo <repo_valido>` é novo comportamento de sucesso;
//! - repo inválido / arquivo retornam erro seguro com exit != 0;
//! - `.gitignore` respeitado; ignores internos aplicados; denylist sensível aplicada;
//! - symlink não é seguido; conteúdo externo invisível;
//! - sem efeitos colaterais (nenhum .ctxc/file-map.json criado);
//! - stdout determinístico e nunca lista paths sensíveis;
//! - compile/inspect/eval continuam sem sucesso.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn ctxc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ctxc"))
}

fn unique_tempdir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ctxc-b002-{}-{}-{}",
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

fn cleanup(p: &PathBuf) {
    let _ = fs::remove_dir_all(p);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn repo_valido_retorna_exit_zero_stdout_agregado_e_stderr_vazio() {
    let tmp = unique_tempdir("valid");
    fs::write(tmp.join("a.txt"), b"x").unwrap();
    fs::write(tmp.join("b.md"), b"y").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();

    assert!(
        out.status.success(),
        "exit != 0; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(
        out.stderr.is_empty(),
        "stderr deve ser vazio em sucesso: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("ctxc scan\n"));
    assert!(stdout.contains("repo: "));
    assert!(stdout.contains("files_considered: 2"));
    assert!(stdout.contains("files_ignored: 0"));
    assert!(stdout.contains("dirs_ignored: 0"));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn repo_inexistente_retorna_erro_seguro() {
    let nonexistent = std::env::temp_dir().join(format!(
        "ctxc-b002-NAO-EXISTE-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));

    let out = ctxc()
        .arg("scan")
        .arg("--repo")
        .arg(&nonexistent)
        .output()
        .unwrap();

    assert!(!out.status.success(), "repo inexistente não pode retornar sucesso");
    assert!(out.stdout.is_empty(), "stdout deve ficar vazio em erro");
    assert!(!out.stderr.is_empty(), "stderr deve conter mensagem de erro");
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn repo_apontando_para_arquivo_retorna_erro_seguro() {
    let tmp = unique_tempdir("file-as-repo");
    let f = tmp.join("only_a_file.txt");
    fs::write(&f, b"x").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&f).output().unwrap();

    assert!(!out.status.success(), "arquivo como --repo não pode retornar sucesso");
    assert!(out.stdout.is_empty());
    assert!(!out.stderr.is_empty());

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn gitignore_eh_respeitado() {
    let tmp = unique_tempdir("gitignore");
    fs::write(tmp.join(".gitignore"), b"ignored.txt\n").unwrap();
    fs::write(tmp.join("kept.txt"), b"k").unwrap();
    fs::write(tmp.join("ignored.txt"), b"i").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();

    // .gitignore + kept.txt = 2 considerados; ignored.txt nunca aparece.
    assert!(
        stdout.contains("files_considered: 2"),
        "stdout: {stdout}"
    );
    assert!(!stdout.contains("ignored.txt"));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn ignores_internos_minimos_aplicados() {
    let tmp = unique_tempdir("internal");
    fs::write(tmp.join("normal.txt"), b"n").unwrap();
    for d in [".git", "node_modules", "dist", "build", ".next", "target", "coverage"] {
        let dir = tmp.join(d);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("inside.txt"), b"x").unwrap();
    }

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();

    assert!(
        stdout.contains("files_considered: 1"),
        "esperava só normal.txt; stdout: {stdout}"
    );
    assert!(
        stdout.contains("dirs_ignored: 7"),
        "esperava 7 diretórios internos pruned; stdout: {stdout}"
    );
    // O nome do arquivo interno nunca aparece no stdout.
    assert!(!stdout.contains("inside.txt"));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn arquivos_sensiveis_ocultos_no_stdout_e_contados_como_ignorados() {
    let tmp = unique_tempdir("sensitive");
    fs::write(tmp.join("README.md"), b"r").unwrap();
    fs::write(tmp.join(".env"), b"X=1").unwrap();
    fs::write(tmp.join(".env.production"), b"X=1").unwrap();
    fs::write(tmp.join("server.pem"), b"k").unwrap();
    fs::write(tmp.join("server.key"), b"k").unwrap();
    fs::write(tmp.join("id_rsa"), b"k").unwrap();
    fs::write(tmp.join("dump.sql"), b"k").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();

    for sensitive in [
        ".env",
        ".env.production",
        "server.pem",
        "server.key",
        "id_rsa",
        "dump.sql",
    ] {
        assert!(
            !stdout.contains(sensitive),
            "stdout vazou path sensível '{sensitive}': {stdout}"
        );
    }

    assert!(stdout.contains("files_considered: 1"), "stdout: {stdout}");
    assert!(stdout.contains("files_ignored: 6"), "stdout: {stdout}");

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn nao_cria_artefatos_inesperados_alem_do_file_map() {
    // B-004 introduziu .ctxc/file-map.json como única side-effect esperada;
    // tudo o mais permanece intacto e nenhum artefato fora do escopo aparece.
    let tmp = unique_tempdir("no-side-effects");
    fs::write(tmp.join("a.txt"), b"a").unwrap();

    let mut before: Vec<_> = fs::read_dir(&tmp)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    before.sort();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());

    let mut after_excluding_ctxc: Vec<_> = fs::read_dir(&tmp)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| n != ".ctxc")
        .collect();
    after_excluding_ctxc.sort();
    assert_eq!(before, after_excluding_ctxc, "scan tocou em arquivos fora de .ctxc/");

    // Side-effect esperada de B-004:
    assert!(tmp.join(".ctxc").join("file-map.json").is_file());

    // Nada fora do escopo aparece no root nem em .ctxc/:
    assert!(!tmp.join("file-map.json").exists());
    assert!(!tmp.join("context.ir.json").exists());
    assert!(!tmp.join("token-report.json").exists());
    assert!(!tmp.join(".ctxc").join("context.ir.json").exists());
    assert!(!tmp.join(".ctxc").join("token-report.json").exists());
    assert!(tmp.join(".ctxc").join("symbol-map.json").is_file());

    cleanup(&tmp);
}

#[cfg(unix)]
#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn nao_segue_symlink_para_diretorio_externo() {
    let outside = unique_tempdir("symlink-target");
    fs::write(outside.join("secret_outside.txt"), b"x").unwrap();
    fs::write(outside.join("another_outside.txt"), b"y").unwrap();

    let inside = unique_tempdir("symlink-repo");
    fs::write(inside.join("normal.txt"), b"n").unwrap();
    std::os::unix::fs::symlink(&outside, inside.join("link-to-outside")).unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&inside).output().unwrap();
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();

    assert!(
        stdout.contains("files_considered: 1"),
        "deveria considerar só normal.txt; stdout: {stdout}"
    );
    assert!(!stdout.contains("secret_outside.txt"));
    assert!(!stdout.contains("another_outside.txt"));

    cleanup(&inside);
    cleanup(&outside);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn stdout_eh_deterministico_entre_execucoes() {
    let tmp = unique_tempdir("determinism");
    for n in ["c.txt", "a.txt", "b.txt", "d.md", "z.toml"] {
        fs::write(tmp.join(n), b"x").unwrap();
    }

    let a = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap().stdout;
    let b = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap().stdout;
    assert_eq!(a, b, "stdout do scan precisa ser determinístico");

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn compile_inspect_eval_continuam_sem_sucesso_em_b002() {
    for sub in ["compile", "inspect", "eval"] {
        let out = ctxc().arg(sub).output().unwrap();
        assert!(
            !out.status.success(),
            "ctxc {sub} não pode retornar sucesso em B-002"
        );
    }
}

// =====================================================================
// B-003 — testes de hardening por categorias. Não alteram contrato de B-002.
// =====================================================================

fn unique_tempdir_b003(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ctxc-b003-{}-{}-{}",
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

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b003_falso_positivo_negativo_substring_nao_marca_sensivel() {
    let tmp = unique_tempdir_b003("substring");
    let neutros = [
        "tokenizer.rs",
        "tokens.rs",
        "tokenization.go",
        "api_key_validation_test.rs",
        "private_key_parser.md",
        "key_notes.md",
        "secret_recipes.md",
        "credentials_doc.md",
    ];
    for n in neutros {
        fs::write(tmp.join(n), b"x").unwrap();
    }

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();

    assert!(
        stdout.contains(&format!("files_considered: {}", neutros.len())),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("files_ignored: 0"));
    assert!(stdout.contains("sensitive_ignored: 0"));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b003_arquivo_grande_eh_ignorado_por_metadata_sem_vazar_path() {
    let tmp = unique_tempdir_b003("large");
    let big = tmp.join("payload_data.csv");
    let buf = vec![b'x'; 2 * 1024 * 1024]; // 2 MiB
    fs::write(&big, &buf).unwrap();
    fs::write(tmp.join("README.md"), b"r").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();

    assert!(
        !stdout.contains("payload_data.csv"),
        "path do arquivo grande vazou: {stdout}"
    );
    assert!(stdout.contains("files_considered: 1"));
    assert!(stdout.contains("files_ignored: 1"));
    assert!(stdout.contains("large_ignored: 1"));
    assert!(stdout.contains("sensitive_ignored: 0"));
    assert!(stdout.contains("binary_ignored: 0"));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b003_arquivo_pequeno_no_limite_eh_considerado() {
    let tmp = unique_tempdir_b003("under-limit");
    // Exatamente 1 MiB → não passa do limite (regra é estritamente '> 1 MiB').
    let buf = vec![b'x'; 1024 * 1024];
    fs::write(tmp.join("borderline.txt"), &buf).unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();

    assert!(stdout.contains("files_considered: 1"), "stdout: {stdout}");
    assert!(stdout.contains("large_ignored: 0"));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b003_binarios_classificados_como_binary_ignored_e_ocultos() {
    let tmp = unique_tempdir_b003("binaries");
    let bins = [
        "logo.png",
        "photo.jpg",
        "archive.zip",
        "build.exe",
        "lib.so",
        "app.jar",
        "doc.pdf",
        "track.mp3",
        "font.woff2",
    ];
    for n in bins {
        fs::write(tmp.join(n), b"x").unwrap();
    }
    fs::write(tmp.join("normal.rs"), b"x").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();

    for n in bins {
        assert!(!stdout.contains(n), "binário '{n}' vazado: {stdout}");
    }
    assert!(stdout.contains("files_considered: 1"));
    assert!(stdout.contains(&format!("binary_ignored: {}", bins.len())));
    assert!(stdout.contains("sensitive_ignored: 0"));
    assert!(stdout.contains("large_ignored: 0"));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b003_dumps_e_bancos_classificados_como_sensitive_e_ocultos() {
    let tmp = unique_tempdir_b003("dumps");
    let dumps = [
        "dump.sql",
        "snapshot.dump",
        "backup.bak",
        "old.backup",
        "data.sqlite",
        "info.sqlite3",
        "store.db",
    ];
    for n in dumps {
        fs::write(tmp.join(n), b"x").unwrap();
    }
    fs::write(tmp.join("README.md"), b"r").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();

    for n in dumps {
        assert!(!stdout.contains(n), "dump/db '{n}' vazado: {stdout}");
    }
    assert!(stdout.contains("files_considered: 1"));
    assert!(stdout.contains(&format!("sensitive_ignored: {}", dumps.len())));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b003_credenciais_expandidas_classificadas_como_sensitive() {
    let tmp = unique_tempdir_b003("creds");
    let creds = [
        "cert.crt",
        "ca.cer",
        ".npmrc",
        ".pypirc",
        ".dockercfg",
        "private_key.json",
        "api_token.yaml",
    ];
    for n in creds {
        fs::write(tmp.join(n), b"x").unwrap();
    }
    fs::write(tmp.join("README.md"), b"r").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();

    for n in creds {
        assert!(!stdout.contains(n), "credencial '{n}' vazada: {stdout}");
    }
    assert!(stdout.contains("files_considered: 1"));
    assert!(stdout.contains(&format!("sensitive_ignored: {}", creds.len())));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b003_diretorios_internos_adicionais_sao_pruned() {
    let tmp = unique_tempdir_b003("dirs-extra");
    fs::write(tmp.join("normal.rs"), b"x").unwrap();
    let extras = [
        "vendor",
        "bower_components",
        ".turbo",
        ".parcel-cache",
        ".cache",
        "out",
        "tmp",
        "temp",
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
    for d in extras {
        fs::create_dir_all(tmp.join(d)).unwrap();
        fs::write(tmp.join(d).join("inside.txt"), b"x").unwrap();
    }

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();

    assert!(
        stdout.contains(&format!("dirs_ignored: {}", extras.len())),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("files_considered: 1"));
    assert!(!stdout.contains("inside.txt"));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b003_ds_store_eh_classificado_como_binary_noise() {
    let tmp = unique_tempdir_b003("dsstore");
    fs::write(tmp.join(".DS_Store"), b"x").unwrap();
    fs::write(tmp.join("normal.rs"), b"x").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();

    assert!(!stdout.contains(".DS_Store"));
    assert!(stdout.contains("files_considered: 1"));
    assert!(stdout.contains("binary_ignored: 1"));

    cleanup(&tmp);
}

#[cfg(unix)]
#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b003_symlink_continua_nao_seguido_apos_hardening() {
    let outside = unique_tempdir_b003("symlink-target-b003");
    fs::write(outside.join("real_secrets.json"), b"x").unwrap();
    let inside = unique_tempdir_b003("symlink-repo-b003");
    fs::write(inside.join("normal.rs"), b"x").unwrap();
    std::os::unix::fs::symlink(&outside, inside.join("link")).unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&inside).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("files_considered: 1"), "stdout: {stdout}");
    assert!(!stdout.contains("real_secrets.json"));

    cleanup(&inside);
    cleanup(&outside);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b003_apenas_file_map_como_side_effect_apos_hardening() {
    // B-004 introduziu .ctxc/file-map.json; B-003 mantém a garantia de que
    // nenhum outro artefato é gravado durante o scan.
    let tmp = unique_tempdir_b003("no-side-effects");
    fs::write(tmp.join("a.rs"), b"a").unwrap();
    fs::write(tmp.join(".env"), b"X=1").unwrap();
    fs::write(tmp.join("logo.png"), b"p").unwrap();

    let mut before: Vec<_> = fs::read_dir(&tmp)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    before.sort();
    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let mut after_excluding_ctxc: Vec<_> = fs::read_dir(&tmp)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| n != ".ctxc")
        .collect();
    after_excluding_ctxc.sort();
    assert_eq!(before, after_excluding_ctxc);

    assert!(tmp.join(".ctxc").join("file-map.json").is_file());
    assert!(!tmp.join("file-map.json").exists());
    assert!(!tmp.join("context.ir.json").exists());
    assert!(!tmp.join(".ctxc").join("context.ir.json").exists());

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b003_compatibilidade_contagens_b002_em_repo_misto() {
    // Repo com cada categoria + arquivos comuns; verifica linhas-chave do
    // contrato B-002 (files_considered/files_ignored/dirs_ignored) e que
    // novas contagens de B-003 são internamente consistentes.
    let tmp = unique_tempdir_b003("compat");
    // 2 arquivos normais
    fs::write(tmp.join("a.rs"), b"a").unwrap();
    fs::write(tmp.join("b.md"), b"b").unwrap();
    // 1 sensível + 1 binário + 1 grande
    fs::write(tmp.join(".env"), b"X=1").unwrap();
    fs::write(tmp.join("logo.png"), b"p").unwrap();
    fs::write(tmp.join("payload.csv"), vec![b'x'; 2 * 1024 * 1024]).unwrap();
    // 1 dir interno
    fs::create_dir_all(tmp.join("node_modules")).unwrap();
    fs::write(tmp.join("node_modules").join("inside.js"), b"x").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();

    assert!(stdout.contains("files_considered: 2"), "stdout: {stdout}");
    assert!(stdout.contains("files_ignored: 3"), "stdout: {stdout}");
    assert!(stdout.contains("dirs_ignored: 1"), "stdout: {stdout}");
    assert!(stdout.contains("sensitive_ignored: 1"));
    assert!(stdout.contains("binary_ignored: 1"));
    assert!(stdout.contains("large_ignored: 1"));

    cleanup(&tmp);
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b003_compile_inspect_eval_continuam_sem_sucesso() {
    for sub in ["compile", "inspect", "eval"] {
        let out = ctxc().arg(sub).output().unwrap();
        assert!(!out.status.success(), "ctxc {sub} retornou sucesso");
    }
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn b003_canario_binary_by_content_sem_ext_denylist_permanece_considered() {
    // Eterniza fronteira de política B-002/B-003: a classificação NUNCA inspeciona
    // bytes do arquivo. Um arquivo com conteúdo binário (null bytes / bytes não
    // imprimíveis) cuja extensão NÃO está em nenhuma denylist (nem BINARY_EXTENSIONS,
    // nem credenciais, nem dump/db) deve permanecer Considered.
    //
    // Pedido por Minerva durante eval de B-004 (PASSAR-COM-OBSERVACOES, 2026-05-01)
    // como canário para tornar a decisão arquitetural visível a futuras alterações:
    // detectar binário por conteúdo exigiria leitura de bytes, o que B-002/B-003
    // proíbem e B-004 herda.
    let tmp = unique_tempdir_b003("binary-by-content-canary");
    // .bin não está em BINARY_EXTENSIONS por design.
    let bytes: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0x7F, 0x80, b'b', b'i', b'n'];
    fs::write(tmp.join("blob.bin"), &bytes).unwrap();
    fs::write(tmp.join("README.md"), b"r").unwrap();

    let out = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();

    assert!(
        stdout.contains("files_considered: 2"),
        "blob.bin sem ext-denylist deve permanecer Considered; stdout: {stdout}"
    );
    assert!(stdout.contains("binary_ignored: 0"));
    assert!(stdout.contains("sensitive_ignored: 0"));
    assert!(stdout.contains("large_ignored: 0"));

    cleanup(&tmp);
}
