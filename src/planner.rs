use crate::adapters::development::symbol_map::{SymbolMap, Symbol};
use crate::ranker::{SelectedSymbol, SelectionMode};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ContextIR {
    pub schema_version: String,
    pub task: String,
    pub selections: Vec<SelectedSymbol>,
}

pub struct SkeletonizationResult {
    pub content: String,
    pub elided_symbols: Vec<String>,
    pub skeletonized_symbols: Vec<String>,
}

pub struct Planner<'a> {
    pub symbol_map: &'a SymbolMap,
    pub selections: Vec<SelectedSymbol>,
}

impl<'a> Planner<'a> {
    pub fn new(symbol_map: &'a SymbolMap, selections: Vec<SelectedSymbol>) -> Self {
        Self { symbol_map, selections }
    }

    pub fn generate_ir(&self, task: &str) -> ContextIR {
        ContextIR {
            schema_version: "0.1.0".to_string(),
            task: task.to_string(),
            selections: self.selections.clone(),
        }
    }

    pub fn skeletonize_file(&self, file_path: &str, content: &str) -> SkeletonizationResult {
        let file_symbols = match self.symbol_map.files.iter().find(|f| f.path == file_path) {
            Some(f) => f,
            None => return SkeletonizationResult {
                content: content.to_string(),
                elided_symbols: Vec::new(),
                skeletonized_symbols: Vec::new(),
            },
        };

        let mut symbol_modes: HashMap<String, SelectionMode> = HashMap::new();
        for s in &self.selections {
            if s.file_path == file_path {
                symbol_modes.insert(s.symbol_name.clone(), s.mode);
            }
        }

        let mut skeletonized = String::new();
        let mut current_pos = 0;
        let mut elided_symbols = Vec::new();
        let mut skeletonized_symbols = Vec::new();

        let mut all_symbols: Vec<&Symbol> = file_symbols.symbols.iter().collect();
        all_symbols.sort_by_key(|s| s.range.start_byte);
        
        for symbol in all_symbols {
            if symbol.range.start_byte < current_pos {
                continue;
            }

            let mode = symbol_modes.get(&symbol.name).cloned().unwrap_or(SelectionMode::Elided);
            
            if symbol.range.start_byte > current_pos {
                skeletonized.push_str(&content[current_pos..symbol.range.start_byte]);
            }

            // B-009/B-011 Mitigation: Header Preservation & Self-Healing
            let is_skeletonizable = matches!(symbol.kind.as_str(), "function" | "class" | "method");
            let effective_mode = if !is_skeletonizable && symbol.kind != "unknown" {
                SelectionMode::Full
            } else {
                mode
            };

            match effective_mode {
                SelectionMode::Full => {
                    skeletonized.push_str(&content[symbol.range.start_byte..symbol.range.end_byte]);
                }
                SelectionMode::Skeleton => {
                    skeletonized_symbols.push(symbol.name.clone());
                    if symbol.signature_range.end_byte > content.len() || symbol.signature_range.start_byte > symbol.signature_range.end_byte {
                         skeletonized.push_str(&content[symbol.range.start_byte..symbol.range.end_byte]);
                    } else {
                        let sig_content = &content[symbol.signature_range.start_byte..symbol.signature_range.end_byte];
                        if !sig_content.contains(&symbol.name) && symbol.name != "anonymous" {
                            eprintln!("warning: self-healing: signature of '{}' in '{}' changed; fallback to full content", symbol.name, file_path);
                            skeletonized.push_str(&content[symbol.range.start_byte..symbol.range.end_byte]);
                        } else {
                            if let Some(doc) = &symbol.doc_range {
                                if doc.start_byte >= symbol.range.start_byte && doc.end_byte <= symbol.range.end_byte && doc.end_byte <= content.len() {
                                    skeletonized.push_str(&content[doc.start_byte..doc.end_byte]);
                                    if symbol.signature_range.start_byte > doc.end_byte {
                                        skeletonized.push_str(&content[doc.end_byte..symbol.signature_range.start_byte]);
                                    }
                                }
                            }
                            
                            skeletonized.push_str(sig_content);
                            
                            if symbol.range.end_byte > symbol.signature_range.end_byte {
                                skeletonized.push_str(" // [body elided]");
                            }
                        }
                    }
                }
                SelectionMode::Elided => {
                    elided_symbols.push(symbol.name.clone());
                    skeletonized.push_str(&format!("// [symbol elided: {}]", symbol.name));
                }
            }
            current_pos = symbol.range.end_byte;
        }

        if current_pos < content.len() {
            skeletonized.push_str(&content[current_pos..]);
        }

        SkeletonizationResult {
            content: skeletonized,
            elided_symbols,
            skeletonized_symbols,
        }
    }
}
