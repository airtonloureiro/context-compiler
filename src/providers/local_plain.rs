use crate::core::context_ir::ContextIR;
use crate::core::cache_layout::CacheLayoutReport;
use super::PromptBuilder;
use std::error::Error;

pub struct LocalPlainPromptBuilder;

impl PromptBuilder for LocalPlainPromptBuilder {
    fn build_prompt(&self, ir: &ContextIR) -> Result<(String, Option<CacheLayoutReport>), Box<dyn Error>> {
        let mut prompt = String::new();
        
        let mut system_text = String::new();
        system_text.push_str("<SYSTEM>\nVocê é um assistente de IA. Analise o contexto para resolver a tarefa.\n</SYSTEM>\n\n");
        
        let mut context_text = String::new();
        context_text.push_str("<CONTEXT>\n");
        for item in &ir.items {
            context_text.push_str(&format!("--- [{}] ---\n", item.id));
            context_text.push_str(&item.content);
            context_text.push_str("\n");
        }
        context_text.push_str("</CONTEXT>\n\n");

        context_text.push_str("<TASK>\n");
        context_text.push_str(&format!("Goal: {}\n", ir.task.goal.as_deref().unwrap_or("N/A")));
        if let Some(req) = &ir.task.user_request {
            context_text.push_str(&format!("Request: {}\n", req));
        }
        context_text.push_str("</TASK>\n");

        prompt.push_str(&system_text);
        prompt.push_str(&context_text);

        let cache_report = CacheLayoutReport::new("local_plain", &system_text, &context_text);

        Ok((prompt, Some(cache_report)))
    }
}
