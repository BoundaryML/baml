import { describe, test, expect, beforeAll, afterAll, afterEach } from 'vitest'
import { chromium, Browser, Page } from 'playwright'
import { spawn, ChildProcess, execSync } from 'child_process'
import { readFileSync, writeFileSync } from 'fs'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'node:url'

// Path constants
const projectRoot = dirname(fileURLToPath(import.meta.url)).replace('/src', '')
const playgroundDir = resolve(projectRoot, '../pkg-playground')
const wasmSourceDir = resolve(projectRoot, '../../baml_language')
const hotReloadSourcePath = resolve(wasmSourceDir, 'crates/bridge_wasm/src/lib.rs')

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
 * Uses a random port between 4900 and 4999.
 */
async function startDevServer(): Promise<DevServer> {
  // Use --force to re-optimize deps and avoid 504 "Outdated Optimize Dep" errors
  // Use a random port between 4900 and 4999 to avoid conflicts
  // Disable strictPort so Vite will try next available port if chosen port is in use
  const randomPort = Math.floor(Math.random() * 100) + 4900
  console.log(`[vite] Starting dev server in ${projectRoot} on port ${randomPort}`)
  const proc = spawn('pnpm', ['dev', '--force', '--port', String(randomPort), '--strictPort', 'false'], {
    cwd: projectRoot,
    stdio: ['pipe', 'pipe', 'pipe'],
    shell: true,
    env: { ...process.env, NO_COLOR: '1' },
  })

  // Collect output and parse port
  let output = ''
  let port: number | null = null

  const portPromise = new Promise<number>((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error(`Timeout waiting for Vite to start.\nOutput: ${output}`))
    }, 30_000)

    const handler = (data: Buffer) => {
      const text = data.toString()
      output += text
      if (process.env.DEBUG_HMR) {
        process.stdout.write(`[vite] ${text}`)
      }

      // Parse port from Vite output: "Local: http://localhost:XXXX/"
      // Match against accumulated output in case the line arrives in multiple chunks
      if (!port) {
        const match = output.match(/Local:\s*http:\/\/localhost:(\d+)/)
        if (match) {
          port = parseInt(match[1], 10)
          console.log(`[vite] Dev server running on port ${port}`)
          clearTimeout(timeout)
          resolve(port)
        }
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
      if (code !== 0 && !port) {
        reject(new Error(`Vite exited with code ${code}\nOutput: ${output}`))
      }
    })
  })

  try {
    const resolvedPort = await portPromise
    return { proc, port: resolvedPort }
  } catch (err) {
    proc.kill()
    throw err
  }
}

/**
 * Rebuild WASM directly using wasm-pack.
 * This is synchronous - it blocks until the build completes.
 */
function rebuildWasm(): void {
  console.log('[wasm] Rebuilding WASM...')
  execSync('pnpm build:wasm', {
    cwd: playgroundDir,
    stdio: 'inherit',
  })
  console.log('[wasm] WASM rebuild complete')
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
  const startTime = Date.now()
  const logInterval = setInterval(async () => {
    const elapsed = Math.round((Date.now() - startTime) / 1000)
    const currentText = await getHotReloadText(page).catch(() => null)
    console.log(`[${elapsed}s] Waiting for "${text}", current: "${currentText}"`)
  }, 10_000)

  try {
    await page.waitForFunction(
      (expectedText) => {
        const el = document.querySelector('[data-testid="hot-reload-test"]')
        return el?.textContent?.includes(expectedText)
      },
      text,
      { timeout: timeoutMs }
    )
  } finally {
    clearInterval(logInterval)
  }
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
  let originalFileContent: string | null = null
  const processes: ChildProcess[] = []

  // Cleanup handler for unexpected termination
  const cleanup = () => {
    processes.forEach((p) => {
      if (!p.killed) {
        p.kill('SIGKILL')
      }
    })
  }
  process.on('SIGINT', cleanup)
  process.on('SIGTERM', cleanup)
  process.on('exit', cleanup)

  beforeAll(async () => {
    browser = await chromium.launch({ headless: true })
  }, 30_000)

  afterAll(async () => {
    // Restore file if it was modified
    if (originalFileContent) {
      writeFileSync(hotReloadSourcePath, originalFileContent, 'utf8')
      rebuildWasm()
    }

    await browser?.close()
    await Promise.all(processes.map(killProcess))
  })

  afterEach(async () => {
    await page?.close()
  })

  test('initial page shows known good WASM content, then detects hot reload changes', async () => {
    // Step 1: Start dev server and verify the known good content is present
    const devServer = await startDevServer()
    processes.push(devServer.proc)

    page = await browser.newPage()

    // Capture browser console output
    page.on('console', (msg) => {
      console.log(`[browser ${msg.type()}] ${msg.text()}`)
    })
    page.on('pageerror', (err) => {
      console.log(`[browser error] ${err.message}`)
    })

    await page.goto(`http://localhost:${devServer.port}`)

    const pageContent = await page.content()
    console.log('[initial load] Page content:\n', pageContent)

    // Wait for React to render (root div should have children)
    console.log('[waiting] Waiting for React to mount...')
    await page.waitForFunction(
      () => {
        const root = document.getElementById('root')
        return root && root.children.length > 0
      },
      { timeout: 30_000 }
    )
    console.log('[ready] React has mounted')

    await waitForHotReloadText(page, KNOWN_GOOD_STRING)
    const initialText = await getHotReloadText(page)
    expect(initialText).toBe(KNOWN_GOOD_STRING)

    // Step 2: Edit the file to change the hot-reload marker
    originalFileContent = readFileSync(hotReloadSourcePath, 'utf8')
    const modified = originalFileContent.replace(KNOWN_GOOD_STRING, MODIFIED_STRING)
    writeFileSync(hotReloadSourcePath, modified, 'utf8')

    // Rebuild WASM directly (blocks until complete)
    rebuildWasm()

    // Verify the modified text appears via HMR (no page reload)
    await waitForHotReloadText(page, MODIFIED_STRING)
    const modifiedText = await getHotReloadText(page)
    expect(modifiedText).toBe(MODIFIED_STRING)

    await killProcess(devServer.proc)
  }, 180_000)
})
