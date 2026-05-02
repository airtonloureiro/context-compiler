use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LossEntry {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub entry_type: LossType,
    pub target: Option<String>,
    pub reason: String,
    pub risk: super::context_ir::RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LossType {
    Dropped,
    Summarized,
    NotInspected,
    Truncated,
    Redacted,
    DeferredForExpansion,
}
