use tiny_http::{Server, Response, Header};
use serde_json::{Value, json};

use crate::core::context_item::{ContextItem, ContextItemType};
use crate::core::context_ir::{Task, Target};
use crate::core::compiler::compile_generic;

pub fn run_gateway(port: u16, dry_run: bool) -> i32 {
    let server = Server::http(format!("0.0.0.0:{}", port)).unwrap();
    println!("Context Compiler Gateway Alpha rodando na porta {}", port);
    if dry_run {
        println!("Modo DRY-RUN ativado. Nenhuma chamada será feita ao provider.");
    }

    for mut request in server.incoming_requests() {
        if request.url() == "/v1/chat/completions" && request.method().as_str() == "POST" {
            let mut content = String::new();
            if let Err(e) = request.as_reader().read_to_string(&mut content) {
                eprintln!("Erro ao ler requisição: {}", e);
                let _ = request.respond(Response::from_string("Bad Request").with_status_code(400));
                continue;
            }

            let mut body: Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(_) => {
                    let _ = request.respond(Response::from_string("Invalid JSON").with_status_code(400));
                    continue;
                }
            };

            // Extrair messages para compilar
            let mut items = Vec::new();
            let mut budget = 2000; // default simulado
            
            if let Some(max_tokens) = body.get("max_tokens").and_then(|v| v.as_u64()) {
                // Heurística simples: se o usuário pediu X max_tokens, podemos limitar o context
                // budget ao que sobrar do limite do modelo, mas aqui vamos simular um budget fixo
                // para o MVP do proxy.
                budget = 4000;
            }

            if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
                for (i, msg) in messages.iter().enumerate() {
                    let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user").to_string();
                    let content_str = msg.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    
                    let item_type = if role == "system" {
                        ContextItemType::SystemInstruction
                    } else {
                        ContextItemType::Message
                    };

                    items.push(ContextItem {
                        id: format!("msg_{}", i),
                        item_type,
                        role: Some(role),
                        content: content_str,
                        source: None,
                        metadata: None,
                        sensitivity: crate::core::context_item::Sensitivity::Public,
                    });
                }
            }

            // Precisamos rodar a compilação.
            // Para não reescrever a lógica interna do compilador e reaproveitar a engine (que grava no .ctxc),
            // criamos o payload genérico e salvamos num input temporário e o chamamos.
            let task = Task {
                task_type: crate::core::context_ir::TaskType::GatewayProxy,
                goal: Some("Responder à chamada da API interceptada".to_string()),
                user_request: None,
            };

            let target = Target {
                provider: "openai".to_string(),
                model: body.get("model").and_then(|m| m.as_str()).map(|s| s.to_string()),
                token_budget: budget,
            };

            let generic_input = crate::core::compiler::GenericInput {
                task,
                target,
                context_items: items,
            };

            let tmp_path = std::path::Path::new(".ctxc-gateway-input.json");
            std::fs::write(tmp_path, serde_json::to_string(&generic_input).unwrap()).unwrap();

            // Roda o core
            let _ = compile_generic(tmp_path, "openai", budget, None);
            let _ = std::fs::remove_file(tmp_path);

            // Lê o resultado da compilação e os reports
            let token_report: crate::core::token_report::TokenReport = 
                serde_json::from_str(&std::fs::read_to_string(".ctxc/token-report.json").unwrap_or_default())
                .unwrap_or_default();
            
            let compiled_prompt_str = std::fs::read_to_string(".ctxc/compiled-context.md").unwrap_or_default();
            
            // O compiled-context.md do openai.rs devolve um JSON array de messages em String.
            // Vamos fazer parse disso e substituir na request original.
            if let Ok(compiled_json) = serde_json::from_str::<Value>(&compiled_prompt_str) {
                if let Some(compiled_msgs) = compiled_json.get("messages") {
                    body["messages"] = compiled_msgs.clone();
                }
            }

            if dry_run {
                // No modo dry-run, devolvemos a própria request otimizada como resposta simulada
                let mut response = Response::from_string(serde_json::to_string_pretty(&body).unwrap())
                    .with_status_code(200);
                response.add_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
                response.add_header(Header::from_bytes(&b"X-CTXC-Token-Before"[..], token_report.before.to_string().as_bytes()).unwrap());
                response.add_header(Header::from_bytes(&b"X-CTXC-Token-After"[..], token_report.after.to_string().as_bytes()).unwrap());
                let reduction_pct = format!("{:.1}%", token_report.reduction_ratio * 100.0);
                response.add_header(Header::from_bytes(&b"X-CTXC-Reduction"[..], reduction_pct.as_bytes()).unwrap());

                let _ = request.respond(response);
            } else {
                // Hybrid Routing: Decisão de destino estilo RouteLLM.
                // Redireciona o tráfego para economia massiva. Se o usuário pediu um modelo local ou se o contexto final 
                // otimizado for extremamente pequeno (tarefa trivial), poupa dinheiro e bate no Ollama local (se existir porta).
                let req_model = body.get("model").and_then(|m| m.as_str()).unwrap_or("gpt-3.5-turbo").to_lowercase();
                
                let is_local_model = req_model.contains("llama") || req_model.contains("qwen") || req_model.contains("mistral");
                let is_trivial_task = token_report.after < 500 && req_model != "gpt-4" && !req_model.contains("claude-3-opus");
                
                let (target_url, api_key) = if is_local_model || is_trivial_task {
                    println!("🔀 Hybrid Routing Ativo: Tarefa classificada para modelo Local (Custo $0). Redirecionando para Ollama.");
                    ("http://127.0.0.1:11434/v1/chat/completions".to_string(), String::new())
                } else {
                    let key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
                    ("https://api.openai.com/v1/chat/completions".to_string(), key)
                };

                let client = reqwest::blocking::Client::new();
                
                let mut req_builder = client.post(&target_url).json(&body);
                if !api_key.is_empty() {
                    req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
                }
                
                let res = req_builder.send();
                    
                match res {
                    Ok(upstream_res) => {
                        let status = upstream_res.status().as_u16();
                        let upstream_body = upstream_res.text().unwrap_or_default();
                        
                        let mut response = Response::from_string(upstream_body).with_status_code(status);
                        response.add_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
                        response.add_header(Header::from_bytes(&b"X-CTXC-Token-Before"[..], token_report.before.to_string().as_bytes()).unwrap());
                        response.add_header(Header::from_bytes(&b"X-CTXC-Token-After"[..], token_report.after.to_string().as_bytes()).unwrap());
                        let reduction_pct = format!("{:.1}%", token_report.reduction_ratio * 100.0);
                        response.add_header(Header::from_bytes(&b"X-CTXC-Reduction"[..], reduction_pct.as_bytes()).unwrap());
                        
                        let _ = request.respond(response);
                    },
                    Err(e) => {
                        let err_msg = json!({"error": e.to_string()});
                        let _ = request.respond(Response::from_string(err_msg.to_string()).with_status_code(502));
                    }
                }
            }

        } else {
            let _ = request.respond(Response::from_string("Not Found").with_status_code(404));
        }
    }
    
    0
}
