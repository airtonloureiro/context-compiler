use serde::{Deserialize, Serialize};
use reqwest::blocking::Client;
use std::time::Duration;
use rayon::prelude::*;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkimInput {
    pub task: String,
    pub file: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SkimClassification {
    Keep,
    Drop,
    Summarize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkimOutput {
    pub classification: SkimClassification,
    pub reason: String,
}

pub struct SkimClient {
    client: Client,
    model: String,
    url: String,
}

impl SkimClient {
    pub fn new(model: &str) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_else(|_| Client::new()),
            model: model.to_string(),
            url: "http://localhost:11434/api/generate".to_string(),
        }
    }

    pub fn classify(&self, input: &SkimInput) -> Result<SkimOutput, String> {
        let system_prompt = r#"Você é o Skim, um classificador de relevância de código de alto desempenho para o Context Compiler.
Sua tarefa é analisar um símbolo de código (função, classe, método) e decidir se ele é essencial para resolver uma tarefa descrita pelo usuário.

Categorias de Saída:
- KEEP: O símbolo é central para a tarefa.
- DROP: O símbolo é irrelevante.
- SUMMARIZE: O símbolo é importante para a estrutura, mas o corpo não é necessário.

Responda APENAS com um objeto JSON válido no formato: {"classification": "KEEP|DROP|SUMMARIZE", "reason": "..."}"#;

        let prompt = serde_json::to_string(input).map_err(|e| e.to_string())?;

        let body = serde_json::json!({
            "model": self.model,
            "prompt": format!("System: {}\n\nInput: {}", system_prompt, prompt),
            "stream": false,
            "format": "json"
        });

        let response = self.client.post(&self.url)
            .json(&body)
            .send()
            .map_err(|e| format!("Falha ao conectar ao Ollama: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Erro do Ollama: {}", response.status()));
        }

        let json: serde_json::Value = response.json().map_err(|e| e.to_string())?;
        let content = json["response"].as_str().ok_or("Resposta vazia do Ollama")?;

        let output: SkimOutput = serde_json::from_str(content)
            .map_err(|e| format!("Erro ao parsear saída do modelo: {} (Output: {})", e, content))?;

        Ok(output)
    }

    pub fn classify_bulk(&self, inputs: Vec<SkimInput>) -> Vec<(SkimInput, Result<SkimOutput, String>)> {
        // Usando rayon para paralelismo.
        // reqwest blocking Client é thread-safe.
        inputs.into_par_iter()
            .map(|input| {
                let res = self.classify(&input);
                (input, res)
            })
            .collect()
    }
}
