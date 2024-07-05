import * as vscode from 'vscode'
import axios from 'axios'
import glooLens from './LanguageToBamlCodeLensProvider'
import { WebPanelView, openPlaygroundConfig } from './panels/WebPanelView'
import plugins from './plugins'
import { requestDiagnostics } from './plugins/language-server'
import { telemetry } from './plugins/language-server'
import cors from 'cors'
import { createProxyMiddleware } from 'http-proxy-middleware'
import { type LanguageClient, type ServerOptions, TransportKind } from 'vscode-languageclient/node'

let client: LanguageClient

const outputChannel = vscode.window.createOutputChannel('baml')
const diagnosticsCollection = vscode.languages.createDiagnosticCollection('baml-diagnostics')
const LANG_NAME = 'Baml'

let timeout: NodeJS.Timeout | undefined
let statusBarItem: vscode.StatusBarItem
let server: any

function scheduleDiagnostics(): void {
  if (timeout) {
    clearTimeout(timeout)
  }
  timeout = setTimeout(() => {
    statusBarItem.show()
    runDiagnostics()
  }, 1000) // 1 second after the last keystroke
}

interface LintRequest {
  lintingRules: string[]
  promptTemplate: string
  promptVariables: { [key: string]: string }
}

interface LinterOutput {
  exactPhrase: string
  reason: string
  severity: string
  recommendation?: string
  recommendation_reason?: string
  fix?: string
}

interface LinterRuleOutput {
  diagnostics: LinterOutput[]
  ruleName: string
}

async function runDiagnostics(): Promise<void> {
  const editor = vscode.window.activeTextEditor
  if (!editor) {
    statusBarItem.hide()
    return
  }

  console.log('Running diagnostics')

  statusBarItem.text = `$(sync~spin) Running AI Linter...`
  statusBarItem.backgroundColor = '##9333ea'
  statusBarItem.color = '#ffffff'
  const text = editor.document.getText()

  const lintRequest: LintRequest = {
    lintingRules: ['Rule1', 'Rule2'],
    promptTemplate: text,
    promptVariables: {},
  }
  const diagnostics: vscode.Diagnostic[] = []

  try {
    const response = await axios.post<LinterRuleOutput[]>('http://localhost:8000/lint', lintRequest)
    console.log('Got response:', response.data)
    const results = response.data

    results.forEach((rule) => {
      let found = false

      rule.diagnostics.forEach((output) => {
        const escapedPhrase = output.exactPhrase.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
        const phrase = output.exactPhrase
        let index = 0
        // Find all occurrences of the phrase
        while ((index = text.indexOf(phrase, index)) !== -1) {
          found = true
          const startPos = editor.document.positionAt(index)
          const endPos = editor.document.positionAt(index + phrase.length)
          const range = new vscode.Range(startPos, endPos)

          const diagnostic = new vscode.Diagnostic(
            range,
            `${output.reason}${output.recommendation ? ` - ${output.recommendation}` : ''}`,
            output.severity === 'error' ? vscode.DiagnosticSeverity.Error : vscode.DiagnosticSeverity.Warning,
          )

          if (output.fix) {
            diagnostic.code = '[linter]' + output.fix
          }
          diagnostic.source = rule.ruleName

          diagnostics.push(diagnostic)
          index += phrase.length // Move index to the end of the current found phrase to continue searching
        }

        if (!found && phrase.length > 100) {
          const subPhrase = phrase.substring(0, 100)
          index = 0 // Reset index for new search
          while ((index = text.indexOf(subPhrase, index)) !== -1) {
            const startPos = editor.document.positionAt(index)
            const endPos = editor.document.positionAt(index + subPhrase.length)
            const range = new vscode.Range(startPos, endPos)

            const diagnostic = new vscode.Diagnostic(
              range,
              `${output.reason}${output.recommendation ? ` - ${output.recommendation}` : ''}`,
              output.severity === 'error' ? vscode.DiagnosticSeverity.Error : vscode.DiagnosticSeverity.Warning,
            )

            if (output.fix) {
              diagnostic.code = '[linter]' + output.fix
            }
            diagnostic.source = rule.ruleName

            diagnostics.push(diagnostic)
            index += subPhrase.length // Move index to the end of the current found phrase to continue searching
          }
        }

        // const newRegex = new RegExp(`\\b${}\\b`, 'gi');
      })
    })
    console.log('Pushing test errorrrr')

    console.log('Diagnostics:', diagnostics)
    diagnosticsCollection.clear()
    diagnosticsCollection.set(editor.document.uri, diagnostics)
  } catch (error) {
    console.error('Failed to run diagnostics:', error)
    vscode.window.showErrorMessage('Failed to run diagnostics')
  }
  statusBarItem.text = 'AI Linter Ready'
  statusBarItem.hide()
}

import type { Express } from 'express'
import StatusBarPanel from './panels/StatusBarPanel'

export function activate(context: vscode.ExtensionContext) {
  console.log('BAML extension activating')

  vscode.workspace.getConfiguration('baml')
  // TODO: Reactivate linter.
  // statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100)
  // statusBarItem.text = `AI Linter Ready`
  // statusBarItem.show()
  context.subscriptions.push(StatusBarPanel.instance)

  const provider = new DiagnosticCodeActionProvider()
  const selector: vscode.DocumentSelector = { scheme: 'file', language: 'baml' } // Adjust language as necessary
  const codeActionProvider = vscode.languages.registerCodeActionsProvider(selector, provider, {
    providedCodeActionKinds: [vscode.CodeActionKind.QuickFix],
  })

  context.subscriptions.push(codeActionProvider)

  const app: Express = require('express')()
  app.use(cors())
  var port: number
  const server = app.listen(0, () => {
    console.log('Server started on port ' + getPort())
    WebPanelView.currentPanel?.postMessage('port_number', {
      port: port,
    })
  })

  const getPort = () => {
    let addr = server.address()
    if (addr === null) {
      vscode.window.showErrorMessage(
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
      changeOrigin: true,
      pathRewrite: (path, req) => {
        // Ensure the URL does not end with a slash
        if (path.endsWith('/')) {
          return path.slice(0, -1)
        }
        return path
      },
      router: (req) => {
        // Extract the original target URL from the custom header
        let originalUrl = req.headers['baml-original-url']
        if (typeof originalUrl === 'string') {
          delete req.headers['baml-original-url']
          delete req.headers['baml-render-url']
          req.headers['origin'] = `http://localhost:${port}`

          // Ensure the URL does not end with a slash
          if (originalUrl.endsWith('/')) {
            originalUrl = originalUrl.slice(0, -1)
          }
          return originalUrl
        } else {
          throw new Error('baml-original-url header is missing or invalid')
        }
      },
      logger: console,
      on: {
        proxyRes: (proxyRes, req, res) => {
          proxyRes.headers['Access-Control-Allow-Origin'] = '*'
        },
        error: (error) => {
          console.error('proxy error:', error)
        },
      },
    }),
  )

  const bamlPlaygroundCommand = vscode.commands.registerCommand(
    'baml.openBamlPanel',
    (args?: { projectId?: string; functionName?: string; implName?: string; showTests?: boolean }) => {
      const config = vscode.workspace.getConfiguration()
      config.update('baml.bamlPanelOpen', true, vscode.ConfigurationTarget.Global)

      WebPanelView.render(context.extensionUri, getPort)
      if (telemetry) {
        telemetry.sendTelemetryEvent({
          event: 'baml.openBamlPanel',
          properties: {},
        })
      }
      // sends project files as well to webview
      requestDiagnostics()
      openPlaygroundConfig.lastOpenedFunction = args?.functionName ?? 'default'
      WebPanelView.currentPanel?.postMessage('select_function', {
        root_path: 'default',
        function_name: args?.functionName ?? 'default',
      })

      console.info('Opening BAML panel')
    },
  )

  context.subscriptions.push(bamlPlaygroundCommand)
  console.log('pushing glooLens')

  const pythonSelector = { language: 'python', scheme: 'file' }
  const typescriptSelector = { language: 'typescript', scheme: 'file' }

  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider(pythonSelector, glooLens),
    vscode.languages.registerCodeLensProvider(typescriptSelector, glooLens),
  )

  context.subscriptions.push(diagnosticsCollection)

  // Add cursor movement listener
  vscode.window.onDidChangeTextEditorSelection((event) => {
    const position = event.selections[0].active

    const editor = vscode.window.activeTextEditor
    if (editor) {
      const name = editor.document.fileName
      const text = editor.document.getText()

      // TODO: buggy when used with multiple functions, needs a fix.
      WebPanelView.currentPanel?.postMessage('update_cursor', {
        cursor: {
          fileName: name,
          fileText: text,
          line: position.line + 1,
          column: position.character,
        },
      })
    }
  })

  // Listen for messages from the webview

  plugins.map(async (plugin) => {
    const enabled = await plugin.enabled()
    if (enabled) {
      console.log(`Activating ${plugin.name}`)
      if (plugin.activate) {
        await plugin.activate(context, outputChannel)
      }
    } else {
      console.log(`${plugin.name} is Disabled`)
    }
  })

  if (process.env.VSCODE_DEBUG_MODE === 'true') {
    console.log(`vscode env: ${JSON.stringify(process.env, null, 2)}`)
    vscode.commands.executeCommand('baml.openBamlPanel')
  }

  // TODO: Reactivate linter.
  // runDiagnostics();
}

export function deactivate(): void {
  console.log('BAML extension deactivating')
  diagnosticsCollection.clear()
  diagnosticsCollection.dispose()
  StatusBarPanel.instance.dispose()
  statusBarItem.dispose()
  for (const plugin of plugins) {
    if (plugin.deactivate) {
      void plugin.deactivate()
    }
  }
  server?.close()
}

class DiagnosticCodeActionProvider implements vscode.CodeActionProvider {
  public provideCodeActions(
    document: vscode.TextDocument,
    range: vscode.Range,
    context: vscode.CodeActionContext,
    token: vscode.CancellationToken,
  ): vscode.ProviderResult<vscode.CodeAction[]> {
    const codeActions: vscode.CodeAction[] = []

    for (const diagnostic of context.diagnostics) {
      if (diagnostic.code?.toString().startsWith('[linter]')) {
        const fixString = diagnostic.code.toString().replace('[linter]', '')
        const fixAction = new vscode.CodeAction(`Apply fix: ${fixString}`, vscode.CodeActionKind.QuickFix)
        fixAction.edit = new vscode.WorkspaceEdit()
        fixAction.diagnostics = [diagnostic]
        fixAction.isPreferred = true

        const edit = new vscode.TextEdit(diagnostic.range, fixString)
        fixAction.edit.set(document.uri, [edit])

        codeActions.push(fixAction)
      }
    }
    return codeActions
  }
}
