import os
import json
import subprocess
import time

CTXC_BIN = "./target/release/ctxc"
REPO_PATH = ".tmp/otclientv8"

# Ensures the repo exists
if not os.path.exists(REPO_PATH):
    os.makedirs(".tmp", exist_ok=True)
    print("Clonando OTCv8...")
    subprocess.run(["git", "clone", "--depth", "1", "https://github.com/OTCv8/otclientv8", REPO_PATH])

tasks = [
    ("debug_error", "Fix the segmentation fault when loading the UI elements in the main menu."),
    ("explain_code", "Explain how the networking protocol and packet encryption is handled in this client."),
    ("modify_code", "Add a new bot macro feature to auto-heal when health is below 50%."),
    ("architecture_review", "Review the overall C++ architecture and Lua binding system.")
]

print("Iniciando Benchmark no repositório OTCv8...")
results = []
start_time_total = time.time()

for idx, (task_type, desc) in enumerate(tasks, 1):
    print(f"[{idx}/4] Analisando: {desc}")
    start_time = time.time()
    cmd = [
        CTXC_BIN, "dev", 
        "--repo", REPO_PATH, 
        "--task", task_type,
        "--goal", desc, 
        "--budget", "16000"
    ]
    res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    elapsed = time.time() - start_time
    
    actual_token_file = ".ctxc/token-report.json"
    if os.path.exists(actual_token_file):
        with open(actual_token_file, "r") as f:
            rep = json.load(f)
            tokens_before = rep.get("before", 0)
            tokens_after = rep.get("after", 0)
            reduction = rep.get("reduction_ratio", 0.0) * 100
    else:
        tokens_before, tokens_after, reduction = 0, 0, 0.0

    status = "SUCCESS" if res.returncode == 0 else "ERROR/BUDGET"
    if status != "SUCCESS":
        print(f"  ❌ Erro: {res.stderr.decode('utf-8')[:200]}")
        
    print(f"  ↳ Tempo: {elapsed:.2f}s | Tokens Brutos: {tokens_before:,} | Status: {status}")
    
    # Extrair uma amostra qualitativa do contexto gerado
    snippet = "*Nenhum contexto gerado*"
    compiled_context_file = ".ctxc/compiled-context.md"
    if status == "SUCCESS" and os.path.exists(compiled_context_file):
        with open(compiled_context_file, "r", encoding="utf-8") as f:
            lines = f.readlines()
            # Pegar as primeiras 25 linhas reais (ignorando headers longos se houver)
            preview = "".join(lines[:25])
            snippet = preview.strip() + "\n... (truncado para o relatório) ..."
            
    results.append({
        "task": task_type,
        "goal": desc,
        "tokens_before": tokens_before,
        "tokens_after": tokens_after,
        "reduction": reduction,
        "time": elapsed,
        "status": status,
        "snippet": snippet
    })

total_time = time.time() - start_time_total

report_path = "OTC_BENCHMARK.md"
with open(report_path, "w", encoding="utf-8") as f:
    f.write(f"# Benchmark de Estresse: OTCv8\n\n")
    f.write(f"**Tempo Total (4 requisições intensas):** {total_time:.2f}s\n\n")
    
    f.write("## Métricas Quantitativas\n\n")
    f.write("| Task Type | Status | Tokens Originais | Tokens Finais | Compressão | Latência |\n")
    f.write("|-----------|--------|------------------|---------------|------------|----------|\n")
    for r in results:
        f.write(f"| {r['task']} | {r['status']} | {r['tokens_before']:,} | {r['tokens_after']:,} | {r['reduction']:.2f}% | {r['time']:.2f}s |\n")

    f.write("\n## Análise Qualitativa (Amostras de Contexto)\n\n")
    f.write("Abaixo estão as primeiras linhas do contexto gerado para cada task. Isso permite validar se a engine está extraindo os arquivos corretos para a tarefa solicitada.\n\n")
    
    for r in results:
        f.write(f"### Task: `{r['task']}`\n")
        f.write(f"**Goal:** {r['goal']}\n\n")
        f.write("<details>\n<summary>Ver Amostra do Contexto Compilado</summary>\n\n")
        f.write("```markdown\n")
        f.write(f"{r['snippet']}\n")
        f.write("```\n")
        f.write("</details>\n\n")

print(f"\nBenchmark finalizado. Relatório gerado em: {report_path}")
