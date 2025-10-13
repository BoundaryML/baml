/* eslint-disable @typescript-eslint/no-misused-promises */

import axios from 'axios'
import cors from 'cors'
import { createProxyMiddleware } from 'http-proxy-middleware'
import * as vscode from 'vscode'
import glooLens from './LanguageToBamlCodeLensProvider'
import { WebviewPanelHost } from './panels/WebviewPanelHost'
import plugins from './plugins'
import {
  publishBamlVersionReport,
  requestDiagnostics,
  telemetry,
} from './plugins/language-server-client'

import type { Express } from 'express'
import { Socket } from 'net'
import StatusBarPanel from './panels/StatusBarPanel'
import { Server } from 'http'

const outputChannel = vscode.window.createOutputChannel('baml')
const diagnosticsCollection =
  vscode.languages.createDiagnosticCollection('baml-diagnostics')

let server: Server | null = null
let glowOnDecoration: vscode.TextEditorDecorationType | null = null
let glowOffDecoration: vscode.TextEditorDecorationType | null = null
let isGlowOn: boolean = true
let animationTimer: NodeJS.Timeout | null = null
let highlightRanges: vscode.Range[] = []

export function activate(context: vscode.ExtensionContext) {
  console.log('BAML extension activating')

  vscode.workspace.getConfiguration('baml')

  context.subscriptions.push(StatusBarPanel.instance)

  // Initialize the highlight effect.
  createDecorations()
  startAnimation()

  const app: Express = require('express')()
  app.use(cors())
  server = app.listen(0, () => {
    console.log('Server started on port ' + getPort())
  })

  const getPort = () => {
    const addr = server?.address() || null
    if (addr === null) {
      vscode.window.showErrorMessage(
        'Failed to start BAML extension server. Please try reloading the window, or restarting VSCode.',
      )
      console.error(
        'Failed to start BAML extension server. Please try reloading the window, or restarting VSCode.',
      )
      return 0
    }
    if (typeof addr === 'string') {
      return parseInt(addr)
    }
    return addr.port
  }

  app.use(
    createProxyMiddleware({
      changeOrigin: true, // leave prependPath = true (default)
      /** Inspect and (maybe) rewrite the path. */
      pathRewrite: (path, req) => {
        console.log('[PROXY] pathRewrite input:', path)

        // If the request clearly targets a static image asset (e.g. '.png', '.jpg' …)
        // and it’s a simple GET, we blank the path so the webview loads it from
        // its own origin.  The previous implementation treated ANY dotted suffix
        // as an “image”, which broke legitimate paths like “/pdf/2305.08675”.

        // If the path looks like an image (xyz.png …) and it's a GET → blank it.
        // if (/\.[a-z0-9]+$/i.test(path) && req.method === 'GET') {

        const imageExtPattern = /\.(png|jpe?g|gif|bmp|webp|svg)$/i
        if (imageExtPattern.test(path) && req.method === 'GET') {
          console.log('[PROXY] Image request detected, clearing path:', path)
          return ''
        }

        // Remove trailing slash so we don't end up with '//'.
        const out = path.endsWith('/') ? path.slice(0, -1) : path
        console.log('[PROXY] pathRewrite output:', out)
        return out
      },

      /** Dynamically choose target and massage req.url. */
      router: (req) => {
        const raw = req.headers['baml-original-url']
        if (typeof raw !== 'string') {
          throw new Error('missing baml-original-url header')
        }

        // Clean up headers the upstream may reject
        delete req.headers['baml-original-url']
        delete req.headers['origin']

        // Strip trailing slash on header value, then parse
        const cleanRaw = raw.endsWith('/') ? raw.slice(0, -1) : raw
        const url = new URL(cleanRaw)

        // Base path to prepend *if necessary*
        const basePath = url.pathname.replace(/\/$/, ''); // '/compat/v1' → '/compat/v1'
        if (!req.url) {
          throw new Error('missing req.url')
        }

        // Guard against double-prefixing
        if (basePath && !req.url.startsWith(basePath)) {
          // Ensure there's exactly one slash between basePath and existing path
          req.url = basePath + (req.url.startsWith('/') ? '' : '/') + req.url
        }

        // Append query parameters from the original URL if they exist
        if (url.search) {
          req.url = req.url.split('?')[0] + url.search
        }

        console.log('[PROXY]', req.method, req.url, '→', url.origin)
        if (req.url?.includes('?')) {
          console.log('[PROXY] Query params detected in request:', req.url)
        }

        // Tell HPM to proxy to the origin only (scheme + host)
        return url.origin; // e.g. 'https://api.llama.com'
      },

      logger: console,

      on: {
        /** Add CORS header. */
        proxyRes: (proxyRes, req) => {
          proxyRes.headers['access-control-allow-origin'] = '*'
          console.log('[PROXY]', req.method, req.url, '←', proxyRes.statusCode)
        },

        /** Robust error reporter with type-guard. */
        error: (err, req, res) => {
          console.error('[PROXY ERROR]', req.method, req.url, ':', err.message)

          if ('writeHead' in res) {
            const svr = res
            if (!svr.headersSent) {
              svr.writeHead(500, { 'content-type': 'application/json' })
            }
            svr.end(JSON.stringify({ error: err.message }))
          } else if (res instanceof Socket) {
            res.destroy()
          }
        },
      },
    }),
  )

  const bamlPlaygroundCommand = vscode.commands.registerCommand(
    'baml.openBamlPanel',
    (args?: { projectId: string; functionName: string }) => {
      const config = vscode.workspace.getConfiguration()
      config.update(
        'baml.bamlPanelOpen',
        true,
        vscode.ConfigurationTarget.Global,
      )

      console.info('context.extensionUri', context.extensionUri)
      WebviewPanelHost.render(context.extensionUri, getPort, telemetry)
      if (telemetry) {
        telemetry.sendTelemetryEvent({
          event: 'baml.openBamlPanel',
          properties: {},
        })
      }
      // sends project files as well to webview
      requestDiagnostics()

      if (!args) return

      WebviewPanelHost.currentPanel?.sendCommandToWebview({
        source: 'lsp_message',
        payload: {
          method: 'workspace/executeCommand',
          params: {
            command: 'baml.openBamlPanel',
            arguments: [args],
          },
        }
      })
    },
  )

  const bamlTestcaseCommand = vscode.commands.registerCommand(
    'baml.runBamlTest',
    (args?: {
      functionName: string
      testCaseName: string
    }) => {
      WebviewPanelHost.render(context.extensionUri, getPort, telemetry)
      if (telemetry) {
        telemetry.sendTelemetryEvent({
          event: 'baml.runBamlTest',
          properties: {},
        })
      }

      // sends project files as well to webview
      requestDiagnostics()

      if (!args) return

      WebviewPanelHost.currentPanel?.sendCommandToWebview({
        source: 'lsp_message',
        payload: {
          method: 'workspace/executeCommand',
          params: {
            command: 'baml.runBamlTest',
            arguments: [args]
          },
        }
      })
    },
  )

  const bamlSetFlashingRegionsCommand = vscode.commands.registerCommand(
    'baml.setFlashingRegions',
    (params: {
      content: {
        spans: {
          file_path: string
          start_line: number
          start: number
          end_line: number
          end: number
        }[]
      }
    }) => {
      // A helpful thing to toggle on for debugging:
      console.info('HANDLER setFlashingRegions', params)
      // vscode.window.showWarningMessage(`setFlashingRegions:` + JSON.stringify(params))

      // Focus the editor to ensure styling updates are applied rapidly.
      if (vscode.window.activeTextEditor) {
        vscode.window.showTextDocument(
          vscode.window.activeTextEditor.document,
          vscode.window.activeTextEditor.viewColumn,
        )
      }

      context.subscriptions.push({
        dispose: () => {
          stopAnimation()
          if (glowOnDecoration) glowOnDecoration.dispose()
          if (glowOffDecoration) glowOffDecoration.dispose()
        },
      })
      const ranges = params.content.spans.map((span) => {
        const start = new vscode.Position(span.start_line, span.start)
        const end = new vscode.Position(span.end_line, span.end)
        return new vscode.Range(start, end)
      })
      highlightRanges = ranges
      updateHighlight()
    },
  )

  if (bamlPlaygroundCommand) {
    context.subscriptions.push(bamlPlaygroundCommand)
  }
  context.subscriptions.push(bamlTestcaseCommand)
  context.subscriptions.push(bamlSetFlashingRegionsCommand)

  const pythonSelector = { language: 'python', scheme: 'file' }
  const typescriptSelector = { language: 'typescript', scheme: 'file' }
  const reactSelector = { language: 'typescriptreact', scheme: 'file' }

  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider(pythonSelector, glooLens),
    vscode.languages.registerCodeLensProvider(typescriptSelector, glooLens),
    vscode.languages.registerCodeLensProvider(reactSelector, glooLens),
  )

  context.subscriptions.push(diagnosticsCollection)

  vscode.window.onDidChangeActiveTextEditor((event) => {
    // makes it so we reload the project. Could probably be called reloadProjectFiles or something. This is because we may be clicking into a different file in a separate baml_src.
    requestDiagnostics()
  })

  // Add cursor movement listener
  vscode.window.onDidChangeTextEditorSelection((event) => {
    const position = event.selections[0]?.active

    const editor = vscode.window.activeTextEditor
    if (!editor) { return; }

    const name = editor.document.fileName
    if (!name.endsWith('.baml')) {
      return
    }

    // TODO: buggy when used with multiple functions, needs a fix.
    WebviewPanelHost.currentPanel?.sendCommandToWebview({
      source: 'ide_message',
      payload: {
        command: 'update_cursor',
        content: {
          fileName: name,
          line: position?.line ?? 0,
          column: position?.character ?? 0,
        },
      }
    })
  })

  const config = vscode.workspace.getConfiguration('editor', {
    languageId: 'baml',
  })
  if (!config.get('defaultFormatter')) {
    // TODO: once the BAML formatter is stable, we should auto-prompt people to set it as the default formatter.
    // void vscode.commands.executeCommand('baml.setDefaultFormatter')
  }

  // Listen for messages from the webview

  plugins.map(async (plugin) => {
    try {
      const enabled = await plugin.enabled()
      if (enabled) {
        console.log(`Activating ${plugin.name}`)
        if (plugin.activate) {
          await plugin.activate(context, outputChannel)
        }
      } else {
        console.log(`${plugin.name} is Disabled`)
      }
    } catch (error) {
      console.error(`Error activating ${plugin.name}:`, error)
    }
  })

  if (process.env.VSCODE_DEBUG_MODE === 'true') {
    console.log(`vscode env: ${JSON.stringify(process.env, null, 2)}`)
    vscode.commands.executeCommand('baml.openBamlPanel')
  }

  setInterval(() => {
    console.log('requesting baml cli version')
    publishBamlVersionReport()
  }, 30_000)

  // TODO: Reactivate linter.
  // runDiagnostics()
}

export function deactivate(): void {
  console.log('BAML extension deactivating')
  diagnosticsCollection.clear()
  diagnosticsCollection.dispose()
  StatusBarPanel.instance.dispose()
  for (const plugin of plugins) {
    if (plugin.deactivate) {
      void plugin.deactivate()
    }
  }
  server?.close()
}

// Create our two decoration states
function createDecorations() {
  // Bright neon color for the glow effect (bright green)
  const glowColor = '#00FF00'
  const offColor = '#009900'

  // Glow ON - attempt to create text glow with textDecoration property
  glowOnDecoration = vscode.window.createTextEditorDecorationType({
    color: glowColor,
    fontWeight: 'bold',
    backgroundColor: 'transparent',
    textDecoration: `none; text-shadow: 0 0 4px ${glowColor}, 0 0 6px ${glowColor}`,
    // Try using before/after elements to reinforce the glow effect
    before: {
      contentText: '',
      textDecoration: `none; text-shadow: 0 0 4px ${glowColor}, 0 0 6px ${glowColor}`,
      color: glowColor,
    },
    after: {
      contentText: '',
      textDecoration: `none; text-shadow: 0 0 4px ${glowColor}, 0 0 6px ${glowColor}`,
      color: glowColor,
    },
  })

  // Glow OFF - text glow with textDecoration property.
  glowOffDecoration = vscode.window.createTextEditorDecorationType({
    color: offColor,
    fontWeight: 'bold',
    backgroundColor: 'transparent',
    textDecoration: `none; `,
    // Try using before/after elements to reinforce the glow effect
    before: {
      contentText: '',
      textDecoration: `none; `,
      color: offColor,
    },
    after: {
      contentText: '',
      textDecoration: `none; `,
      color: offColor,
    },
  })
}

// Update the highlight based on current state
function updateHighlight() {
  // vscode.window.showWarningMessage(`updateHighlight:` +  isGlowOn)
  const editor = vscode.window.activeTextEditor
  if (!editor) return

  // Clear both decorations
  // Apply appropriate decoration based on state
  if (glowOnDecoration && glowOffDecoration && isGlowOn) {
    editor.setDecorations(glowOffDecoration, [])
    editor.setDecorations(glowOnDecoration, highlightRanges)
  }
  if (glowOnDecoration && glowOffDecoration && !isGlowOn) {
    editor.setDecorations(glowOnDecoration, [])
    editor.setDecorations(glowOffDecoration, highlightRanges)
  }
}

// Start the simple toggling animation
function startAnimation() {
  if (animationTimer) return

  // Toggle every 500ms (2 times per second)
  animationTimer = setInterval(() => {
    // Toggle between on and off states
    isGlowOn = !isGlowOn

    // Update the highlight
    updateHighlight()
  }, 500); // 500ms = half a second
}

// Stop animation
function stopAnimation(): void {
  if (animationTimer) {
    clearInterval(animationTimer)
    animationTimer = null
  }
}
