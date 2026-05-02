use serde::{Deserialize, Serialize};
use super::context_item::ContextItem;
use super::loss_report::LossEntry;
use super::token_report::TokenReport;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextIR {
    pub ir_version: String,
    pub task: Task,
    pub target: Target,
    pub items: Vec<ContextItem>,
    pub segments: Vec<serde_json::Value>,
    pub compiled_facts: Vec<serde_json::Value>,
    #[serde(default)]
    pub evidence_pointers: Vec<EvidencePointer>,
    pub selected_context: Vec<String>,
    pub loss_report: Vec<LossEntry>,
    pub expansion_candidates: Vec<serde_json::Value>,
    #[serde(default)]
    pub unknowns: Vec<String>,
    pub risk: Risk,
    pub token_report: TokenReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidencePointer {
    pub id: String,
    pub source_item_id: String,
    pub source_segment_id: Option<String>,
    pub path: String,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
    pub byte_start: Option<usize>,
    pub byte_end: Option<usize>,
    pub symbol_name: Option<String>,
    pub quote: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    #[serde(rename = "type")]
    pub task_type: TaskType,
    pub goal: Option<String>,
    pub user_request: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    DebugError,
    ModifyCode,
    ExplainCode,
    ArchitectureReview,
    Development,
    GatewayProxy,
    Generic(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub provider: String,
    pub model: Option<String>,
    pub token_budget: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Risk {
    pub level: RiskLevel,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}
