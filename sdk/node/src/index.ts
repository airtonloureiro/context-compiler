import { spawn } from 'child_process';
import { resolve } from 'path';

export interface CompileOptions {
  task: {
    type: string;
    goal?: string;
    user_request?: string;
  };
  target: {
    provider: string;
    model?: string;
    token_budget: number;
  };
  context_items: Array<{
    id: string;
    type: string;
    role?: string;
    content: string;
    source?: any;
    metadata?: any;
    sensitivity?: 'public' | 'internal' | 'confidential' | 'secret';
  }>;
}

export class ContextCompiler {
  private binPath: string;

  constructor(binPath?: string) {
    this.binPath = binPath || 'ctxc';
  }

  /**
   * Compiles raw context into optimized context using the Rust core via CLI.
   * In a production scenario, this could use FFI/NAPI or HTTP.
   */
  public async compile(options: CompileOptions): Promise<any> {
    const tmpInput = resolve(process.cwd(), '.ctxc-tmp-input.json');
    const fs = await import('fs/promises');
    await fs.writeFile(tmpInput, JSON.stringify(options, null, 2));

    return new Promise((resolveResult, reject) => {
      const child = spawn(this.binPath, [
        'compile',
        '--input', tmpInput,
        '--target', options.target.provider,
        '--budget', options.target.token_budget.toString()
      ]);

      let stdout = '';
      let stderr = '';

      child.stdout.on('data', (data) => stdout += data.toString());
      child.stderr.on('data', (data) => stderr += data.toString());

      child.on('close', async (code) => {
        // Cleanup tmp file
        await fs.unlink(tmpInput).catch(() => {});
        
        if (code === 0) {
          // Read artifacts generated in .ctxc folder
          try {
            const irFile = await fs.readFile('.ctxc/context.ir.json', 'utf-8');
            const tokenFile = await fs.readFile('.ctxc/token-report.json', 'utf-8');
            const compiledFile = await fs.readFile('.ctxc/compiled-context.md', 'utf-8');
            
            resolveResult({
              ir: JSON.parse(irFile),
              token_report: JSON.parse(tokenFile),
              compiled_prompt: compiledFile,
            });
          } catch (e) {
            reject(new Error(`Failed to read artifacts: ${e}`));
          }
        } else {
          reject(new Error(`ctxc failed with code ${code}: ${stderr}`));
        }
      });
    });
  }
}
