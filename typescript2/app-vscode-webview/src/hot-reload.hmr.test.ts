import { describe, test, expect, beforeAll, afterAll, afterEach } from 'vitest'
import { chromium, Browser, Page } from 'playwright'
import { spawn, ChildProcess } from 'child_process'
import { readFileSync, writeFileSync, rmSync, existsSync } from 'fs'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'node:url'

// Path constants
const projectRoot = dirname(fileURLToPath(import.meta.url)).replace('/src', '')
const viteCacheDir = resolve(projectRoot, 'node_modules/.vite')
const playgroundDir = resolve(projectRoot, '../pkg-playground')
const wasmSourceDir = resolve(projectRoot, '../../baml_language')
const hotReloadSourcePath = resolve(wasmSourceDir, 'crates/baml_playground_wasm/src/hot_reload_testdata.rs')

// Test strings
const KNOWN_GOOD_STRING = 'injected for hot reload test, see hot-reload.hmr.test.ts'
const MODIFIED_STRING = 'MODIFIED for hot reload test, see hot-reload.hmr.test.ts'

interface DevServer {
  proc: ChildProcess
  port: number
}

/**
 * Wait for a specific string to appear in process stdout/stderr
 */
function waitForOutput(
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
      const matches = typeof match === 'string' ? text.includes(match) : match.test(text)

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
async function startDevServer(): Promise<DevServer> {
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

  return { proc, port: 4000 }
}

/**
 * Start the WASM file watcher using nodemon
 */
async function startWasmWatcher(): Promise<ChildProcess> {
  const proc = spawn(
    'npx',
    ['nodemon', '--watch', '../../baml_language', '--ext', 'rs,toml', '--exec', 'pnpm build:wasm'],
    {
      cwd: playgroundDir,
      stdio: ['ignore', 'pipe', 'pipe'],
      shell: true,
    }
  )

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
 */
async function waitForWasmRebuild(proc: ChildProcess, timeoutMs = 60_000): Promise<void> {
  await waitForOutput(proc, /\[nodemon\].*restarting due to changes/, timeoutMs / 2)
  await waitForOutput(proc, /\[nodemon\].*clean exit|Done in/, timeoutMs / 2)
}

/**
 * Kill a process and wait for it to exit
 */
function killProcess(proc: ChildProcess): Promise<void> {
  return new Promise((resolve) => {
    if (proc.killed) {
      resolve()
      return
    }

    proc.on('exit', () => resolve())
    proc.kill('SIGTERM')

    setTimeout(() => {
      if (!proc.killed) {
        proc.kill('SIGKILL')
      }
      resolve()
    }, 5000)
  })
}

/**
 * Wait for the hot reload test element to contain specific text
 */
async function waitForHotReloadText(page: Page, text: string, timeoutMs = 30_000): Promise<void> {
  await page.waitForFunction(
    (expectedText) => {
      const el = document.querySelector('[data-testid="hot-reload-test"]')
      return el?.textContent?.includes(expectedText)
    },
    text,
    { timeout: timeoutMs }
  )
}

/**
 * Get the hot reload test string from the page
 */
async function getHotReloadText(page: Page): Promise<string | null> {
  return page.evaluate(() => {
    const el = document.querySelector('[data-testid="hot-reload-test"]')
    return el?.textContent ?? null
  })
}

describe('WASM Build Pipeline', () => {
  let browser: Browser
  let page: Page
  let wasmWatcher: ChildProcess
  let originalFileContent: string | null = null
  const processes: ChildProcess[] = []

  beforeAll(async () => {
    browser = await chromium.launch({ headless: true })

    wasmWatcher = await startWasmWatcher()
    processes.push(wasmWatcher)

    // Give nodemon time to complete initial build
    await new Promise((resolve) => setTimeout(resolve, 3000))
  }, 90_000)

  afterAll(async () => {
    // Restore file if it was modified
    if (originalFileContent) {
      writeFileSync(hotReloadSourcePath, originalFileContent, 'utf8')
      await waitForWasmRebuild(wasmWatcher, 60_000).catch(() => {})
    }

    await browser?.close()
    await Promise.all(processes.map(killProcess))
  })

  afterEach(async () => {
    await page?.close()
  })

  test('initial page shows known good WASM content, then detects hot reload changes', async () => {
    // Step 1: Verify the known good content is present
    const devServer1 = await startDevServer()
    processes.push(devServer1.proc)

    page = await browser.newPage()
    await page.goto(`http://localhost:${devServer1.port}`)

    await waitForHotReloadText(page, KNOWN_GOOD_STRING)
    const initialText = await getHotReloadText(page)
    expect(initialText).toBe(KNOWN_GOOD_STRING)

    await page.close()
    await killProcess(devServer1.proc)

    // Step 2: Edit the file to change the hot-reload marker
    originalFileContent = readFileSync(hotReloadSourcePath, 'utf8')
    const modified = originalFileContent.replace(KNOWN_GOOD_STRING, MODIFIED_STRING)
    writeFileSync(hotReloadSourcePath, modified, 'utf8')

    // Wait for nodemon to detect the change and rebuild WASM
    await waitForWasmRebuild(wasmWatcher, 60_000)

    // Start a fresh dev server to pick up the new WASM
    const devServer2 = await startDevServer()
    processes.push(devServer2.proc)

    page = await browser.newPage()
    await page.goto(`http://localhost:${devServer2.port}`)

    // Verify the modified text appears
    await waitForHotReloadText(page, MODIFIED_STRING)
    const modifiedText = await getHotReloadText(page)
    expect(modifiedText).toBe(MODIFIED_STRING)

    await killProcess(devServer2.proc)
  }, 180_000)
})
