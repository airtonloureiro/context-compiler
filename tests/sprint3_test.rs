use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn ctxc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ctxc"))
}

fn unique_tempdir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ctxc-sprint3-{}-{}-{}",
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
fn sprint3_compile_with_skeletonization() {
    let tmp = unique_tempdir("skeleton");
    
    // 1. Create a file with a function
    let code = "/// This is a doc\npub fn hello() {\n    println!(\"world\");\n}\n";
    fs::write(tmp.join("lib.rs"), code).unwrap();
    
    // 2. Setup .ctxc with file-map and symbol-map
    let ctxc_dir = tmp.join(".ctxc");
    fs::create_dir_all(&ctxc_dir).unwrap();
    
    let file_map = serde_json::json!({
        "schema_version": "1.1.0",
        "repo": {"root": tmp.display().to_string(), "canonical_path": fs::canonicalize(&tmp).unwrap().to_string_lossy()},
        "entries": [
            {
                "kind": "file",
                "status": "considered",
                "is_hidden": false,
                "is_sensitive_candidate": false,
                "path": "lib.rs",
                "size_bytes": code.len(),
                "depth": 1
            }
        ],
        "summary": {
            "considered": 1,
            "ignored": 0,
            "dirs_ignored": 0,
            "sensitive_ignored": 0,
            "binary_ignored": 0,
            "large_ignored": 0,
            "total_bytes_considered": code.len(),
            "total_tokens_estimate": code.len() / 3
        }
    });
    fs::write(ctxc_dir.join("file-map.json"), serde_json::to_string(&file_map).unwrap()).unwrap();
    
    let symbol_map = serde_json::json!({
        "schema_version": "1.0.0",
        "files": [
            {
                "path": "lib.rs",
                "symbols": [
                    {
                        "name": "hello",
                        "kind": "function",
                        "range": {
                            "start_line": 0,
                            "start_byte": 0,
                            "end_line": 4,
                            "end_byte": code.len()
                        },
                        "signature_range": {
                            "start_line": 1,
                            "start_byte": 18,
                            "end_line": 1,
                            "end_byte": 32
                        },
                        "doc_range": {
                            "start_line": 0,
                            "start_byte": 0,
                            "end_line": 0,
                            "end_byte": 17
                        }
                    }
                ]
            }
        ]
    });
    fs::write(ctxc_dir.join("symbol-map.json"), serde_json::to_string(&symbol_map).unwrap()).unwrap();
    
    // 3. Run compile with a task that DOES NOT match "hello"
    let out = ctxc()
        .arg("compile")
        .arg("--task")
        .arg("something else")
        .arg("--repo")
        .arg(&tmp)
        .output()
        .unwrap();
    
    assert!(out.status.success());
    
    let md = fs::read_to_string(ctxc_dir.join("compiled-context.md")).unwrap();
    // Should be skeletonized
    assert!(md.contains("pub fn hello()"));
    assert!(md.contains("/// This is a doc"));
    assert!(md.contains("/* ... elided ... */"));
    assert!(!md.contains("println!"));
    
    // 4. Run compile with a task that matches "hello"
    let out = ctxc()
        .arg("compile")
        .arg("--task")
        .arg("fix hello")
        .arg("--repo")
        .arg(&tmp)
        .output()
        .unwrap();
    
    assert!(out.status.success());
    
    let md = fs::read_to_string(ctxc_dir.join("compiled-context.md")).unwrap();
    // Should be FULL
    assert!(md.contains("pub fn hello()"));
    assert!(md.contains("println!(\"world\");"));
    assert!(!md.contains("/* ... elided ... */"));
    
    // 5. Check Context IR
    let ir_path = ctxc_dir.join("context.ir.json");
    assert!(ir_path.exists());
    let ir: serde_json::Value = serde_json::from_str(&fs::read_to_string(ir_path).unwrap()).unwrap();
    assert_eq!(ir["task"], "fix hello");
    assert_eq!(ir["schema_version"], "0.1.0");

    fs::remove_dir_all(&tmp).unwrap();
}
