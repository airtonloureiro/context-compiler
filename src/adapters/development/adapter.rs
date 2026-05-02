use std::path::Path;
use crate::core::context_item::{ContextItem, ContextItemType};
use crate::core::embedding::SemanticSearch;
use crate::core::keyword_score::{extract_keywords, keyword_match_score};
use super::repo_scanner;
use super::stack_detector::StackDetector;

/// Tamanho máximo do snippet por arquivo enviado ao embedder.
/// BGE-small aceita até ~512 tokens (~2000 chars). Truncamos para não estourar e manter latência.
const EMBED_SNIPPET_MAX_CHARS: usize = 1500;

/// Constrói "documento" para embedding de um arquivo: path + cabeçalho do conteúdo.
/// O path importa: muitos arquivos curtos (configs) ganham contexto pelo nome.
fn build_embed_snippet(rel_path: &str, content: &str) -> String {
    let head: String = content.chars().take(EMBED_SNIPPET_MAX_CHARS).collect();
    format!("{}\n\n{}", rel_path, head)
}

/// Constrói "query" para embedding da tarefa: task description + log (truncado).
fn build_embed_query(task: &str, log: Option<&str>) -> String {
    let mut q = task.to_string();
    if let Some(l) = log {
        q.push_str("\n\n");
        let l_trunc: String = l.chars().take(800).collect();
        q.push_str(&l_trunc);
    }
    q
}

/// Configs canônicos de stack-profile.
/// Usados como MUST-INCLUDE apenas quando em depth ≤ 1 (root + 1 nível). Em monorepos
/// (ripgrep com 11 Cargo.toml, zod com 9 package.json) marcar todos crashea o budget
/// — sub-pacotes ainda recebem score alto via scorer, mas podem ser cortados.
const STACK_PROFILE_BASENAMES: &[&str] = &[
    "package.json",
    "cargo.toml",
    "pyproject.toml",
    "requirements.txt",
    "go.mod",
    "dockerfile",
    "docker-compose.yml",
    "docker-compose.yaml",
    "tsconfig.json",
];

/// Entry-points canônicos: arquivos que sempre são MUST-INCLUDE em projetos típicos.
const ENTRY_POINT_PATHS: &[&str] = &[
    "src/main.rs",
    "src/lib.rs",
    "src/index.ts",
    "src/index.js",
    "src/main.ts",
    "src/main.py",
    "src/app.py",
    "main.py",
    "app.py",
    "index.js",
    "index.ts",
    "app/layout.tsx",
    "app/page.tsx",
];

/// Decide se este arquivo é MUST-INCLUDE (`metadata.critical = true`).
///
/// Hard-rule (D-014): se MUST-INCLUDE > budget, pipeline falha explícita.
/// Por isso a regra abaixo é **conservadora** — marca só o que é genuinamente
/// indispensável; outros configs/source files entram via score do scorer.
fn is_critical_file(rel_path: &str, task: &str, log: Option<&str>) -> bool {
    let lc = rel_path.to_lowercase();

    // 1. Citação direta em task ou log → critical (qualquer caminho).
    if task.to_lowercase().contains(&lc) {
        return true;
    }
    if let Some(l) = log {
        if l.to_lowercase().contains(&lc) {
            return true;
        }
    }

    // 2. Stack-profile config em depth ≤ 1 (root ou um nível de subpasta).
    // Bug fix (Fix A): antes marcava TODOS Cargo.toml/package.json sem filtro,
    // o que em monorepos enchia o budget só com configs.
    let basename = lc.rsplit('/').next().unwrap_or(&lc);
    let depth = lc.matches('/').count();
    if STACK_PROFILE_BASENAMES.contains(&basename) && depth <= 1 {
        return true;
    }

    // 3. Entry-points canônicos (Rust binary, Node entry, Python __main__, Next.js root).
    if ENTRY_POINT_PATHS.contains(&lc.as_str()) {
        return true;
    }

    false
}

pub struct DevelopmentAdapter;

impl DevelopmentAdapter {
    /// Varre o repositório, detecta a stack e retorna a lista de ContextItems
    /// que o Core Universal usará para compilar o contexto final.
    pub fn ingest(repo_root: &Path, task_description: &str, error_log: Option<&str>) -> Vec<ContextItem> {
        let mut items = Vec::new();

        // 1. Task & Request info
        items.push(ContextItem {
            id: "task_instruction".to_string(),
            item_type: ContextItemType::SystemInstruction,
            role: Some("system".to_string()),
            content: format!("Você está em um repositório de software. Sua tarefa: {}", task_description),
            source: None,
            metadata: Some(serde_json::json!({"critical": true})),
            sensitivity: crate::core::context_item::Sensitivity::Public,
        });

        // 2. Ingest Logs
        if let Some(log) = error_log {
            items.push(ContextItem {
                id: "error_log".to_string(),
                item_type: ContextItemType::Log,
                role: None,
                content: log.to_string(),
                source: Some(serde_json::json!({"kind": "user_provided_log"})),
                metadata: Some(serde_json::json!({"critical": true})),
                sensitivity: crate::core::context_item::Sensitivity::Public,
            });
        }

        // 3. Stack Profiles
        let profiles = StackDetector::detect(repo_root);
        let profiles_str = profiles.iter().map(|p| format!("{:?}", p)).collect::<Vec<_>>().join(", ");
        items.push(ContextItem {
            id: "stack_profile".to_string(),
            item_type: ContextItemType::Metadata,
            role: None,
            content: format!("Stack detectada: {}", profiles_str),
            source: None,
            metadata: None,
            sensitivity: crate::core::context_item::Sensitivity::Public,
        });

        // 3.5 Semantic search com graceful degradation enterprise-grade.
        // Otimização: Inicializar ONNX FastEmbed agora utiliza o modelo MultilingualE5Small
        // para suportar queries em PT-BR para código em EN. 
        // Habilitado por padrão para garantir precisão semântica (cross-lingual).
        // Pode ser desativado com CTXC_FAST_EMBED=0 para velocidade extrema.
        let use_fastembed = std::env::var("CTXC_FAST_EMBED").unwrap_or_else(|_| "0".to_string()) == "1";
        let embedder = if use_fastembed {
            match SemanticSearch::new() {
                Ok(e) => Some(e),
                Err(err) => {
                    eprintln!("ctxc: fastembed indisponível ({}); usando fallback keyword TF", err);
                    None
                }
            }
        } else {
            None
        };

        let mut embed_targets: Vec<(usize, String)> = Vec::new();

        // 4. File Map & Repo Scan Ingest
        if let Ok((report, raw_entries)) = repo_scanner::scan_with_entries(repo_root) {
            items.push(ContextItem {
                id: "repo_scan_report".to_string(),
                item_type: ContextItemType::Metadata,
                role: None,
                content: repo_scanner::render(&report),
                source: None,
                metadata: None,
                sensitivity: crate::core::context_item::Sensitivity::Public,
            });

            use rayon::prelude::*;
            let task_description_lc = task_description.to_lowercase();
            let repo_root_path = repo_root.to_path_buf();

            let parsed_results: Vec<_> = raw_entries.into_par_iter().filter_map(|entry| {
                if entry.class == repo_scanner::FileClass::Considered && entry.kind == repo_scanner::RawKind::File {
                    let path = repo_root_path.join(&entry.rel_path);
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let mut final_content = content.clone();
                        let is_targeted = task_description_lc.contains(&entry.rel_path.to_lowercase());
                        
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        let file_symbols = if matches!(ext, "rs" | "ts" | "tsx" | "py" | "js") {
                            Some(crate::adapters::development::symbol_map::extract_symbols(&path, &content))
                        } else {
                            None
                        };

                        if !is_targeted && content.len() > 2000 {
                            if let Some(syms) = &file_symbols {
                                if syms.parsing_error.is_none() && !syms.symbols.is_empty() {
                                    let mut skeleton = format!("// [COMPRESSED: AST SKELETON] {}\n", entry.rel_path);
                                    for sym in &syms.symbols {
                                        let sig = crate::adapters::development::context_slicing::ContextSlicer::slice_signature(&content, sym);
                                        if !sig.is_empty() {
                                            if ext == "py" {
                                                skeleton.push_str(&format!("{} ...\n", sig.trim()));
                                            } else {
                                                skeleton.push_str(&format!("{} {{ /* ... */ }}\n", sig.trim()));
                                            }
                                        } else {
                                            skeleton.push_str(&format!("{} {} {{ /* stale range */ }}\n", sym.kind, sym.name));
                                        }
                                    }
                                    final_content = skeleton;
                                }
                            }
                        }
                        
                        // Fix paths inside file_symbols to be relative (so they match between components)
                        let mut final_symbols = file_symbols;
                        if let Some(ref mut fsyms) = final_symbols {
                            fsyms.path = entry.rel_path.clone();
                        }

                        let embed_snippet = build_embed_snippet(&entry.rel_path, &content);
                        return Some((entry, final_content, embed_snippet, final_symbols));
                    }
                }
                None
            }).collect();

            // 4.5 Graph-RAG Traversal
            let mut symbol_files = Vec::new();
            let mut critical_files = std::collections::HashSet::new();

            for (entry, _, _, symbols_opt) in &parsed_results {
                if let Some(syms) = symbols_opt {
                    symbol_files.push(syms.clone());
                }
                if is_critical_file(&entry.rel_path, task_description, error_log) {
                    critical_files.insert(format!("file:{}", entry.rel_path));
                }
            }

            let symbol_map = crate::adapters::development::symbol_map::SymbolMap {
                schema_version: "1.0.0".to_string(),
                files: symbol_files,
            };
            let knowledge_graph = crate::adapters::development::graph_builder::GraphBuilder::build(&symbol_map);

            let mut graph_boosts: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
            for edge in &knowledge_graph.edges {
                if edge.edge_type == crate::adapters::development::graph_builder::EdgeType::Imports {
                    if critical_files.contains(&edge.source) {
                        // Target is imported by a critical file -> boost target
                        *graph_boosts.entry(edge.target.clone()).or_insert(0.0) += edge.weight * 50.0;
                    }
                    if critical_files.contains(&edge.target) {
                        // Source imports a critical file -> boost source
                        *graph_boosts.entry(edge.source.clone()).or_insert(0.0) += edge.weight * 30.0;
                    }
                }
            }

            for (entry, final_content, embed_snippet, _) in parsed_results {
                let critical = is_critical_file(&entry.rel_path, task_description, error_log);
                let graph_score = graph_boosts.get(&format!("file:{}", entry.rel_path)).copied().unwrap_or(0.0);
                
                let mut meta_map = serde_json::Map::new();
                if critical {
                    meta_map.insert("critical".to_string(), serde_json::Value::Bool(true));
                    meta_map.insert("must_include".to_string(), serde_json::Value::Bool(true));
                }
                if graph_score > 0.0 {
                    meta_map.insert("graph_score".to_string(), serde_json::json!(graph_score));
                }
                
                let metadata = if meta_map.is_empty() {
                    None
                } else {
                    Some(serde_json::Value::Object(meta_map))
                };
                
                embed_targets.push((items.len(), embed_snippet));
                
                items.push(ContextItem {
                    id: format!("file_{}", entry.rel_path.replace("/", "_")),
                    item_type: ContextItemType::Code,
                    role: None,
                    content: final_content,
                    source: Some(serde_json::json!({"path": entry.rel_path})),
                    metadata,
                    sensitivity: crate::core::context_item::Sensitivity::Public,
                });
            }
        }

        // 5. Semantic scoring com tier 1 (fastembed) + tier 2 (keyword TF) fallback.
        if !embed_targets.is_empty() {
            let query = build_embed_query(task_description, error_log);
            let scores = compute_semantic_scores(embedder.as_ref(), &query, &embed_targets);

            // Inject score em metadata. Preserva critical/must_include se já existirem.
            for ((item_idx, _), score) in embed_targets.iter().zip(scores.iter()) {
                let item = &mut items[*item_idx];
                let mut m = match item.metadata.take() {
                    Some(serde_json::Value::Object(map)) => map,
                    _ => serde_json::Map::new(),
                };
                m.insert(
                    "semantic_score".to_string(),
                    serde_json::Value::from(*score as f64),
                );
                item.metadata = Some(serde_json::Value::Object(m));
            }
        }

        items
    }
}

/// Computa score [0..1] para cada doc em `embed_targets` contra `query`.
/// Tier 1: fastembed cosine similarity (semântica real).
/// Tier 2: keyword TF (fallback quando embedder None ou erro de inferência).
fn compute_semantic_scores(
    embedder: Option<&SemanticSearch>,
    query: &str,
    embed_targets: &[(usize, String)],
) -> Vec<f32> {
    // Tier 1: fastembed
    if let Some(emb) = embedder {
        let mut all_texts: Vec<&str> = Vec::with_capacity(embed_targets.len() + 1);
        all_texts.push(query);
        for (_, s) in embed_targets {
            all_texts.push(s);
        }
        match emb.embed(all_texts) {
            Ok(vectors) if vectors.len() == embed_targets.len() + 1 => {
                let task_vec = &vectors[0];
                return embed_targets
                    .iter()
                    .enumerate()
                    .map(|(i, _)| SemanticSearch::cosine_similarity(task_vec, &vectors[i + 1]))
                    .collect();
            }
            Ok(_) => {
                eprintln!("ctxc: embed retornou contagem inesperada; usando keyword fallback");
            }
            Err(err) => {
                eprintln!("ctxc: erro de inferência ({}); usando keyword fallback", err);
            }
        }
    }

    // Tier 2: keyword TF fallback (sempre funciona, sem rede, sem ML).
    let keywords = extract_keywords(query);
    if keywords.is_empty() {
        // Query sem keywords úteis (só stopwords) → todos os docs ficam neutros.
        return vec![0.5; embed_targets.len()];
    }
    embed_targets
        .iter()
        .map(|(_, snippet)| {
            let snippet_lc = snippet.to_lowercase();
            keyword_match_score(&keywords, &snippet_lc)
        })
        .collect()
}