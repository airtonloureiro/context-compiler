use crate::adapters::development::symbol_map::{SymbolMap};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    Full,
    Skeleton,
    Elided,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedSymbol {
    pub file_path: String,
    pub symbol_name: String,
    pub mode: SelectionMode,
    pub reason: String,
    pub score: f32,
}

pub struct Ranker<'a> {
    pub symbol_map: &'a SymbolMap,
    pub task: &'a str,
    pub log: Option<&'a str>,
}

impl<'a> Ranker<'a> {
    pub fn new(symbol_map: &'a SymbolMap, task: &'a str, log: Option<&'a str>) -> Self {
        Self { symbol_map, task, log }
    }

    pub fn rank(&self) -> Vec<SelectedSymbol> {
        let mut selections = Vec::new();
        let task_lower = self.task.to_lowercase();
        let log_lower = self.log.map(|l| l.to_lowercase());
        
        for file in &self.symbol_map.files {
            let file_path_lower = file.path.to_lowercase();
            let file_name = file_path_lower.split('/').next_back().unwrap_or("");
            
            // Heurística de match de caminho
            let path_score = if task_lower.contains(&file_path_lower) || task_lower.contains(file_name) {
                1.0
            } else if let Some(ref l) = log_lower {
                if l.contains(&file_path_lower) || l.contains(file_name) {
                    0.8
                } else {
                    0.0
                }
            } else {
                0.0
            };

            for symbol in &file.symbols {
                let symbol_name_lower = symbol.name.to_lowercase();
                
                let mut symbol_score = path_score;
                let mut reason = if path_score > 0.0 { "path_match".to_string() } else { "none".to_string() };

                if task_lower.contains(&symbol_name_lower) {
                    symbol_score += 1.0;
                    reason = "task_symbol_match".to_string();
                } else if let Some(ref l) = log_lower {
                    if l.contains(&symbol_name_lower) {
                        symbol_score += 0.5;
                        reason = "log_symbol_match".to_string();
                    }
                }

                let mode = if symbol_score >= 1.0 {
                    SelectionMode::Full
                } else if symbol_score > 0.0 {
                    SelectionMode::Skeleton
                } else {
                    SelectionMode::Elided
                };

                selections.push(SelectedSymbol {
                    file_path: file.path.clone(),
                    symbol_name: symbol.name.clone(),
                    mode,
                    reason,
                    score: symbol_score,
                });
            }
        }
        selections
    }
}
