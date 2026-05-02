#!/usr/bin/env node

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { execFile } from "child_process";
import { promisify } from "util";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const execAsync = promisify(execFile);

// Resolve __dirname equivalente para ESM
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Obtém o caminho do binário do Context Compiler compilado (na raiz do repositório)
const ctxcBinary = path.resolve(__dirname, "../../target/release/ctxc");

if (!fs.existsSync(ctxcBinary)) {
    console.error(`ERROR: Context Compiler binary not found at ${ctxcBinary}`);
    console.error("Please run 'cargo build --release' in the root directory first.");
    process.exit(1);
}

// 1. Instancia o Servidor MCP
const server = new Server(
  {
    name: "ctxc-mcp",
    version: "1.0.0",
  },
  {
    capabilities: {
      tools: {},
    },
  }
);

// 2. Define as Ferramentas para a IDE
server.setRequestHandler(ListToolsRequestSchema, async () => {
  return {
    tools: [
      {
        name: "compile_context",
        description:
          "Compiles a local repository into a focused, highly compressed context markdown using Context Compiler and Graph-RAG. Use this when you need to understand or modify code across multiple files in a repository.",
        inputSchema: {
          type: "object",
          properties: {
            repo_path: {
              type: "string",
              description: "Absolute or relative path to the target repository. (e.g., '.' or '/path/to/repo')",
            },
            task: {
              type: "string",
              description: "The intention, task, or bug report. IMPORTANT: You MUST translate the user's intent into 5-10 English keywords that are likely to appear in the source code (functions, variables) and append them to this field before calling the tool.",
            },
            budget: {
              type: "number",
              description: "Max tokens for the compiled context. Default is 16000 if omitted.",
            },
          },
          required: ["repo_path", "task"],
        },
      },
    ],
  };
});

// 3. Lida com a execução da ferramenta chamada pela IDE
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  if (request.params.name === "compile_context") {
    const args = request.params.arguments as any;
    const repoPath = args.repo_path;
    const task = args.task;
    const budget = args.budget || 16000;

    try {
      // Verifica se o caminho do repo é válido
      if (!fs.existsSync(repoPath)) {
        return {
          content: [
            {
              type: "text",
              text: `Error: The repository path '${repoPath}' does not exist.`,
            },
          ],
          isError: true,
        };
      }

      // Chama o binário do Context Compiler
      await execAsync(ctxcBinary, [
        "dev",
        "--repo",
        repoPath,
        "--task",
        "modify_code",
        "--goal",
        task,
        "--budget",
        String(budget),
      ]);

      // Lê o resultado otimizado no subdiretório .ctxc/
      const compiledPath = path.join(repoPath, ".ctxc", "compiled-context.md");
      if (!fs.existsSync(compiledPath)) {
         return {
          content: [{ type: "text", text: `Error: Context Compiler ran but failed to generate ${compiledPath}` }],
          isError: true,
        };
      }
      
      const result = fs.readFileSync(compiledPath, "utf-8");

      // Retorna o markdown super otimizado como resposta da tool
      return {
        content: [
          {
            type: "text",
            text: result,
          },
        ],
      };
    } catch (error: any) {
      return {
        content: [
          {
            type: "text",
            text: `Tool Execution Error:\n${error.message}\nStderr:\n${error.stderr || "None"}`,
          },
        ],
        isError: true,
      };
    }
  }

  throw new Error("Tool not found");
});

// 4. Inicia a comunicação
async function run() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
}

run().catch((error) => {
  console.error("Fatal error running MCP Server:", error);
  process.exit(1);
});