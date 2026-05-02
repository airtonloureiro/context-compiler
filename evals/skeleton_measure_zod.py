import os
import tiktoken
import json

def count_tokens(text):
    enc = tiktoken.get_encoding("cl100k_base")
    return len(enc.encode(text))

fixture_path = "/Users/airkun/Downloads/context-compiler/evals/fixtures/zod"
compiled_path = os.path.join(fixture_path, ".ctxc", "compiled-context.md")

gt_files = [
    "packages/zod/src/v4/core/schemas.ts"
]

with open(compiled_path, 'r', encoding='utf-8') as f:
    compiled_content = f.read()
    ours_tokens = count_tokens(compiled_content)

gt_tokens = 0
for gt_file in gt_files:
    gt_path = os.path.join(fixture_path, gt_file)
    if os.path.exists(gt_path):
        with open(gt_path, 'r', encoding='utf-8', errors='ignore') as f:
            gt_tokens += count_tokens(f.read())

noise_ratio = (ours_tokens - gt_tokens) / ours_tokens if ours_tokens > 0 else 0

print(f"Tokens Compiled: {ours_tokens:,}")
print(f"GT Tokens: {gt_tokens:,}")
print(f"Noise Ratio: {noise_ratio:.2%}")

raw_tokens = 2816153 # From previous EVAL_REPORT
reduction = raw_tokens / ours_tokens if ours_tokens > 0 else 1.0
print(f"Reduction vs Raw: {reduction:.2f}x")
