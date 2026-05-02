import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import * as path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function runTest() {
  console.log("Iniciando Cliente de Teste MCP...\n");
  
  const serverPath = path.resolve(__dirname, "../build/index.js");
  
  // O transporte stdio é o padrão usado por Cursor/Claude Desktop para falar com ferramentas
  const transport = new StdioClientTransport({
    command: "node",
    args: [serverPath],
  });

  const client = new Client({
    name: "test-client",
    version: "1.0.0",
  }, {
    capabilities: {}
  });

  console.log("1. Conectando ao processo do Servidor MCP...");
  await client.connect(transport);
  console.log("✅ Conexão stdio estabelecida.\n");

  console.log("2. Solicitando lista de ferramentas ao servidor...");
  const tools = await client.listTools();
  console.log(`✅ Ferramentas expostas pela API: [${tools.tools.map(t => t.name).join(", ")}]\n`);

  console.log("3. Disparando comando para o servidor rodar o Context Compiler no repositório OTCv8...");
  const repoPath = path.resolve(__dirname, "../../.tmp/otclientv8");
  const taskDesc = "Como eu faço para criar um bot que recupera minha vida quando eu estiver apenas com 20% de vida";
  
  console.log(`   - Repositório Alvo: ${repoPath}`);
  console.log(`   - Tarefa (Intenção): ${taskDesc}`);
  console.log(`   - Budget de Tokens: 4000\n`);

  try {
    console.log("Aguardando o processamento pesado no Rust em background...");
    const startTime = Date.now();
    const result = await client.callTool({
      name: "compile_context",
      arguments: {
        repo_path: repoPath,
        task: taskDesc,
        budget: 4000
      }
    });
    const elapsed = Date.now() - startTime;

    console.log(`✅ Sucesso! O servidor concluiu a tarefa em ${elapsed}ms e devolveu a resposta via stdio.`);
    const content = (result as any).content[0] as { type: "text", text: string };
    
    console.log("\n--- PRÉVIA DO CONTEXTO DEVOLVIDO PARA A IDE (Primeiros 500 chars) ---\n");
    console.log(content.text.substring(0, 500) + "...\n");
    console.log("----------------------------------------------------------------------\n");
    
    console.log(`📊 O Servidor entregou um Markdown final perfeitamente mastigado contendo ${content.text.length} caracteres.`);
  } catch (err) {
    console.error("❌ Erro ao chamar a ferramenta:", err);
  } finally {
    // Encerra a conexão para liberar o terminal
    await transport.close();
  }
}

runTest().catch(console.error);