pub mod openai;
pub mod local_plain;
pub mod anthropic;

use crate::core::context_ir::ContextIR;
use crate::core::cache_layout::CacheLayoutReport;
use std::error::Error;

pub trait PromptBuilder {
    fn build_prompt(&self, ir: &ContextIR) -> Result<(String, Option<CacheLayoutReport>), Box<dyn Error>>;
}

pub struct ProviderManager;

impl ProviderManager {
    pub fn get_builder(provider_name: &str) -> Option<Box<dyn PromptBuilder>> {
        match provider_name {
            "openai" => Some(Box::new(openai::OpenAIPromptBuilder)),
            "local_plain" => Some(Box::new(local_plain::LocalPlainPromptBuilder)),
            "anthropic" => Some(Box::new(anthropic::AnthropicPromptBuilder)),
            _ => None,
        }
    }
}
