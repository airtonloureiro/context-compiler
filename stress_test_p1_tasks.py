import os
import json
import subprocess
import time

CTXC_BIN = "./target/release/ctxc"
REPO_PATH = ".tmp/express"

# Ensures the repo exists
if not os.path.exists(REPO_PATH):
    subprocess.run(["git", "clone", "https://github.com/expressjs/express.git", REPO_PATH])

tasks = [
    ("debug_error", "Consertar erro 'Route.get() requires a callback function but got a [object Undefined]' no router.", ["router/index.js", "router/route.js"]),
    ("explain_code", "Explicar como o express trata middlewares e a função next().", ["router/layer.js", "application.js"]),
    ("modify_code", "Adicionar validação automática de headers via um novo middleware nativo.", ["middleware", "application.js"]),
    ("architecture_review", "Revisão da arquitetura de roteamento.", ["router/index.js", "application.js"])
]

print("Iniciando Teste de Estresse Específico (Tasks Tipadas) no repositório Express (Node.js)...")
print("Isto testará as heurísticas rígidas de cada tipo de task (debug, explain, modify, arch).\n")

results = []

for idx, (task_type, desc, expected_hints) in enumerate(tasks, 1):
    print(f"[{idx}/4] Testando TaskType: {task_type}")
    print(f"Goal: {desc}")
    
    start_time = time.time()
    
    cmd = [
        CTXC_BIN, "dev", 
        "--repo", REPO_PATH, 
        "--task", task_type,
        "--goal", desc, 
        "--budget", "60000"
    ]

    res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    elapsed = time.time() - start_time

    if res.returncode != 0:
        print(f"  ❌ Erro na execução:\n{res.stderr.decode('utf-8')}")
        continue

    compiled_file = ".ctxc/compiled-context.md"
    
    if os.path.exists(compiled_file):
        with open(compiled_file, "r") as f:
            content = f.read().lower()
            
            # Simple heuristic check:
            matched_hints = 0
            for hint in expected_hints:
                if hint.lower() in content:
                    matched_hints += 1
            
            success_rate = matched_hints / len(expected_hints)
            print(f"  ✅ Concluído em {elapsed:.2f}s. Heurísticas validadas: {matched_hints}/{len(expected_hints)} encontrados.\n")
    else:
        print("  ❌ Artefato compilado não encontrado.\n")

print("Teste de estresse de Tasks Tipadas finalizado.")