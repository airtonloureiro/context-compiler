use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn ctxc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ctxc"))
}

fn unique_tempdir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ctxc-adr009-{}-{}-{}",
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
fn adr009_skeletonization_new_api() {
    let tmp_raw = unique_tempdir("adr009");
    let tmp = fs::canonicalize(&tmp_raw).unwrap();
    
    // Create a file > 2000 bytes to trigger skeletonization logic
    let mut code = String::from("fn skeleton_target() {\n    println!(\"skeleton body\");\n}\n\n");
    for i in 0..100 {
        code.push_str(&format!("fn padding_func_{}() {{ println!(\"padding\"); }}\n", i));
    }
    fs::write(tmp.join("lib.rs"), &code).unwrap();
    
    // Run scan to generate file-map
    let out_scan = ctxc().arg("scan").arg("--repo").arg(&tmp).output().unwrap();
    assert!(out_scan.status.success());
    
    // Run dev compile with task not matching lib.rs directly, should trigger skeletonization
    let out = ctxc()
        .arg("dev")
        .arg("--task")
        .arg("modify_code")
        .arg("--goal")
        .arg("fix other_bug")
        .arg("--repo")
        .arg(&tmp)
        .arg("--budget")
        .arg("2000")
        .output()
        .unwrap();
    
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    
    let md = fs::read_to_string(tmp.join(".ctxc").join("compiled-context.md")).unwrap();
    
    // Check IR
    let ir_path = tmp.join(".ctxc").join("context.ir.json");
    assert!(ir_path.exists());

    // Check for the new AST SKELETON marker
    assert!(md.contains("[COMPRESSED: AST SKELETON] lib.rs"));
    assert!(md.contains("skeleton_target"));
    assert!(md.contains("/* ... */"));
}
