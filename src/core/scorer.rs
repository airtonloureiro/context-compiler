use super::context_item::{ContextItem, ContextItemType};
use super::context_ir::{Task, TaskType};

#[derive(Debug)]
pub struct ScoredItem {
    pub item: ContextItem,
    pub score: i32,
    pub estimated_tokens: usize,
    /// MUST-INCLUDE flag (D-014): pacote sempre, independente de score/tamanho.
    pub must_include: bool,
}

pub struct Scorer;

impl Scorer {
    /// Atribui uma pontuação de prioridade para cada item.
    /// Regras (acumulativas):
    /// - SystemInstruction → +200 (sempre)
    /// - Message user → +100, Message assistant → +50
    /// - Log em task debug-like → +80
    /// - metadata.critical=true → +150 (e marca must_include)
    /// - Code: heurística de relevância (path no task/log, configs, entry points, src/)
    pub fn score(items: Vec<ContextItem>, task: &Task) -> Vec<ScoredItem> {
        let task_goal_lc = task.goal.as_deref().unwrap_or("").to_lowercase();
        items
            .into_iter()
            .map(|item| {
                // Estimativa de tokens: Base + Overhead do cabeçalho do item no prompt ("--- ITEM [...] ---\n\n")
                let estimated_tokens = crate::core::tokenizer::Tokenizer::count_tokens(&item.content) + 20;
                let mut score = 0i32;
                let mut must_include = false;

                // 1. Type-based base score.
                match item.item_type {
                    ContextItemType::SystemInstruction => score += 200,
                    ContextItemType::Message if item.role.as_deref() == Some("user") => {
                        score += 100
                    }
                    ContextItemType::Message => score += 50,
                    ContextItemType::Log => {
                        // Logs valem alto em qualquer tarefa de debug-like (não só "debug_error").
                        let is_debug = matches!(task.task_type, TaskType::DebugError);
                        let goal_has_debug = task_goal_lc.contains("debug")
                            || task_goal_lc.contains("error")
                            || task_goal_lc.contains("fail");
                        if is_debug || goal_has_debug {
                            score += 80;
                        }
                    }
                    _ => {}
                }

                // 2. metadata.critical / must_include (D-014).
                if let Some(meta) = &item.metadata {
                    if meta
                        .get("critical")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        score += 150;
                    }
                    if meta
                        .get("must_include")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        must_include = true;
                    }
                }

                // 3. Code item relevance (heurística determinística).
                if matches!(item.item_type, ContextItemType::Code) {
                    score += score_code_relevance(&item, &task_goal_lc, &task.task_type);
                }

                // 4. Sinal semântico (fastembed BGE-small).
                if let Some(meta) = &item.metadata {
                    if let Some(score_f) = meta.get("semantic_score").and_then(|v| v.as_f64()) {
                        let s = (score_f.max(0.0) * 200.0) as i32;
                        let penalty = if score_f < 0.3 { -10 } else { 0 };
                        score += s + penalty;
                    }
                    
                    // 5. Graph-RAG score (petgraph based PageRank/BFS boost)
                    if let Some(graph_score) = meta.get("graph_score").and_then(|v| v.as_f64()) {
                        score += graph_score as i32;
                    }
                }

                ScoredItem {
                    item,
                    score,
                    estimated_tokens,
                    must_include,
                }
            })
            .collect()
    }
}

fn score_code_relevance(item: &ContextItem, task_goal_lc: &str, task_type: &TaskType) -> i32 {
    let path = item
        .source
        .as_ref()
        .and_then(|s| s.get("path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if path.is_empty() {
        return 0;
    }
    let path_lc = path.to_lowercase();
    let basename = path_lc.rsplit('/').next().unwrap_or(&path_lc).to_string();
    let mut s = 0i32;

    // Citação direta no goal.
    if !task_goal_lc.is_empty() {
        if task_goal_lc.contains(&path_lc) {
            s += 120;
        } else if !basename.is_empty() && task_goal_lc.contains(&basename) {
            s += 100;
        }
    }

    // Configs de stack-profile
    let is_config = matches!(
        basename.as_str(),
        "package.json"
            | "cargo.toml"
            | "pyproject.toml"
            | "requirements.txt"
            | "go.mod"
            | "dockerfile"
            | "docker-compose.yml"
            | "docker-compose.yaml"
            | "tsconfig.json"
    );

    if is_config {
        s += 80;
    }

    // Heurísticas específicas por Task Type (Sprint P1)
    match task_type {
        TaskType::DebugError => {
            // Em debug_error: prioriza logs (já feito no tipo) e configs/testes
            if is_config { s += 20; } // Bonus extra para config em debug
            if path_lc.contains("test") || path_lc.contains("spec") {
                s += 20; // Testes ajudam a reproduzir
            }
        },
        TaskType::ModifyCode => {
            // Em modify_code: prioriza source code
            if path_lc.starts_with("src/") || path_lc.starts_with("lib/") {
                s += 30; 
            }
        },
        TaskType::ExplainCode => {
            // Em explain_code: menos bonus para testes
        },
        TaskType::ArchitectureReview => {
            // Em architecture_review: prioriza configs, READMEs, ADRs, entry points
            if is_config { s += 40; }
            if path_lc.contains("adr") || path_lc.contains("docs") || path_lc.contains("readme") {
                s += 50;
            }
        },
        _ => {}
    }

    // Entry-point principal.
    if matches!(
        path_lc.as_str(),
        "src/main.rs"
            | "src/lib.rs"
            | "src/index.ts"
            | "src/index.js"
            | "src/main.ts"
            | "main.py"
            | "app.py"
            | "index.js"
            | "index.ts"
    ) {
        s += 90;
    }

    // Source code genérico
    if path_lc.starts_with("src/")
        || path_lc.starts_with("lib/")
        || path_lc.contains("/src/")
    {
        s += 50;
    }

    // Test files genérico
    if path_lc.contains("/test")
        || path_lc.contains("/__tests__/")
        || basename.starts_with("test_")
        || basename.ends_with(".test.ts")
        || basename.ends_with(".test.js")
        || basename.ends_with(".spec.ts")
    {
        s += 25;
    }

    // README e CHANGELOG
    if matches!(basename.as_str(), "readme.md" | "changelog.md") {
        s += 15;
    }

    // Penalty: junk/CI files
    if path_lc.starts_with(".github/")
        || path_lc.starts_with(".cargo/")
        || basename == ".gitignore"
        || basename == ".gitattributes"
        || basename == ".editorconfig"
        || basename == "license"
        || basename == "license-mit"
        || basename == "license-apache"
        || basename == "unlicense"
        || basename == "copying"
        || basename == "funding.yml"
    {
        s -= 30;
    }

    s
}
