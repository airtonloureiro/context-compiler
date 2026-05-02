use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheLayoutReport {
    pub provider: String,
    pub static_prefix_bytes: usize,
    pub dynamic_context_bytes: usize,
    pub is_cache_friendly: bool,
}

impl CacheLayoutReport {
    pub fn new(provider: &str, static_prefix: &str, dynamic_context: &str) -> Self {
        Self {
            provider: provider.to_string(),
            static_prefix_bytes: static_prefix.len(),
            dynamic_context_bytes: dynamic_context.len(),
            is_cache_friendly: static_prefix.len() > dynamic_context.len() / 2, // Heurística simples
        }
    }
}
