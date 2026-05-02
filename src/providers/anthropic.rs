use crate::core::context_ir::ContextIR;
use crate::core::cache_layout::CacheLayoutReport;
use super::PromptBuilder;
use std::error::Error;

pub struct AnthropicPromptBuilder;

impl PromptBuilder for AnthropicPromptBuilder {
    fn build_prompt(&self, ir: &ContextIR) -> Result<(String, Option<CacheLayoutReport>), Box<dyn Error>> {
        // Anthropic utiliza o campo "system" para prefixos cacheados e "messages" para a interação dinâmica.
        // Simularemos o dump deste payload.
        
        let mut system_text = String::new();
        system_text.push_str("Você é um assistente de IA. Analise o contexto a seguir para realizar a tarefa solicitada.\n\n");
        system_text.push_str("[STATIC INSTRUCTIONS]\nUse o Context IR para guiar sua resposta. Priorize responder com base nas evidências fornecidas.\n");
        
        let mut user_message = String::new();
        user_message.push_str(&format!("<CTXC v=\"0\" task=\"{:?}\" budget=\"{}\">\n\n", ir.task.task_type, ir.target.token_budget));
        
        user_message.push_str("GOAL:\n");
        user_message.push_str(&ir.task.goal.as_deref().unwrap_or("N/A"));
        user_message.push_str("\n\n");

        if !ir.evidence_pointers.is_empty() {
            user_message.push_str("EVIDENCE POINTERS:\n");
            for ev in &ir.evidence_pointers {
                let quote = ev.quote.as_deref().unwrap_or("");
                let line_info = match (ev.line_start, ev.line_end) {
                    (Some(s), Some(e)) if s != e => format!(" lines {}-{}", s, e),
                    (Some(s), _) => format!(" line {}", s),
                    _ => "".to_string(),
                };
                user_message.push_str(&format!("- [{}] {}{}: \"{}\"\n", ev.id, ev.path, line_info, quote));
            }
            user_message.push_str("\n");
        }

        // NOTE: loss_report é artefato auditável separado em `.ctxc/loss-report.{json,md}`.
        // Não vai no prompt — o LLM não precisa saber o que NÃO recebeu.
        // Bug fix: inlining do loss-report consumia ~53% do budget na média de 15 repos.

        user_message.push_str("SELECTED CONTEXT:\n");
        for item in &ir.items {
            user_message.push_str(&format!("--- ITEM [{}] ---\n", item.id));
            user_message.push_str(&item.content);
            user_message.push_str("\n\n");
        }
        
        user_message.push_str("</CTXC>\n");
        
        if let Some(req) = &ir.task.user_request {
            user_message.push_str("\nUSER REQUEST:\n");
            user_message.push_str(req);
            user_message.push_str("\n");
        }

        // Mock Anthropic payload com Prompt Caching block no system prompt
        let payload = serde_json::json!({
            "system": [
                {
                    "type": "text",
                    "text": system_text,
                    "cache_control": { "type": "ephemeral" }
                }
            ],
            "messages": [
                {
                    "role": "user",
                    "content": user_message
                }
            ]
        });

        let cache_report = CacheLayoutReport::new("anthropic", &system_text, &user_message);

        Ok((serde_json::to_string_pretty(&payload)?, Some(cache_report)))
    }
}
