use std::path::Path;
use std::fs;
use crate::core::context_ir::ContextIR;

pub fn run_inspect(repo: &Path) -> i32 {
    let ir_path = repo.join(".ctxc").join("context.ir.json");
    if !ir_path.exists() {
        eprintln!("Nenhum contexto compilado encontrado em {:?}. Rode 'ctxc compile' ou 'ctxc dev' primeiro.", ir_path);
        return 1;
    }

    let ir_data = match fs::read_to_string(&ir_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Erro ao ler context.ir.json: {}", e);
            return 1;
        }
    };

    let ir: ContextIR = match serde_json::from_str(&ir_data) {
        Ok(ir) => ir,
        Err(e) => {
            eprintln!("Erro ao fazer parse de context.ir.json: {}", e);
            return 1;
        }
    };

    println!("=== Context Compiler Inspect ===");
    println!("Tarefa: {:?} (Goal: {})", ir.task.task_type, ir.task.goal.as_deref().unwrap_or("N/A"));
    println!("Provider Alvo: {} (Model: {})", ir.target.provider, ir.target.model.as_deref().unwrap_or("default"));
    println!("Tokens: {} antes -> {} depois (Budget: {})", ir.token_report.before, ir.token_report.after, ir.target.token_budget);
    println!("Redução de Tokens: {:.1}%", ir.token_report.reduction_ratio * 100.0);
    println!("Risco de Perda: {:?}", ir.risk.level);
    println!("Itens no Contexto Final: {}", ir.items.len());
    println!("Itens Descartados/Perdidos: {}", ir.loss_report.len());

    0
}
