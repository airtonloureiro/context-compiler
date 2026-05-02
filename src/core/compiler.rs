use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};

use super::context_item::ContextItem;
use super::context_ir::{ContextIR, Task, Target, Risk, RiskLevel};
use super::token_report::TokenReport;
use super::normalizer::Normalizer;
use super::segmenter::Segmenter;
use super::deduplicator::Deduplicator;
use super::scorer::Scorer;
use super::budget::BudgetAllocator;
use super::cache_layout::CacheLayoutReport;
use crate::providers::ProviderManager;

#[derive(Debug, Deserialize, Serialize)]
pub struct GenericInput {
    pub task: Task,
    pub target: Target,
    pub context_items: Vec<ContextItem>,
}

pub fn compile_generic(input_path: &Path, target_provider: &str, budget: usize, out_dir: Option<&Path>) -> Result<(), Box<dyn std::error::Error>> {
    let input_data = fs::read_to_string(input_path)?;
    let generic_input: GenericInput = serde_json::from_str(&input_data)?;

    let mut ir = ContextIR {
        ir_version: "ctx_ir_v0".to_string(),
        task: generic_input.task.clone(),
        target: generic_input.target.clone(),
        items: vec![],
        segments: vec![],
        compiled_facts: vec![],
        evidence_pointers: vec![],
        selected_context: vec![],
        loss_report: vec![],
        expansion_candidates: vec![],
        unknowns: vec![],
        risk: Risk { level: RiskLevel::Low, reasons: vec![] },
        token_report: TokenReport::default(),
    };

    ir.target.provider = target_provider.to_string();
    ir.target.token_budget = budget;

    let mut tokens_before = 0;
    
    // 1. Normalizer
    let mut normalized_items = Vec::new();
    for item in generic_input.context_items.clone() {
        tokens_before += item.content.len() / 4;
        normalized_items.push(Normalizer::normalize(item));
    }

    // 2. Segmenter
    let segmented_items = Segmenter::segment(normalized_items);

    // 3. Deduplicator
    let (unique_items, dedupe_loss) = Deduplicator::deduplicate(segmented_items);
    ir.loss_report.extend(dedupe_loss);

    // 4. Priority Scorer
    let scored_items = Scorer::score(unique_items, &generic_input.task);

    // 5. Budget Allocator
    // Reservar tokens para envelope do prompt (system, GOAL, ITEM headers, tags).
    // Medido empiricamente em 15 repos:
    //   - fixed overhead (system + GOAL + tags): ~200 tokens
    //   - per-item overhead (headers `--- ITEM [id] ---`): ~10-25 tokens × n_items
    // Reserve 350 + ceiling 10% no hard-fail cobrem a variância do estimador chars/4
    // sem forçar items_budget pequeno demais (que vira MUST-INCLUDE > budget em monorepos).
    // Bug fix (D-014): budget é o teto do PROMPT total, não dos items.
    const ENVELOPE_RESERVE: usize = 350;
    let items_budget = budget.saturating_sub(ENVELOPE_RESERVE);
    if items_budget == 0 {
        return Err(format!(
            "budget {} too small: needs at least {} tokens for prompt envelope (system + headers)",
            budget,
            ENVELOPE_RESERVE + 1
        ).into());
    }
    let (selected_items, budget_loss, _tokens_after) = BudgetAllocator::apply_budget(scored_items, items_budget)
        .map_err(|violation| -> Box<dyn std::error::Error> {
            // D-014: MUST-INCLUDE > budget → falha explícita ANTES de tocar disco.
            eprintln!("ctxc: {}", violation.message);
            if !violation.culprit_paths.is_empty() {
                eprintln!("ctxc: MUST-INCLUDE files ({} tokens):", violation.must_include_tokens);
                for p in &violation.culprit_paths {
                    eprintln!("ctxc:   - {}", p);
                }
            }
            eprintln!("ctxc: aumente --budget acima de {}", violation.must_include_tokens + 350);
            Box::new(violation)
        })?;
    ir.loss_report.extend(budget_loss);
    ir.items = selected_items;
    
    // Extrair os paths dos arquivos selecionados para o selected_context
    let selected_files: Vec<String> = ir.items.iter()
        .filter_map(|item| item.source.as_ref().and_then(|s| s.get("path")).and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();
    ir.selected_context = selected_files.clone();
    
    // Heurística de unknowns (exigência D-02)
    // Procurar indicativos de falta de contexto
    let mut unknowns = Vec::new();
    
    // Verifica se temos um stack_profile
    if !generic_input.context_items.iter().any(|item| item.id == "stack_profile") {
        unknowns.push("Stack profile de tecnologias não fornecido ou não detectado.".to_string());
    }
    
    // Se a task de debug não tiver logs de erro fornecidos
    if ir.task.task_type == crate::core::context_ir::TaskType::DebugError && !generic_input.context_items.iter().any(|item| matches!(item.item_type, super::context_item::ContextItemType::Log)) {
        unknowns.push("Tarefa é de debug, mas nenhum log de erro foi fornecido.".to_string());
    }
    
    // Verificar se existe um repo scan
    if !generic_input.context_items.iter().any(|item| item.id == "repo_scan_report") {
        unknowns.push("Relatório de varredura do repositório ausente. O contexto pode estar incompleto.".to_string());
    }
    
    ir.unknowns = unknowns;
    
    // Determine overall risk
    if ir.loss_report.iter().any(|l| matches!(l.risk, RiskLevel::High)) {
        ir.risk.level = RiskLevel::High;
        ir.risk.reasons.push("Critical facts might have been dropped due to budget".to_string());
    } else if !ir.loss_report.is_empty() {
        ir.risk.level = RiskLevel::Medium;
        ir.risk.reasons.push("Some content was dropped".to_string());
    }

    // Create artifacts directory
    let default_out = Path::new(".ctxc");
    let output_dir = out_dir.unwrap_or(default_out);
    if !output_dir.exists() {
        fs::create_dir_all(output_dir)?;
    }

    // Gerar selected-files.json
    let selected_json = serde_json::to_string_pretty(&selected_files)?;
    fs::write(output_dir.join("selected-files.json"), selected_json)?;

    // 6. Provider Prompt Builder
    let prompt_builder = ProviderManager::get_builder(target_provider)
        .unwrap_or_else(|| ProviderManager::get_builder("local_plain").unwrap());

    let (compiled_md, cache_report) = prompt_builder.build_prompt(&ir)?;

    // Bug fix: medir tokens REAIS do prompt final (não só dos items selecionados).
    // Estimativa usando tokenizer real (cl100k_base).
    let real_prompt_tokens = crate::core::tokenizer::Tokenizer::count_tokens(&compiled_md);
    ir.token_report = TokenReport::new(tokens_before, real_prompt_tokens);

    // Hard-fail: budget é contrato (`/docs/00 §Princípio 7`).
    // Tolerância de 25% acomoda a inflação de tokens causada pelo payload JSON escapar newlines (\n) e aspas (\").
    // O provedor retorna um JSON válido, o que significa que o Tokenizer (cl100k_base) conta tokens da string escapada.
    let budget_ceiling = budget + budget / 4; // budget * 1.25
    if real_prompt_tokens > budget_ceiling {
        eprintln!(
            "ctxc: budget violation — real prompt={} tokens, budget={} (ceiling={} = +10%).",
            real_prompt_tokens, budget, budget_ceiling
        );
        eprintln!("ctxc: aumente --budget, reduza o repo (--repo subdir), ou reporte como bug.");
        // Mesmo falhando, persistimos os artefatos para auditoria.
        fs::write(output_dir.join("compiled-context.md"), &compiled_md)?;
        let token_json = serde_json::to_string_pretty(&ir.token_report)?;
        fs::write(output_dir.join("token-report.json"), token_json)?;
        return Err(format!(
            "budget exceeded: real prompt={} tokens > ceiling={} (budget={} +5%)",
            real_prompt_tokens, budget_ceiling, budget
        ).into());
    }

    fs::write(output_dir.join("compiled-context.md"), compiled_md)?;
    
    if let Some(cr) = cache_report {
        let cr_json = serde_json::to_string_pretty(&cr)?;
        fs::write(output_dir.join("cache-layout-report.json"), cr_json)?;
    }

    // Write Context IR
    let ir_json = serde_json::to_string_pretty(&ir)?;
    fs::write(output_dir.join("context.ir.json"), ir_json)?;

    // Write Token Report
    let token_json = serde_json::to_string_pretty(&ir.token_report)?;
    fs::write(output_dir.join("token-report.json"), token_json)?;

    // Write Loss Report
    let loss_json = serde_json::to_string_pretty(&ir.loss_report)?;
    fs::write(output_dir.join("loss-report.json"), loss_json)?;
    
    let mut loss_md = String::from("# Loss Report\n\n");
    if ir.loss_report.is_empty() {
        loss_md.push_str("No loss. All requested context was preserved.\n");
    } else {
        for entry in &ir.loss_report {
            loss_md.push_str(&format!("- **{:?}**: {} (Risk: {:?})\n", entry.entry_type, entry.reason, entry.risk));
        }
    }
    fs::write(output_dir.join("loss-report.md"), loss_md)?;

    println!("Context compiled successfully. Artifacts written to .ctxc/");
    Ok(())
}

