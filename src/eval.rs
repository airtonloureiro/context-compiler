use std::path::Path;
use std::fs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct GroundTruth {
    pub name: String,
    pub gt_files: Vec<String>,
    #[serde(default)]
    pub symbols: Vec<String>,
    #[serde(default)]
    pub tokens_baseline: u64,
    #[serde(default)]
    pub tokens_gt: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EvalResult {
    pub tokens_ours: u64,
    pub tokens_baseline: u64,
    pub noise_ratio: f64,
    pub recall_rate: f64,
}

pub fn run_eval(_baseline_name: &str, repo: &Path, gt_path: Option<&Path>) -> i32 {
    // 1. Carregar Ground Truth
    let gt = match load_gt(gt_path, repo) {
        Ok(gt) => gt,
        Err(e) => {
            eprintln!("ctxc eval: erro ao carregar Ground Truth: {}", e);
            return 1;
        }
    };

    // 2. Localizar compiled-context.md
    let md_path = repo.join(".ctxc/compiled-context.md");
    if !md_path.exists() {
        eprintln!(
            "ctxc eval: arquivo compiled-context.md não encontrado em {:?}. Rode 'ctxc compile' primeiro.",
            md_path
        );
        return 1;
    }

    let content = match fs::read_to_string(&md_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ctxc eval: erro ao ler compiled-context.md: {}", e);
            return 1;
        }
    };

    // 3. Processamento
    // tokens_ours: usando Régua Única (bytes/3) conforme B-007
    let tokens_ours = (content.len() as u64).div_ceil(3);
    
    // Fallbacks para tokens baseados no BASELINE_COMPARISON.md se não estiverem no manifest
    let tokens_baseline = if gt.tokens_baseline > 0 { gt.tokens_baseline } else {
        match gt.name.as_str() {
            "context-compiler" => 129069,
            "zod" => 1661454,
            "ripgrep" => 889457,
            _ => 0
        }
    };
    
    let tokens_gt = if gt.tokens_gt > 0 { gt.tokens_gt } else {
        match gt.name.as_str() {
            "context-compiler" => 9441,
            "zod" => 37600,
            "ripgrep" => 9337,
            _ => 0
        }
    };

    // Recall: Verificação de presença no Markdown
    let mut total_items = 0;
    let mut found_items = 0;

    for file_path in &gt.gt_files {
        total_items += 1;
        // Padrão do header: ## `path`
        let marker = format!("## `{}`", file_path);
        if content.contains(&marker) {
            found_items += 1;
        }
    }

    for symbol in &gt.symbols {
        total_items += 1;
        if content.contains(symbol) {
            found_items += 1;
        }
    }

    let recall_rate = if total_items == 0 {
        1.0
    } else {
        found_items as f64 / total_items as f64
    };

    // Noise Ratio: (Tokens Ours - Tokens GT) / Tokens Ours
    let noise_ratio = if tokens_ours == 0 {
        0.0
    } else {
        (tokens_ours.saturating_sub(tokens_gt) as f64) / (tokens_ours as f64)
    };

    let result = EvalResult {
        tokens_ours,
        tokens_baseline,
        noise_ratio,
        recall_rate,
    };

    // 4. Output: Estritamente JSON no stdout
    match serde_json::to_string_pretty(&result) {
        Ok(json) => {
            println!("{}", json);
            0
        }
        Err(e) => {
            eprintln!("ctxc eval: erro ao serializar resultado: {}", e);
            1
        }
    }
}

fn load_gt(gt_path: Option<&Path>, repo: &Path) -> Result<GroundTruth, String> {
    let path = if let Some(p) = gt_path {
        p.to_path_buf()
    } else {
        repo.join(".ctxc/eval-manifest.json")
    };

    if !path.exists() {
        return Err(format!("Arquivo manifest/GT não encontrado em: {:?}", path));
    }

    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}
