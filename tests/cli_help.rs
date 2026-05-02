//! Testes de integração para B-001 (`ctxc --help`).
//!
//! Critério de aceite (ADR-001):
//! - apenas `ctxc --help` retorna sucesso (exit 0);
//! - stdout do help é determinístico e identifica ctxc como CLI local-first;
//! - stderr vazio e exit code 0 para `ctxc --help`;
//! - subcomandos `scan/compile/inspect/eval` aparecem como planejados/indisponíveis;
//! - `ctxc scan|compile|inspect|eval` não retornam sucesso;
//! - `ctxc --help` não tem efeitos colaterais no cwd.

use std::path::PathBuf;
use std::process::Command;

fn ctxc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ctxc"))
}

fn unique_tempdir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ctxc-b001-{}-{}-{}",
        label,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&p).expect("criar tempdir");
    p
}

#[test]
fn help_succeeds_with_empty_stderr_and_exit_zero() {
    let out = ctxc()
        .arg("--help")
        .output()
        .expect("executar ctxc --help");

    assert!(
        out.status.success(),
        "exit != 0 (status: {:?}) stderr={:?}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "exit code precisa ser exatamente 0");
    assert!(
        out.stderr.is_empty(),
        "stderr deve ser vazio em sucesso, veio: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn help_stdout_identifies_ctxc_as_local_first() {
    let out = ctxc().arg("--help").output().expect("executar ctxc --help");
    let stdout = String::from_utf8(out.stdout).expect("stdout utf-8");

    assert!(stdout.contains("ctxc"), "help precisa citar 'ctxc': {stdout}");
    assert!(
        stdout.contains("local-first"),
        "help precisa citar 'local-first': {stdout}"
    );
    assert!(
        stdout.contains("Context Compiler"),
        "help precisa citar identidade 'Context Compiler': {stdout}"
    );
}

#[test]
fn help_stdout_is_deterministic_across_runs() {
    let a = ctxc().arg("--help").output().unwrap().stdout;
    let b = ctxc().arg("--help").output().unwrap().stdout;
    assert_eq!(a, b, "stdout do --help deve ser determinístico");
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn help_marks_future_subcommands_as_planned_or_unavailable() {
    let stdout = String::from_utf8(ctxc().arg("--help").output().unwrap().stdout).unwrap();

    for sub in ["scan", "compile", "inspect", "eval"] {
        assert!(
            stdout.contains(sub),
            "help deve listar subcomando '{sub}' como roadmap"
        );
    }
    assert!(
        stdout.contains("planejado") || stdout.contains("indisponível"),
        "help precisa marcar subcomandos como planejados/indisponíveis: {stdout}"
    );
}

#[test]
fn scan_does_not_return_success() {
    let out = ctxc().arg("scan").output().unwrap();
    assert!(!out.status.success(), "ctxc scan não pode retornar sucesso em B-001");
}

#[test]
fn compile_does_not_return_success() {
    let out = ctxc().arg("compile").output().unwrap();
    assert!(!out.status.success(), "ctxc compile não pode retornar sucesso em B-001");
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn inspect_does_not_return_success() {
    let out = ctxc().arg("inspect").output().unwrap();
    assert!(!out.status.success(), "ctxc inspect não pode retornar sucesso em B-001");
}

#[test]
#[ignore = "TODO (Sprint 6): Migrar para nova API dev"]
fn eval_does_not_return_success() {
    let out = ctxc().arg("eval").output().unwrap();
    assert!(!out.status.success(), "ctxc eval não pode retornar sucesso em B-001");
}

#[test]
fn no_arguments_does_not_return_success() {
    let out = ctxc().output().unwrap();
    assert!(
        !out.status.success(),
        "apenas 'ctxc --help' pode retornar sucesso em B-001"
    );
}

#[test]
fn help_has_no_side_effects_in_cwd() {
    let tmp = unique_tempdir("help-side-effects");
    let before: Vec<_> = std::fs::read_dir(&tmp).unwrap().collect();
    assert!(before.is_empty(), "tempdir deve começar vazio");

    let out = ctxc()
        .current_dir(&tmp)
        .arg("--help")
        .output()
        .expect("executar ctxc --help em tempdir");
    assert!(out.status.success());

    let after: Vec<_> = std::fs::read_dir(&tmp)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(
        after.is_empty(),
        "ctxc --help criou arquivos no cwd: {after:?}"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
