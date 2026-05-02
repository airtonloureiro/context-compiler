import os
import time
import subprocess
import tiktoken
from openai import OpenAI

# REQUISITOS:
# pip install openai tiktoken
# export DEEPSEEK_API_KEY="sk-..."

CTXC_BIN = "./target/release/ctxc"
REPO_PATH = ".tmp/otclientv8" # Repositório atual ou caminho para o repo de teste
TASK_DESC = "Como eu faço para criar um bot que recupera minha vida quando eu estiver apenas com 20% de vida"

client = None
MODEL = "deepseek-chat"

def estimate_tokens(text):
    try:
        # DeepSeek uses a different tokenizer, but cl100k_base is a good approximation
        enc = tiktoken.get_encoding("cl100k_base")
        return len(enc.encode(text))
    except:
        return len(text) // 4

def get_raw_context(repo_path):
    """Simula o envio de contexto bruto (ex: colando os arquivos principais na mão)"""
    print("Coletando contexto bruto (A)...")
    context = ""
    file_count = 0
    for root, _, files in os.walk(os.path.join(repo_path, "src")):
        for file in files:
            if file.endswith((".cpp", ".h", ".lua")):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, "r", encoding="utf-8") as f:
                        content = f.read()
                        if len(content) < 50000:
                            context += f"\n--- {filepath} ---\n" + content
                            file_count += 1
                except Exception:
                    pass
            if file_count >= 50:
                break
        if file_count >= 50:
            break
    
    for root, _, files in os.walk(os.path.join(repo_path, "modules")):
        for file in files:
            if file.endswith(".lua"):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, "r", encoding="utf-8") as f:
                        content = f.read()
                        if len(content) < 50000:
                            context += f"\n--- {filepath} ---\n" + content
                            file_count += 1
                except Exception:
                    pass
            if file_count >= 80:
                break
        if file_count >= 80:
            break
    
    return context

def expand_query(task):
    """Resolve o trade-off de idioma usando o LLM super rápido e barato para traduzir a task em keywords de código em inglês"""
    global client
    print("Realizando Query Expansion (Tradução para Código EN)...")
    prompt = f"Traduza a seguinte intenção do usuário para 5 palavras-chave em inglês que apareceriam em nomes de arquivos ou funções de um código-fonte (separadas por espaço). Intenção: '{task}'"
    
    try:
        response = client.chat.completions.create(
            model=MODEL,
            messages=[{"role": "user", "content": prompt}],
            temperature=0.0
        )
        keywords = response.choices[0].message.content.strip()
        print(f"  ↳ Keywords extraídas: {keywords}")
        return f"{task} {keywords}"
    except:
        return task

def get_compiled_context(repo_path, task):
    """Usa o Context Compiler para gerar o contexto otimizado (B)"""
    expanded_task = expand_query(task)
    print("Compilando contexto com CTXC (B)...")
    cmd = [
        CTXC_BIN, "dev", 
        "--repo", repo_path, 
        "--task", "explain_code",
        "--goal", expanded_task, 
        "--budget", "8000"
    ]
    subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    
    compiled_file = os.path.join(repo_path, ".ctxc", "compiled-context.md")
    with open(compiled_file, "r", encoding="utf-8") as f:
        return f.read()

def call_llm(prompt_content):
    """Chama a API da DeepSeek e retorna resposta e uso de tokens"""
    global client
    start_time = time.time()
    
    try:
        response = client.chat.completions.create(
            model=MODEL,
            messages=[
                {"role": "system", "content": "Você é um assistente de IA focado em engenharia de software."},
                {"role": "user", "content": prompt_content}
            ],
            temperature=0.0
        )
        latency = time.time() - start_time
        usage = response.usage
        
        return {
            "answer": response.choices[0].message.content,
            "input_tokens": usage.prompt_tokens,
            "output_tokens": usage.completion_tokens,
            "latency": latency,
            "success": True
        }
    except Exception as e:
        latency = time.time() - start_time
        est_tokens = estimate_tokens(prompt_content)
        return {
            "answer": f"ERRO DA API: {str(e)}",
            "input_tokens": est_tokens,
            "output_tokens": 0,
            "latency": latency,
            "success": False
        }

def main():
    global client
    if not os.environ.get("DEEPSEEK_API_KEY"):
        print("ERRO: DEEPSEEK_API_KEY não encontrada. Exporte a variável antes de rodar.")
        return
        
    # Inicializa cliente compatível com OpenAI apontando para DeepSeek
    client = OpenAI(
        api_key=os.environ.get("DEEPSEEK_API_KEY"),
        base_url="https://api.deepseek.com"
    )

    print(f"Iniciando Teste A/B...\nTarefa: '{TASK_DESC}'\nModelo: {MODEL}\n")

    # ---------------------------------------------------------
    # TESTE A: RAW CONTEXT
    # ---------------------------------------------------------
    raw_context = get_raw_context(REPO_PATH)
    prompt_a = f"Tarefa: {TASK_DESC}\n\nContexto do repositório:\n{raw_context}"
    
    print("-> Enviando Teste A (Raw) para o LLM...")
    res_a = call_llm(prompt_a)

    # ---------------------------------------------------------
    # TESTE B: COMPILED CONTEXT (Context Compiler)
    # ---------------------------------------------------------
    compiled_context = get_compiled_context(REPO_PATH, TASK_DESC)
    prompt_b = f"Tarefa: {TASK_DESC}\n\nContexto compilado:\n{compiled_context}"
    
    print("-> Enviando Teste B (Compiled) para o LLM...")
    res_b = call_llm(prompt_b)

    # ---------------------------------------------------------
    # RESULTADOS
    # ---------------------------------------------------------
    print("\n" + "="*50)
    print("🏆 RESULTADOS DO TESTE A/B 🏆")
    print("="*50)
    
    print(f"\n[A] Raw Context (Direto pro LLM):")
    if res_a.get('success', True):
        print(f"  - Input Tokens : {res_a['input_tokens']:,}")
        print(f"  - Output Tokens: {res_a['output_tokens']:,}")
        print(f"  - Latência LLM : {res_a['latency']:.2f}s")
        print(f"  - Custo Est.   : ${(res_a['input_tokens']*0.14/1000000):.5f} (base deepseek-chat)")
    else:
        print(f"  - FAILED. Input Tokens estimados em {res_a['input_tokens']:,} (Excedeu contexto)")

    print(f"\n[B] Context Compiler (Otimizado):")
    if res_b.get('success', True):
        print(f"  - Input Tokens : {res_b['input_tokens']:,}")
        print(f"  - Output Tokens: {res_b['output_tokens']:,}")
        print(f"  - Latência LLM : {res_b['latency']:.2f}s")
        print(f"  - Custo Est.   : ${(res_b['input_tokens']*0.14/1000000):.5f} (base deepseek-chat)")
    else:
        print(f"  - FAILED. Input Tokens estimados em {res_b['input_tokens']:,}")
    
    if res_a['input_tokens'] > 0:
        economia = 100 - (res_b['input_tokens'] / res_a['input_tokens'] * 100)
        print(f"\n📈 Economia de Tokens/Custo: {economia:.2f}%")
    else:
        print("\n📈 Não foi possível calcular economia.")
    
    print("\n--- Amostra da Resposta (A) ---")
    print(res_a['answer'][:300] + "...\n")
    
    print("--- Amostra da Resposta (B) ---")
    print(res_b['answer'][:300] + "...\n")

if __name__ == "__main__":
    main()