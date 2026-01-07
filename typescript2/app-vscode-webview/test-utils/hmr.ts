import { spawn, ChildProcess } from 'child_process'
import { readFileSync, writeFileSync, rmSync, existsSync } from 'fs'
import { resolve } from 'path'
import { dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const projectRoot = dirname(fileURLToPath(import.meta.url)).replace('/test-utils', '')
const viteCacheDir = resolve(projectRoot, 'node_modules/.vite')
const playgroundDir = resolve(projectRoot, '../pkg-playground')
const wasmSourceDir = resolve(projectRoot, '../../baml_language')

export interface DevServer {
  proc: ChildProcess
  port: number
  kill: () => void
}

/**
 * Wait for a specific string to appear in process stdout/stderr
 */
export function waitForOutput(
  proc: ChildProcess,
  match: string | RegExp,
  timeoutMs = 30_000
): Promise<void> {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error(`Timeout waiting for output: ${match}`))
    }, timeoutMs)

    const handler = (data: Buffer) => {
      const text = data.toString()
      const matches = typeof match === 'string'
        ? text.includes(match)
        : match.test(text)

      if (matches) {
        clearTimeout(timeout)
        proc.stdout?.off('data', handler)
        proc.stderr?.off('data', handler)
        resolve()
      }
    }

    proc.stdout?.on('data', handler)
    proc.stderr?.on('data', handler)

    proc.on('error', (err) => {
      clearTimeout(timeout)
      reject(err)
    })

    proc.on('exit', (code) => {
      clearTimeout(timeout)
      if (code !== 0) {
        reject(new Error(`Process exited with code ${code}`))
      }
    })
  })
}

/**
 * Start the Vite dev server and wait for it to be ready.
 * Clears Vite's cache before starting to ensure fresh modules.
 */
export async function startDevServer(): Promise<DevServer> {
  // Clear Vite's dependency cache to ensure fresh WASM is loaded
  if (existsSync(viteCacheDir)) {
    rmSync(viteCacheDir, { recursive: true, force: true })
  }

  const proc = spawn('pnpm', ['dev'], {
    cwd: projectRoot,
    stdio: ['pipe', 'pipe', 'pipe'],
    shell: true,
  })

  // Collect output for debugging
  let output = ''
  proc.stdout?.on('data', (data) => {
    output += data.toString()
    if (process.env.DEBUG_HMR) {
      process.stdout.write(`[vite] ${data}`)
    }
  })
  proc.stderr?.on('data', (data) => {
    output += data.toString()
    if (process.env.DEBUG_HMR) {
      process.stderr.write(`[vite:err] ${data}`)
    }
  })

  try {
    await waitForOutput(proc, /ready in|Local:.*http/, 30_000)
  } catch (err) {
    proc.kill()
    throw new Error(`Failed to start Vite dev server.\nOutput: ${output}\n${err}`)
  }

  return {
    proc,
    port: 4000,
    kill: () => {
      proc.kill('SIGTERM')
      // Force kill after 5 seconds if still running
      setTimeout(() => {
        if (!proc.killed) {
          proc.kill('SIGKILL')
        }
      }, 5000)
    },
  }
}

/**
 * Start the WASM file watcher using nodemon
 */
export async function startWasmWatcher(): Promise<ChildProcess> {
  const proc = spawn(
    'npx',
    [
      'nodemon',
      '--watch', '../../baml_language',
      '--ext', 'rs,toml',
      '--exec', 'pnpm build:wasm',
    ],
    {
      cwd: playgroundDir,
      stdio: ['ignore', 'pipe', 'pipe'],
      shell: true,
    }
  )

  // Collect output for debugging
  proc.stdout?.on('data', (data) => {
    if (process.env.DEBUG_HMR) {
      process.stdout.write(`[nodemon] ${data}`)
    }
  })
  proc.stderr?.on('data', (data) => {
    if (process.env.DEBUG_HMR) {
      process.stderr.write(`[nodemon:err] ${data}`)
    }
  })

  // Wait for nodemon to start (it prints "[nodemon] starting...")
  try {
    await waitForOutput(proc, /\[nodemon\].*starting|watching path/, 15_000)
  } catch (err) {
    proc.kill()
    throw new Error(`Failed to start nodemon watcher: ${err}`)
  }

  return proc
}

/**
 * Wait for WASM rebuild to complete by watching nodemon output.
 * First waits for nodemon to detect the change and start rebuilding,
 * then waits for the build to complete.
 */
export async function waitForWasmRebuild(
  proc: ChildProcess,
  timeoutMs = 60_000
): Promise<void> {
  // First wait for nodemon to detect the change
  await waitForOutput(proc, /\[nodemon\].*restarting due to changes/, timeoutMs / 2)
  // Then wait for the build to complete
  await waitForOutput(proc, /\[nodemon\].*clean exit|Done in/, timeoutMs / 2)
}

/**
 * Get the path to the Rust lib.rs file that contains the hot-reload marker
 */
export function getHotReloadSourcePath(): string {
  return resolve(wasmSourceDir, 'crates/baml_playground_wasm/src/lib.rs')
}

/**
 * Edit a file using a replacer function
 */
export function editFile(
  filePath: string,
  replacer: (content: string) => string
): { original: string; modified: string } {
  const original = readFileSync(filePath, 'utf8')
  const modified = replacer(original)
  writeFileSync(filePath, modified, 'utf8')
  return { original, modified }
}

/**
 * Restore a file to its original content
 */
export function restoreFile(filePath: string, content: string): void {
  writeFileSync(filePath, content, 'utf8')
}

/**
 * Kill a process and all its children
 */
export function killProcess(proc: ChildProcess): Promise<void> {
  return new Promise((resolve) => {
    if (proc.killed) {
      resolve()
      return
    }

    proc.on('exit', () => resolve())
    proc.kill('SIGTERM')

    // Force kill after timeout
    setTimeout(() => {
      if (!proc.killed) {
        proc.kill('SIGKILL')
      }
      resolve()
    }, 5000)
  })
}

/**
 * Cleanup helper that ensures all processes are killed
 */
export class ProcessCleanup {
  private processes: ChildProcess[] = []

  add(proc: ChildProcess): void {
    this.processes.push(proc)
  }

  async cleanup(): Promise<void> {
    await Promise.all(this.processes.map(killProcess))
    this.processes = []
  }
}
