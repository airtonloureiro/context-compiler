import os
import json
import subprocess
import time

CTXC_BIN = "./target/release/ctxc"
REPO_PATH = ".tmp/express"

tasks = [
    ("debug", "Consertar erro 'Route.get() requires a callback function but got a [object Undefined]' no router."),
    ("explain", "Explicar como o express trata middlewares e a função next()."),
    ("modify", "Adicionar validação automática de headers via um novo middleware nativo."),
    ("debug", "Diagnosticar vazamento de memória quando usa res.send() com buffers muito grandes."),
    ("modify", "Implementar suporte nativo a requisições HTTP/2 push."),
    ("explain", "Como funciona o roteamento de sub-apps (app.use('/admin', adminApp)) no Express?"),
    ("debug", "Erro ao fazer parse de payload JSON grande: 'request entity too large' no body-parser interno."),
    ("modify", "Adicionar tipagem estrita (TypeScript) para Request e Response no core."),
    ("explain", "Descrever o ciclo de vida completo de uma request no Express.js."),
    ("modify", "Implementar cache de rotas ETag automático para res.json().")
]

results = []

print("Iniciando Benchmark do Context Compiler no repositório Express (Node.js)...")
start_time_total = time.time()

for idx, (task_type, desc) in enumerate(tasks, 1):
    print(f"Executando Teste {idx}/10: {desc}")
    
    start_time = time.time()
    
    cmd = [
        CTXC_BIN, "dev", 
        "--repo", REPO_PATH, 
        "--task", f"{task_type}_code" if task_type != "debug" else "debug_error",
        "--goal", desc, 
        "--budget", "2000"
    ]
    
    res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    elapsed = time.time() - start_time
    
    if res.returncode != 0:
        print(f"Erro no teste {idx}: {res.stderr.decode('utf-8')}")
        results.append({
            "id": idx,
            "task": task_type,
            "status": "ERROR",
            "time": elapsed
        })
        continue
        
    report_file = ".ctxc-dev-tmp.json" # tmp file cleaned by cli
    token_file = os.path.join(REPO_PATH, ".ctxc", "token-report.json")
    
    # Actually, the output goes to .ctxc in the current working directory, not the repo path, 
    # because compile_generic uses output_dir = Path::new(".ctxc"); which is relative to cwd.
    
    actual_token_file = ".ctxc/token-report.json"
    
    if os.path.exists(actual_token_file):
        with open(actual_token_file, "r") as f:
            rep = json.load(f)
            tokens_before = rep.get("before", 0)
            tokens_after = rep.get("after", 0)
            reduction = rep.get("reduction_ratio", 0.0) * 100
    else:
        tokens_before = 0
        tokens_after = 0
        reduction = 0.0

    results.append({
        "id": idx,
        "task": task_type,
        "tokens_before": tokens_before,
        "tokens_after": tokens_after,
        "reduction": reduction,
        "time": elapsed,
        "status": "SUCCESS"
    })

total_time = time.time() - start_time_total

# Generate report
report_path = "EXPRESS_BENCHMARK.md"
with open(report_path, "w") as f:
    f.write("# Benchmark em Repositório Real: Express.js\n\n")
    f.write(f"**Tempo total de execução (10 testes):** {total_time:.2f}s\n\n")
    f.write("| Teste | Tipo | Tokens Originais | Tokens Finais | Redução | Tempo (s) |\n")
    f.write("|-------|------|------------------|---------------|---------|-----------|\n")
    
    avg_red = 0
    avg_time = 0
    success_count = 0
    
    for r in results:
        if r["status"] == "SUCCESS":
            f.write(f"| {r['id']} | {r['task']} | {r['tokens_before']} | {r['tokens_after']} | {r['reduction']:.2f}% | {r['time']:.2f}s |\n")
            avg_red += r["reduction"]
            avg_time += r["time"]
            success_count += 1
        else:
            f.write(f"| {r['id']} | {r['task']} | ERROR | ERROR | ERROR | {r['time']:.2f}s |\n")
            
    if success_count > 0:
        avg_red /= success_count
        avg_time /= success_count
        f.write(f"| **MÉDIA** | - | - | - | **{avg_red:.2f}%** | **{avg_time:.2f}s** |\n")

print(f"Benchmark finalizado. Relatório gerado em {report_path}")
