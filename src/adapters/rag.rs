use crate::core::context_item::{ContextItem, ContextItemType, Sensitivity};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RetrievedChunk {
    pub id: String,
    pub content: String,
    pub source_doc: String,
    pub semantic_score: f64,
}

pub struct RagAdapter;

impl RagAdapter {
    /// Ingests retrieved RAG chunks.
    /// The semantic score will be used as a hint for the priority scorer.
    pub fn ingest(query: &str, chunks: Vec<RetrievedChunk>) -> Vec<ContextItem> {
        let mut items = Vec::new();

        // 1. User Query
        items.push(ContextItem {
            id: "rag_query".to_string(),
            item_type: ContextItemType::Message,
            role: Some("user".to_string()),
            content: query.to_string(),
            source: None,
            metadata: Some(serde_json::json!({"critical": true})),
            sensitivity: Sensitivity::Public,
        });

        // 2. Chunks
        for chunk in chunks {
            // Se o score for alto, podemos marcar como algo importante para não ser limado facilmente.
            let is_critical = chunk.semantic_score > 0.85;
            
            items.push(ContextItem {
                id: format!("rag_chunk_{}", chunk.id),
                item_type: ContextItemType::Document,
                role: None,
                content: chunk.content,
                source: Some(serde_json::json!({"document": chunk.source_doc})),
                metadata: Some(serde_json::json!({
                    "semantic_score": chunk.semantic_score,
                    "critical": is_critical
                })),
                sensitivity: Sensitivity::Public,
            });
        }

        items
    }
}
