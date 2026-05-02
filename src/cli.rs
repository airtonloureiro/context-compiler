use std::path::PathBuf;

use clap::{Parser, Subcommand};

const ABOUT: &str = "ctxc — Context Compiler: Motor universal de otimização de contexto para LLMs.";

const LONG_ABOUT: &str = "ctxc — Context Compiler: Motor universal de otimização de contexto para LLMs.\n\nTransforma contexto bruto em contexto mínimo, estruturado e validável orientado à tarefa.";

#[derive(Parser, Debug)]
#[command(
    name = "ctxc",
    bin_name = "ctxc",
    version = "0.0.2",
    about = ABOUT,
    long_about = LONG_ABOUT,
    disable_version_flag = true,
    arg_required_else_help = true,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Compila contexto a partir de um input genérico (Context IR genérico ou lista de itens).
    Compile {
        /// Caminho para o context.json genérico.
        #[arg(long, value_name = "FILE")]
        input: PathBuf,
        /// Target provider. Default: openai.
        #[arg(long, value_name = "PROVIDER", default_value = "openai")]
        target: String,
        /// Token budget. Default: 2000.
        #[arg(long, value_name = "TOKENS", default_value = "2000")]
        budget: usize,
    },
    /// Adapter de desenvolvimento: Compila contexto a partir de repositório e logs.
    Dev {
        /// Caminho do repositório local a varrer. Default: '.'.
        #[arg(long, value_name = "PATH", default_value = ".")]
        repo: PathBuf,
        /// Categoria da tarefa (debug_error, modify_code, explain_code, architecture_review).
        #[arg(long, value_name = "TASK")]
        task: String,
        /// A intenção específica ou erro do usuário.
        #[arg(long, value_name = "GOAL")]
        goal: String,
        /// Caminho do log de erro (opcional).
        #[arg(long, value_name = "PATH")]
        log: Option<PathBuf>,
        /// Token budget. Default: 2000.
        #[arg(long, value_name = "TOKENS", default_value = "2000")]
        budget: usize,
        /// (Opcional) Usar modelo local para classificação refinada.
        #[arg(long)]
        skim: bool,
    },
    /// Varre um repositório local e imprime resumo determinístico.
    Scan {
        /// Caminho do repositório local a varrer.
        #[arg(long, value_name = "PATH")]
        repo: PathBuf,
    },
    /// Avaliar contexto contra eval set.
    Eval {
        /// Nome do baseline para comparação.
        #[arg(long, value_name = "NAME", default_value = "repomix")]
        baseline: String,
        /// Caminho do repositório local. Default: '.'.
        #[arg(long, value_name = "PATH", default_value = ".")]
        repo: PathBuf,
        /// Caminho para o Ground Truth (JSON).
        #[arg(long, value_name = "PATH")]
        gt: Option<PathBuf>,
    },
    /// Inicia o Gateway OpenAI-compatible local
    Serve {
        /// Porta para o gateway escutar. Default: 8711.
        #[arg(long, value_name = "PORT", default_value = "8711")]
        port: u16,
        /// Modo dry-run: não repassa a requisição para a OpenAI, devolve o payload que seria enviado.
        #[arg(long)]
        dry_run: bool,
    },
    /// Inspecionar artefatos compilados
    Inspect {
        /// Caminho do repositório local. Default: '.'.
        #[arg(long, value_name = "PATH", default_value = ".")]
        repo: PathBuf,
    },
    /// Explicar porque um arquivo/alvo foi mantido ou descartado no contexto.
    Explain {
        /// Caminho do repositório local. Default: '.'.
        #[arg(long, value_name = "PATH", default_value = ".")]
        repo: PathBuf,
        /// Nome do arquivo, ID do item ou texto para buscar a explicação.
        #[arg(long, value_name = "TARGET")]
        target: Option<String>,
    },
}

pub fn run(cli: Cli) -> i32 {
    match cli.command {
        Some(Commands::Compile { input, target, budget }) => {
            match crate::core::compiler::compile_generic(&input, &target, budget, None) {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("Erro ao compilar: {}", e);
                    1
                }
            }
        },
        Some(Commands::Dev { repo, task, goal, log, budget, skim: _ }) => {
            if log.is_none() {
                eprintln!("💡 UX Hint: Sem --log ou citação de arquivos no --goal, o recall do contexto pode ser baixo. Considere fornecer logs de erro reais ou caminhos de arquivos para resultados melhores.");
            }
            
            let log_str = log.map(|p| std::fs::read_to_string(&p).unwrap_or_else(|_| p.to_string_lossy().to_string()));
            let items = crate::adapters::development::adapter::DevelopmentAdapter::ingest(&repo, &goal, log_str.as_deref());
            
            let task_type = match task.as_str() {
                "debug_error" => crate::core::context_ir::TaskType::DebugError,
                "modify_code" => crate::core::context_ir::TaskType::ModifyCode,
                "explain_code" => crate::core::context_ir::TaskType::ExplainCode,
                "architecture_review" => crate::core::context_ir::TaskType::ArchitectureReview,
                _ => crate::core::context_ir::TaskType::Generic(task.clone()),
            };

            let gen_input = crate::core::compiler::GenericInput {
                task: crate::core::context_ir::Task {
                    task_type,
                    goal: Some(goal.clone()),
                    user_request: None,
                },
                target: crate::core::context_ir::Target {
                    provider: "openai".to_string(),
                    model: None,
                    token_budget: budget,
                },
                context_items: items,
            };
            
            let out_dir = repo.join(".ctxc");
            let tmp_path = out_dir.join("dev-tmp.json");
            if !out_dir.exists() {
                std::fs::create_dir_all(&out_dir).unwrap();
            }
            std::fs::write(&tmp_path, serde_json::to_string(&gen_input).unwrap()).unwrap();
            
            let res = crate::core::compiler::compile_generic(&tmp_path, "openai", budget, Some(&out_dir));
            let _ = std::fs::remove_file(tmp_path);
            
            match res {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("Erro ao compilar contexto de desenvolvimento: {}", e);
                    1
                }
            }
        },
        Some(Commands::Scan { repo }) => crate::adapters::development::repo_scanner::run_scan(&repo),
        Some(Commands::Inspect { repo }) => crate::inspect::run_inspect(&repo),
        Some(Commands::Explain { repo, target }) => crate::explain::run_explain(&repo, target.as_deref()),
        Some(Commands::Serve { port, dry_run }) => crate::gateway::server::run_gateway(port, dry_run),
        Some(Commands::Eval { baseline, repo, gt }) => {
            crate::eval::run_eval(&baseline, &repo, gt.as_deref())
        }
        None => {
            eprintln!("ctxc: nenhum subcomando fornecido. Rode 'ctxc --help'.");
            64
        }
    }
}
