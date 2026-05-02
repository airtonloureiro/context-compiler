use super::context_item::ContextItem;
use sha2::{Digest, Sha256};

pub struct Normalizer;

impl Normalizer {
    pub fn normalize(mut item: ContextItem) -> ContextItem {
        item.content = item.content.replace("\r\n", "\n").trim().to_string();
        
        let hash = Self::compute_hash(&item.content);
        
        let mut metadata = item.metadata.unwrap_or_else(|| serde_json::json!({}));
        if let serde_json::Value::Object(ref mut map) = metadata {
            map.insert("content_hash".to_string(), serde_json::json!(hash));
        }
        item.metadata = Some(metadata);
        
        item
    }

    fn compute_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}
