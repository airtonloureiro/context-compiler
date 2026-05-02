use std::path::Path;
use std::fs;
use crate::core::context_ir::ContextIR;
use crate::core::loss_report::LossType;

pub fn run_explain(repo: &Path, target: Option<&str>) -> i32 {
    let ir_path = repo.join(".ctxc").join("context.ir.json");
    if !ir_path.exists() {
        eprintln!("Nenhum contexto compilado encontrado em {:?}. Rode 'ctxc compile' primeiro.", ir_path);
        return 1;
    }

    let ir_data = fs::read_to_string(&ir_path).unwrap();
    let ir: ContextIR = serde_json::from_str(&ir_data).unwrap();

    println!("=== Context Compiler Explain ===");

    if let Some(t) = target {
        println!("Buscando explicações para o alvo: '{}'\n", t);
        
        let mut found = false;
        
        // Procurar nos itens mantidos
        for item in &ir.items {
            let item_target = item.source.as_ref().map(|s| s.to_string()).unwrap_or_default();
            if item.id.contains(t) || item_target.contains(t) {
                println!("✅ MANTIDO:");
                println!("  ID: {}", item.id);
                println!("  Tipo: {:?}", item.item_type);
                println!("  Origem: {}", item_target);
                found = true;
            }
        }

        // Procurar no loss report
        for loss in &ir.loss_report {
            let loss_target = loss.target.as_deref().unwrap_or("");
            let loss_id = loss.id.as_deref().unwrap_or("");
            if loss_id.contains(t) || loss_target.contains(t) {
                println!("❌ PERDA ({:?}):", loss.entry_type);
                println!("  ID/Target: {} / {}", loss_id, loss_target);
                println!("  Motivo: {}", loss.reason);
                println!("  Risco: {:?}", loss.risk);
                found = true;
            }
        }

        if !found {
            println!("Alvo '{}' não encontrado no contexto compilado nem no relatório de perdas.", t);
        }
    } else {
        println!("Risco Geral: {:?}", ir.risk.level);
        for reason in &ir.risk.reasons {
            println!(" - {}", reason);
        }
        println!("\nResumo do Loss Report ({} itens):", ir.loss_report.len());
        let mut dropped = 0;
        let mut truncated = 0;
        let mut other = 0;
        for loss in &ir.loss_report {
            match loss.entry_type {
                LossType::Dropped => dropped += 1,
                LossType::Truncated => truncated += 1,
                _ => other += 1,
            }
        }
        println!(" - Dropped: {}", dropped);
        println!(" - Truncated: {}", truncated);
        println!(" - Outros: {}", other);
        println!("\nPara ver detalhes de um arquivo ou ID específico, use: ctxc explain --target <nome>");
    }

    0
}
