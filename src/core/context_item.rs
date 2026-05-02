use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: ContextItemType,
    pub role: Option<String>,
    pub content: String,
    pub source: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub sensitivity: Sensitivity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextItemType {
    Message,
    SystemInstruction,
    Document,
    File,
    Code,
    Log,
    ToolOutput,
    StructuredData,
    Memory,
    Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    #[default]
    Public,
    Internal,
    Confidential,
    Secret,
}
