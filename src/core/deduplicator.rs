use std::collections::HashSet;
use super::context_item::ContextItem;
use super::loss_report::{LossEntry, LossType};
use super::context_ir::RiskLevel;

pub struct Deduplicator;

impl Deduplicator {
    /// Remove duplicações baseadas no content_hash gerado no Normalizer.
    pub fn deduplicate(items: Vec<ContextItem>) -> (Vec<ContextItem>, Vec<LossEntry>) {
        let mut seen_hashes = HashSet::new();
        let mut unique_items = Vec::new();
        let mut loss_report = Vec::new();

        for item in items {
            let hash = item.metadata.as_ref()
                .and_then(|m| m.get("content_hash"))
                .and_then(|h| h.as_str())
                .unwrap_or("").to_string();

            if seen_hashes.contains(&hash) {
                loss_report.push(LossEntry {
                    id: Some(item.id.clone()),
                    entry_type: LossType::Dropped,
                    target: item.source.as_ref().map(|s| s.to_string()),
                    reason: "duplicated content (exact hash)".to_string(),
                    risk: RiskLevel::Low,
                });
                continue;
            }

            seen_hashes.insert(hash);
            unique_items.push(item);
        }

        (unique_items, loss_report)
    }
}
