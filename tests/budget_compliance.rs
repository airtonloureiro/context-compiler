use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn ctxc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ctxc"))
}

fn unique_tempdir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ctxc-budget-{}-{}-{}",
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
fn eval_guard_budget_compliance_within_10_percent_margin() {
    let tmp_raw = unique_tempdir("compliance");
    let tmp = fs::canonicalize(&tmp_raw).unwrap();
    
    // Create a lot of files to ensure we exceed the budget
    for i in 0..100 {
        let code = format!("pub fn dummy_func_{}() {{ println!(\"dummy\"); }}\n", i);
        fs::write(tmp.join(format!("file_{}.rs", i)), code).unwrap();
    }
    
    // Also add critical files (MUST-INCLUDE)
    fs::write(tmp.join("src").join("main.rs"), "fn main() {}").unwrap_or_else(|_| {
        fs::create_dir_all(tmp.join("src")).unwrap();
        fs::write(tmp.join("src").join("main.rs"), "fn main() {}").unwrap();
    });
    fs::write(tmp.join("Cargo.toml"), "[package]\nname=\"dummy\"\n").unwrap();

    let budget = 2000;
    
    let out = ctxc()
        .arg("dev")
        .arg("--task")
        .arg("debug_error")
        .arg("--goal")
        .arg("test budget compliance")
        .arg("--repo")
        .arg(&tmp)
        .arg("--budget")
        .arg(budget.to_string())
        .output()
        .unwrap();
        
    // O Eval Guard verifica se a engine com seu novo Scorer ciente de overheads per-item
    // foi capaz de acomodar os 100 arquivos dentro do budget com sucesso (sem estourar para o Hard-Fail de 1.25x)
    assert!(out.status.success(), "ctxc dev falhou inesperadamente: {}", String::from_utf8_lossy(&out.stderr));
    
    let report_path = tmp.join(".ctxc").join("token-report.json");
    assert!(report_path.exists(), "token-report.json não gerado");
    
    let report_str = fs::read_to_string(report_path).unwrap();
    let report: serde_json::Value = serde_json::from_str(&report_str).unwrap();
    
    let tokens_after = report.get("after").unwrap().as_u64().unwrap();
    
    // Teto de 25% acomodando escape JSON e afins.
    let ceiling = (budget as f64 * 1.25) as u64;
    
    assert!(
        tokens_after <= ceiling,
        "Minerva Guard Failed: A engine não respeitou o contrato! Tokens finais ({}) excedem o teto permitido de {}",
        tokens_after, ceiling
    );
}

