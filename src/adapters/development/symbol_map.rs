use std::path::Path;
use serde::{Deserialize, Serialize};
use tree_sitter::{Parser, Node};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub start_line: usize,
    pub start_byte: usize,
    pub end_line: usize,
    pub end_byte: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub range: Range,
    pub signature_range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_range: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsing_error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct FileSymbols {
    pub path: String,
    pub symbols: Vec<Symbol>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsing_error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SymbolMap {
    pub schema_version: String,
    pub files: Vec<FileSymbols>,
}

pub const SYMBOL_MAP_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug)]
pub enum SymbolMapError {
    Io(std::io::Error),
    Serialize(serde_json::Error),
}

impl std::fmt::Display for SymbolMapError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SymbolMapError::Io(e) => write!(f, "io: {e}"),
            SymbolMapError::Serialize(e) => write!(f, "serialize: {e}"),
        }
    }
}

impl std::error::Error for SymbolMapError {}

impl From<std::io::Error> for SymbolMapError {
    fn from(e: std::io::Error) -> Self {
        SymbolMapError::Io(e)
    }
}

impl From<serde_json::Error> for SymbolMapError {
    fn from(e: serde_json::Error) -> Self {
        SymbolMapError::Serialize(e)
    }
}

impl SymbolMap {
    pub fn load(path: &Path) -> Result<Self, SymbolMapError> {
        let bytes = std::fs::read(path)?;
        let map: Self = serde_json::from_slice(&bytes)?;
        if map.schema_version != SYMBOL_MAP_SCHEMA_VERSION {
            return Err(SymbolMapError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "symbol-map schema_version={}, expected exactly {}; regenerate with ctxc scan",
                    map.schema_version, SYMBOL_MAP_SCHEMA_VERSION
                ),
            )));
        }
        Ok(map)
    }
}

pub fn extract_symbols(path: &Path, content: &str) -> FileSymbols {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let language = match extension {
        "rs" => Some(tree_sitter_rust::language()),
        "ts" | "tsx" => Some(tree_sitter_typescript::language_typescript()),
        "py" => Some(tree_sitter_python::language()),
        _ => None,
    };

    let mut symbols = Vec::new();
    let mut file_error = None;

    if let Some(lang) = language {
        let mut parser = Parser::new();
        if parser.set_language(lang).is_err() {
            file_error = Some("failed_to_set_language".to_string());
        } else if let Some(tree) = parser.parse(content, None) {
            let root_node = tree.root_node();
            if root_node.has_error() {
                file_error = Some("syntax_error".to_string());
            }
            // Basic traversal for symbols
            traverse_nodes(root_node, content, &mut symbols, extension);
        } else {
            file_error = Some("parse_failure".to_string());
        }
    }

    FileSymbols {
        path: path.to_string_lossy().into_owned(),
        symbols,
        parsing_error: file_error,
    }
}

fn traverse_nodes(node: Node, content: &str, symbols: &mut Vec<Symbol>, extension: &str) {
    let kind = node.kind();
    let mut is_symbol = false;
    let mut symbol_kind = "";

    match extension {
        "rs" => {
            if matches!(kind, "function_item" | "struct_item" | "enum_item" | "trait_item" | "impl_item" | "type_item" | "use_declaration") {
                is_symbol = true;
                symbol_kind = match kind {
                    "function_item" => "function",
                    "struct_item" => "struct",
                    "enum_item" => "enum",
                    "trait_item" => "trait",
                    "impl_item" => "impl",
                    "type_item" => "type",
                    "use_declaration" => "import",
                    _ => "unknown",
                };
            }
        }
        "py" => {
            if matches!(kind, "function_definition" | "class_definition" | "import_statement" | "import_from_statement") {
                is_symbol = true;
                symbol_kind = match kind {
                    "function_definition" => "function",
                    "class_definition" => "class",
                    "import_statement" | "import_from_statement" => "import",
                    _ => "unknown",
                };
            }
        }
        "ts" | "tsx" => {
            if matches!(kind, "function_declaration" | "class_declaration" | "interface_declaration" | "type_alias_declaration" | "method_definition" | "method_signature" | "import_statement" | "export_statement") {
                is_symbol = true;
                symbol_kind = match kind {
                    "function_declaration" => "function",
                    "class_declaration" => "class",
                    "interface_declaration" => "interface",
                    "type_alias_declaration" => "type",
                    "method_definition" | "method_signature" => "method",
                    "import_statement" => "import",
                    "export_statement" => "export",
                    _ => "unknown",
                };
            }
        }
        _ => {}
    }

    if is_symbol {
        let name = get_name(node, content, extension);
        let range = get_range(node);
        let signature_range = get_signature_range(node, content, extension);
        let doc_range = get_doc_range(node, content, extension);

        symbols.push(Symbol {
            name,
            kind: symbol_kind.to_string(),
            range,
            signature_range,
            doc_range,
            parsing_error: if node.has_error() { Some("syntax_error".to_string()) } else { None },
        });
    }

    // Recurse for nested symbols (e.g. methods in classes)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse_nodes(child, content, symbols, extension);
    }
}

fn get_name(node: Node, content: &str, extension: &str) -> String {
    let kind = node.kind();
    
    // Special handling for imports/exports to get a meaningful name
    if kind == "use_declaration" || kind == "import_statement" || kind == "import_from_statement" || kind == "export_statement" {
        return content[node.start_byte()..node.end_byte()]
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
    }

    let name_node = match extension {
        "rs" => node.child_by_field_name("name"),
        "py" => node.child_by_field_name("name"),
        "ts" | "tsx" => node.child_by_field_name("name"),
        _ => None,
    };

    if let Some(n) = name_node {
        content[n.start_byte()..n.end_byte()].to_string()
    } else {
        "anonymous".to_string()
    }
}

fn get_range(node: Node) -> Range {
    Range {
        start_line: node.start_position().row,
        start_byte: node.start_byte(),
        end_line: node.end_position().row,
        end_byte: node.end_byte(),
    }
}

fn get_signature_range(node: Node, _content: &str, _extension: &str) -> Range {
    let body_node = node.child_by_field_name("body");
    if let Some(body) = body_node {
        Range {
            start_line: node.start_position().row,
            start_byte: node.start_byte(),
            end_line: body.start_position().row,
            end_byte: body.start_byte(),
        }
    } else {
        get_range(node)
    }
}

fn get_doc_range(node: Node, content: &str, extension: &str) -> Option<Range> {
    let mut current = node;
    
    // In TS, if it's an export, the comment is on the export statement
    if matches!(extension, "ts" | "tsx") {
        if let Some(parent) = current.parent() {
            if parent.kind() == "export_statement" {
                current = parent;
            }
        }
    }

    let mut prev = current.prev_sibling();
    while let Some(p) = prev {
        let p_kind = p.kind();
        if p_kind == "line_comment" || p_kind == "block_comment" || p_kind == "comment" {
            let comment_text = &content[p.start_byte()..p.end_byte()];
            let is_doc = match extension {
                "rs" => comment_text.starts_with("///") || comment_text.starts_with("/**") || comment_text.starts_with("//!"),
                "ts" | "tsx" => comment_text.starts_with("/**") || comment_text.starts_with("//"),
                "py" => comment_text.starts_with("\"\"\"") || comment_text.starts_with("'''"),
                _ => false,
            };
            if is_doc {
                return Some(get_range(p));
            }
        } else if p_kind == "attribute_item" || p_kind == "decorator" {
            // Skip decorators/macros
        } else {
            break;
        }
        prev = p.prev_sibling();
    }
    
    if extension == "py" {
        if let Some(body) = node.child_by_field_name("body") {
            if let Some(first_child) = body.child(0) {
                if first_child.kind() == "expression_statement" {
                    if let Some(string_node) = first_child.child(0) {
                        if string_node.kind() == "string" {
                            return Some(get_range(string_node));
                        }
                    }
                }
            }
        }
    }

    None
}
