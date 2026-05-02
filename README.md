# Context Compiler (ctxc)

**Context Compiler é um motor universal de otimização de contexto para LLMs.**

Ele transforma contexto bruto de aplicações, repositórios de código e logs em contexto mínimo, estruturado, validável e orientado à tarefa, pronto para ser enviado para a OpenAI, Anthropic ou modelos locais.

---

## 🚀 Quickstart (Em menos de 10 minutos)

### 1. Instalação

Como o projeto é em Rust, você pode instalá-lo e compilá-lo localmente com o Cargo:

```bash
# Clone o repositório
git clone https://github.com/seu-usuario/context-compiler.git
cd context-compiler

# Compile e instale
cargo install --path .
```

### 2. Uso Básico (Adapter de Desenvolvimento)

Se você tem um bug no seu projeto, você pode usar o adapter de desenvolvimento embutido para varrer o repositório, fatiar as funções relevantes e gerar um contexto focado:

```bash
# Execute o compiler na raiz do projeto onde está o bug
ctxc dev --repo . --task "Consertar erro no build do Prisma" --log erro.log --budget 2000
```
Isso vai criar uma pasta `.ctxc/` no seu repositório contendo o contexto final compilado (`compiled-context.md`), o relatório de perda (`loss-report.md`) e a infraestrutura interna.

### 3. Inspecionando o Resultado

Use os novos subcomandos para entender o que a engine fez:

```bash
# Veja um resumo do gasto de tokens e a saúde do contexto
ctxc inspect

# Descubra por que um arquivo específico foi descartado ou mantido
ctxc explain --target "nome-do-arquivo"
```

---

## Estrutura do Motor Universal

A CLI é dividida nos seguintes fluxos:

- `ctxc compile`: Compila contexto genérico a partir de um `context.json`. Usado por SDKs ou Gateways.
- `ctxc dev`: Usa inteligência de código (Tree-sitter) para gerar contexto para devs.
- `ctxc inspect`: Analisa o `context.ir.json` gerado para mostrar redução de tokens.
- `ctxc explain`: Analisa o `loss-report.json` para explicar descartes e truncamentos.

---

## Bootstrap para Maestri (Desenvolvimento Interno)

Este pacote também é o ponto de partida do projeto orquestrado por agentes de desenvolvimento no [Maestri](https://www.themaestri.app).

> **Princípio central**:
> ```
> Agentes constroem o produto.
> O produto não depende dos agentes para rodar.
> ```

Consulte o arquivo `BOOTSTRAP_GUIDE.md` para entender como a equipe virtual opera.

### Índice interno:
- `docs/`: Visão e especificações (v2).
- `maestri-notes/`: Relatórios de decisão, arquitetura, avaliações e benchs.
- `src/core/`: A engine universal.
- `src/adapters/`: Os domínios (ex: `development`).
