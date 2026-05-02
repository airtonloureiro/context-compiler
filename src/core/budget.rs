use super::context_item::ContextItem;
use super::loss_report::{LossEntry, LossType};
use super::context_ir::RiskLevel;
use super::scorer::ScoredItem;

pub struct BudgetAllocator;

#[derive(Debug)]
pub struct BudgetViolation {
    pub message: String,
    pub must_include_tokens: usize,
    pub budget: usize,
    pub culprit_paths: Vec<String>,
}

impl std::fmt::Display for BudgetViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for BudgetViolation {}

impl BudgetAllocator {
    /// Aloca itens dentro do budget respeitando MUST-INCLUDE como hard constraint (D-014):
    /// 1. Items com `must_include=true` entram primeiro, custe o que custar.
    /// 2. Se a soma dos must_include já estourar o budget → erro explícito (não silencioso).
    /// 3. Resto do orçamento: items remanescentes ordenados por score decrescente.
    /// 4. Items que não couberem viram entradas no loss_report (com risco High se score > 80).
    pub fn apply_budget(
        scored_items: Vec<ScoredItem>,
        budget: usize,
    ) -> Result<(Vec<ContextItem>, Vec<LossEntry>, usize), BudgetViolation> {
        // Particionar em must_include vs resto.
        let (must, rest): (Vec<ScoredItem>, Vec<ScoredItem>) =
            scored_items.into_iter().partition(|s| s.must_include);

        let must_total: usize = must.iter().map(|s| s.estimated_tokens).sum();
        if must_total > budget {
            // D-014: MUST-INCLUDE > budget → falha explícita, não truncamento silencioso.
            let culprits: Vec<String> = must
                .iter()
                .map(|s| {
                    s.item
                        .source
                        .as_ref()
                        .and_then(|src| src.get("path"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(&s.item.id)
                        .to_string()
                })
                .collect();
            return Err(BudgetViolation {
                message: format!(
                    "MUST-INCLUDE items ({} tokens) exceed budget ({}); aumente --budget ou refine a tarefa",
                    must_total, budget
                ),
                must_include_tokens: must_total,
                budget,
                culprit_paths: culprits,
            });
        }

        let mut selected: Vec<ContextItem> = Vec::new();
        let mut loss_report: Vec<LossEntry> = Vec::new();
        let mut current_tokens: usize = 0;

        // Pass 1: must_include — entram TODOS, na ordem de score (estabilidade).
        let mut must_sorted = must;
        must_sorted.sort_by(|a, b| b.score.cmp(&a.score));
        for scored in must_sorted {
            current_tokens += scored.estimated_tokens;
            selected.push(scored.item);
        }

        // Pass 2: resto, decrescente por score, encaixando até o budget.
        let mut rest_sorted = rest;
        rest_sorted.sort_by(|a, b| b.score.cmp(&a.score));
        for scored in rest_sorted {
            if current_tokens + scored.estimated_tokens <= budget {
                current_tokens += scored.estimated_tokens;
                selected.push(scored.item);
            } else {
                loss_report.push(LossEntry {
                    id: Some(scored.item.id.clone()),
                    entry_type: LossType::Dropped,
                    target: scored.item.source.as_ref().map(|s| s.to_string()),
                    reason: format!(
                        "budget exceeded (score={}, ~{} tokens, {} remaining)",
                        scored.score,
                        scored.estimated_tokens,
                        budget.saturating_sub(current_tokens)
                    ),
                    risk: if scored.score > 80 {
                        RiskLevel::High
                    } else {
                        RiskLevel::Medium
                    },
                });
            }
        }

        Ok((selected, loss_report, current_tokens))
    }
}
