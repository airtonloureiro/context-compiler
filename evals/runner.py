import os
import json
import subprocess
import glob

# Ensure cases exist
CASES_DIR = "evals/contextbench/cases"
os.makedirs(CASES_DIR, exist_ok=True)

# Generate 8 real-world mock cases based on D-018
if not glob.glob(f"{CASES_DIR}/*.json"):
    minibench_cases = [
        ("TS import error", "debug_error", "Resolver TS2307: Cannot find module '@core/api'", ["import {", "api"]),
        ("Docker build", "debug_error", "Erro ao compilar Dockerfile passo 4: RUN npm install", ["RUN npm", "Dockerfile"]),
        ("Prisma error", "debug_error", "Prisma Client falhou: Error: P2002 Unique constraint failed", ["@prisma/client", "schema.prisma"]),
        ("modify service", "modify_code", "Adicionar validação de e-mail no UserService", ["class UserService", "email"]),
        ("refactor", "modify_code", "Extrair lógica de DB para UserRepository", ["class UserRepository", "db"]),
        ("explain module", "explain_code", "Explicar como o AuthModule funciona e gerencia tokens", ["class AuthModule", "token"]),
        ("review diff", "architecture_review", "Fazer code review nas mudanças de arquitetura do último PR", ["diff", "architecture"]),
        ("add test", "modify_code", "Adicionar testes unitários para a função de pagamento", ["describe", "payment"])
    ]
    
    for i, (name, task_type, goal, must_preserve) in enumerate(minibench_cases, 1):
        case_data = {
            "id": f"case_minibench_{i:03d}",
            "category": task_type,
            "input": {
                "task": {
                    "type": task_type,
                    "goal": goal
                },
                "target": {
                    "provider": "openai",
                    "token_budget": 500
                },
                "context_items": [
                    {
                        "id": "ctx_001",
                        "type": "message",
                        "role": "user",
                        "content": goal
                    },
                    {
                        "id": "ctx_002",
                        "type": "file",
                        "content": f"{must_preserve[0]} dummy code {must_preserve[1]};\n" * 500  # Dummy large context
                    },
                    {
                        "id": "ctx_003",
                        "type": "log",
                        "content": f"Log relevant: {must_preserve[1]}"
                    }
                ]
            },
            "expected": {
                "must_preserve": must_preserve
            }
        }
        with open(f"{CASES_DIR}/case_minibench_{i:03d}.json", "w") as f:
            json.dump(case_data, f, indent=2)

def run_case(filepath):
    with open(filepath, 'r') as f:
        case = json.load(f)
    
    # Save input for compiler
    tmp_input = "evals/contextbench/tmp_input.json"
    with open(tmp_input, "w") as f:
        json.dump(case["input"], f)
    
    # Run Context Compiler
    cmd = [
        "cargo", "run", "--quiet", "--", "compile", 
        "--input", tmp_input, 
        "--budget", str(case["input"]["target"]["token_budget"])
    ]
    subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    
    # Check outputs
    report_file = ".ctxc/token-report.json"
    compiled_file = ".ctxc/compiled-context.md"
    
    reduction = 0.0
    if os.path.exists(report_file):
        with open(report_file, 'r') as f:
            rep = json.load(f)
            reduction = rep.get("reduction_ratio", 0.0)
            
    compiled_content = ""
    if os.path.exists(compiled_file):
        with open(compiled_file, 'r') as f:
            compiled_content = f.read()
            
    # Check fact preservation
    facts_preserved = 0
    total_facts = len(case["expected"]["must_preserve"])
    for fact in case["expected"]["must_preserve"]:
        # case insensitive simple check
        if fact.lower() in compiled_content.lower():
            facts_preserved += 1
            
    fact_score = (facts_preserved / total_facts) if total_facts > 0 else 1.0
    
    return {
        "id": case["id"],
        "category": case["category"],
        "reduction": reduction,
        "fact_score": fact_score
    }

print("Running ContextBench Evals...")
results = []
for case_file in sorted(glob.glob(f"{CASES_DIR}/*.json")):
    res = run_case(case_file)
    results.append(res)

# Generate EVAL_REPORT.md
report_path = "EVAL_REPORT.md"
with open(report_path, "w") as f:
    f.write("# ContextBench Evaluation Report\n\n")
    f.write("| Case ID | Category | Token Reduction | Critical Facts Preserved |\n")
    f.write("|---------|----------|-----------------|--------------------------|\n")
    
    total_red = 0
    total_facts = 0
    for r in results:
        red_pct = r['reduction'] * 100
        fact_pct = r['fact_score'] * 100
        total_red += red_pct
        total_facts += fact_pct
        f.write(f"| {r['id']} | {r['category']} | {red_pct:.1f}% | {fact_pct:.1f}% |\n")
        
    avg_red = total_red / len(results) if results else 0
    avg_facts = total_facts / len(results) if results else 0
    
    f.write(f"| **AVERAGE** | - | **{avg_red:.1f}%** | **{avg_facts:.1f}%** |\n")

print(f"Evaluation complete. Report generated at {report_path}")
