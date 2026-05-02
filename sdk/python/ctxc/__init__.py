import json
import tempfile
import subprocess
import os
from typing import Dict, Any, List

class ContextCompiler:
    def __init__(self, bin_path: str = "ctxc"):
        self.bin_path = bin_path

    def compile(
        self,
        task: Dict[str, str],
        target: Dict[str, Any],
        context_items: List[Dict[str, Any]]
    ) -> Dict[str, Any]:
        """
        Compiles raw context into optimized context using the Rust core via CLI.
        """
        payload = {
            "task": task,
            "target": target,
            "context_items": context_items
        }

        with tempfile.NamedTemporaryFile("w", delete=False, suffix=".json") as tmp:
            json.dump(payload, tmp)
            tmp_path = tmp.name

        try:
            cmd = [
                self.bin_path,
                "compile",
                "--input", tmp_path,
                "--target", target.get("provider", "openai"),
                "--budget", str(target.get("token_budget", 2000))
            ]

            result = subprocess.run(cmd, capture_output=True, text=True)

            if result.returncode != 0:
                raise RuntimeError(f"ctxc failed: {result.stderr}")

            # Read artifacts
            ctxc_dir = ".ctxc"
            
            with open(os.path.join(ctxc_dir, "context.ir.json"), "r") as f:
                ir = json.load(f)
                
            with open(os.path.join(ctxc_dir, "token-report.json"), "r") as f:
                token_report = json.load(f)
                
            with open(os.path.join(ctxc_dir, "compiled-context.md"), "r") as f:
                compiled_prompt = f.read()

            return {
                "ir": ir,
                "token_report": token_report,
                "compiled_prompt": compiled_prompt
            }
        finally:
            os.remove(tmp_path)
