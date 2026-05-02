import subprocess
import os

repo = "/Users/airkun/Downloads/context-compiler/evals/fixtures/zod"
file_rel = "packages/zod/src/v3/types.ts"
file_path = os.path.join(repo, file_rel)

with open(file_path, 'r') as f:
    orig_len = len(f.read())

print(f"Original length: {orig_len}")

subprocess.run(["/Users/airkun/Downloads/context-compiler/target/debug/ctxc", "compile", "--task", "eval", "--repo", repo], capture_output=True)

compiled_path = os.path.join(repo, ".ctxc", "compiled-context.md")
with open(compiled_path, 'r') as f:
    content = f.read()
    start_marker = "## " + file_rel
    start_idx = content.find(start_marker)
    if start_idx != -1:
        end_idx = content.find("---", start_idx + len(start_marker))
        block = content[start_idx:end_idx]
        print(f"Skeletonized length: {len(block)}")
    else:
        print("Block not found")
