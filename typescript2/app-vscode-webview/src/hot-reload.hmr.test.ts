import { describe, test, expect, beforeAll, afterAll, afterEach } from 'vitest'
import { chromium, Browser, Page } from 'playwright'
import {
  startDevServer,
  startWasmWatcher,
  waitForWasmRebuild,
  getHotReloadSourcePath,
  editFile,
  restoreFile,
  DevServer,
  ProcessCleanup,
  killProcess,
} from '../test-utils/hmr'
import { ChildProcess } from 'child_process'

/**
 * Wait for a text to be visible on the page
 */
async function waitForText(page: Page, text: string, timeoutMs = 30_000): Promise<void> {
  await page.waitForSelector(`text=${text}`, { timeout: timeoutMs, state: 'visible' })
}

describe('WASM Build Pipeline', () => {
  let browser: Browser
  let page: Page
  let wasmWatcher: ChildProcess
  let originalFileContent: string
  const cleanup = new ProcessCleanup()

  beforeAll(async () => {
    // Start browser
    browser = await chromium.launch({
      headless: true,
    })

    // Start WASM file watcher
    wasmWatcher = await startWasmWatcher()
    cleanup.add(wasmWatcher)

    // Give nodemon time to complete initial build
    await new Promise((resolve) => setTimeout(resolve, 3000))
  }, 90_000)

  afterAll(async () => {
    // Restore file if it was modified
    if (originalFileContent) {
      const filePath = getHotReloadSourcePath()
      restoreFile(filePath, originalFileContent)
      // Trigger rebuild with original content
      await waitForWasmRebuild(wasmWatcher, 60_000).catch(() => {})
    }

    // Close browser
    await browser?.close()

    // Kill all processes
    await cleanup.cleanup()
  })

  afterEach(async () => {
    await page?.close()
  })

  test('initial page shows WASM-rendered content', async () => {
    // Start a fresh dev server for this test
    const devServer = await startDevServer()
    cleanup.add(devServer.proc)

    try {
      page = await browser.newPage()
      await page.goto(`http://localhost:${devServer.port}`)

      // Wait for the WASM content to render
      await waitForText(page, 'injected-hot-reload4')

      // Verify the full function name format
      const text = await page.locator('text=/injected-hot-reload4/').first().textContent()
      expect(text).toContain('injected-hot-reload4')
    } finally {
      await killProcess(devServer.proc)
    }
  }, 60_000)

  test('WASM rebuilds when Rust source changes', async () => {
    const filePath = getHotReloadSourcePath()

    // Edit the file to change the hot-reload marker
    const { original } = editFile(filePath, (content) =>
      content.replace('injected-hot-reload4', 'injected-hot-reload5')
    )
    originalFileContent = original

    // Wait for nodemon to detect the change and rebuild WASM
    await waitForWasmRebuild(wasmWatcher, 60_000)

    // Start a fresh dev server to pick up the new WASM
    const devServer = await startDevServer()

    try {
      page = await browser.newPage()
      await page.goto(`http://localhost:${devServer.port}`)

      // Verify the new text appears
      await waitForText(page, 'injected-hot-reload5')

      const text = await page.locator('text=/injected-hot-reload5/').first().textContent()
      expect(text).toContain('injected-hot-reload5')
    } finally {
      await killProcess(devServer.proc)
    }

    // Restore the original file content
    restoreFile(filePath, originalFileContent)
    originalFileContent = ''
  }, 120_000)
})
