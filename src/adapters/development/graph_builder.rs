use std::collections::HashMap;
use petgraph::graph::{NodeIndex, DiGraph};
use serde::{Deserialize, Serialize};

use super::symbol_map::{SymbolMap, Symbol};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NodeType {
    File,
    Function,
    Class,
    Interface,
    Trait,
    Impl,
    Type,
    Enum,
    Method,
    Import,
    Export,
    Unknown,
}

impl From<&str> for NodeType {
    fn from(s: &str) -> Self {
        match s {
            "function" => NodeType::Function,
            "class" => NodeType::Class,
            "interface" => NodeType::Interface,
            "trait" => NodeType::Trait,
            "impl" => NodeType::Impl,
            "type" => NodeType::Type,
            "enum" => NodeType::Enum,
            "method" => NodeType::Method,
            "import" => NodeType::Import,
            "export" => NodeType::Export,
            _ => NodeType::Unknown,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphNode {
    pub id: String,
    pub node_type: NodeType,
    pub name: String,
    pub file_path: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EdgeType {
    Contains,
    Imports,
    Calls,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub edge_type: EdgeType,
    pub weight: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KnowledgeGraph {
    pub version: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

pub struct GraphBuilder;

impl GraphBuilder {
    pub fn build(symbol_map: &SymbolMap) -> KnowledgeGraph {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        
        // Use a directed graph to optionally analyze connected components later
        let mut graph = DiGraph::<GraphNode, GraphEdge>::new();
        let mut node_indices: HashMap<String, NodeIndex> = HashMap::new();

        // 1. Add File Nodes and Their Symbols
        for file in &symbol_map.files {
            let file_id = format!("file:{}", file.path);
            
            let file_node = GraphNode {
                id: file_id.clone(),
                node_type: NodeType::File,
                name: std::path::Path::new(&file.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                file_path: file.path.clone(),
            };
            
            nodes.push(file_node.clone());
            let idx = graph.add_node(file_node);
            node_indices.insert(file_id.clone(), idx);

            for symbol in &file.symbols {
                // Ignore imports directly as primary structural nodes unless needed for edges
                if symbol.kind == "import" {
                    continue; 
                }

                let sym_id = format!("{}:{}:{}", symbol.kind, file.path, symbol.name);
                
                // Avoid duplicates (e.g. multiple impls with same name)
                if !node_indices.contains_key(&sym_id) {
                    let sym_node = GraphNode {
                        id: sym_id.clone(),
                        node_type: NodeType::from(symbol.kind.as_str()),
                        name: symbol.name.clone(),
                        file_path: file.path.clone(),
                    };
                    
                    nodes.push(sym_node.clone());
                    let s_idx = graph.add_node(sym_node);
                    node_indices.insert(sym_id.clone(), s_idx);
                }

                // Add "Contains" Edge (File -> Symbol)
                edges.push(GraphEdge {
                    source: file_id.clone(),
                    target: sym_id.clone(),
                    edge_type: EdgeType::Contains,
                    weight: 1.0,
                });
            }
        }

        // 2. Resolve Import Edges
        // A naive resolution: if an import string contains the name of another file, or we just link it.
        // For the MVP Graph-RAG, we will link `File -> File` based on import symbols.
        for file in &symbol_map.files {
            let file_id = format!("file:{}", file.path);
            
            for symbol in &file.symbols {
                if symbol.kind == "import" {
                    // Very naive heuristic to find target files based on import string.
                    // A proper implementation would resolve relative paths.
                    let import_text = symbol.name.to_lowercase();
                    
                    for target_file in &symbol_map.files {
                        if target_file.path == file.path { continue; }
                        
                        let target_stem = std::path::Path::new(&target_file.path)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase();

                        if import_text.contains(&target_stem) {
                            let target_file_id = format!("file:{}", target_file.path);
                            
                            edges.push(GraphEdge {
                                source: file_id.clone(),
                                target: target_file_id,
                                edge_type: EdgeType::Imports,
                                weight: 0.7,
                            });
                        }
                    }
                }
            }
        }

        KnowledgeGraph {
            version: "1.0.0".to_string(),
            nodes,
            edges,
        }
    }
}
