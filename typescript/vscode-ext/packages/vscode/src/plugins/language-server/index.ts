import * as path from 'path'

import { commands, ExtensionContext, window, workspace } from 'vscode'
import { LanguageClientOptions } from 'vscode-languageclient'
import { LanguageClient, ServerOptions, TransportKind } from 'vscode-languageclient/node'
import TelemetryReporter from '../../telemetryReporter'
import { checkForMinimalColorTheme, createLanguageServer, isDebugOrTestSession, restartClient } from '../../util'
import { BamlVSCodePlugin } from '../types'

const packageJson = require('../../../package.json') // eslint-disable-line

let client: LanguageClient
let serverModule: string
let telemetry: TelemetryReporter

const isDebugMode = () => process.env.VSCODE_DEBUG_MODE === 'true'
const isE2ETestOnPullRequest = () => process.env.PRISMA_USE_LOCAL_LS === 'true'

const activateClient = (
  context: ExtensionContext,
  serverOptions: ServerOptions,
  clientOptions: LanguageClientOptions,
) => {
  // Create the language client
  client = createLanguageServer(serverOptions, clientOptions)

  const disposable = client.start()


  // Start the client. This will also launch the server
  context.subscriptions.push(disposable)
}

const onFileChange = (filepath: string) => {
  console.debug(`File ${filepath} has changed, restarting TS Server.`)
  void commands.executeCommand('typescript.restartTsServer')
}

const plugin: BamlVSCodePlugin = {
  name: 'baml-language-server',
  enabled: () => true,
  activate: async (context) => {
    const isDebugOrTest = isDebugOrTestSession()

    // setGenerateWatcher(!!workspace.getConfiguration('baml').get('fileWatcher'))

    // eslint-disable-next-line @typescript-eslint/no-unsafe-member-access
    // if (packageJson.name === 'prisma-insider-pr-build') {
    //   console.log('Using local Language Server for prisma-insider-pr-build');
    //   serverModule = context.asAbsolutePath(path.join('./language-server/dist/src/bin'));
    // } else if (isDebugMode() || isE2ETestOnPullRequest()) {
    //   // use Language Server from folder for debugging
    //   console.log('Using local Language Server from filesystem');
    //   serverModule = context.asAbsolutePath(path.join('../../packages/language-server/dist/src/bin'));
    // } else {
    //   console.log('Using published Language Server (npm)');
    //   // use published npm package for production
    //   serverModule = require.resolve('@prisma/language-server/dist/src/bin');
    // }
    console.log('debugmode', isDebugMode())
    // serverModule = context.asAbsolutePath(path.join('../../packages/language-server/dist/src/bin'))

    serverModule = context.asAbsolutePath(
      path.join('language-server', 'out', 'bin')
    );


    console.log(`serverModules: ${serverModule}`)

    // The debug options for the server
    // --inspect=6009: runs the server in Node's Inspector mode so VS Code can attach to the server for debugging
    const debugOptions = {
      execArgv: ['--nolazy', '--inspect=6009'],
      env: { DEBUG: true },
    }

    // If the extension is launched in debug mode then the debug server options are used
    // Otherwise the run options are used
    const serverOptions: ServerOptions = {
      run: { module: serverModule, transport: TransportKind.ipc },
      debug: {
        module: serverModule,
        transport: TransportKind.ipc,
        options: debugOptions,
      },
    }

    // Options to control the language client
    const clientOptions: LanguageClientOptions = {
      // Register the server for prisma documents
      documentSelector: [{ scheme: 'file', language: 'baml' }],

      /* This middleware is part of the workaround for https://github.com/prisma/language-tools/issues/311 */
      // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
      // middleware: {
      //   async provideCodeActions(
      //     document: TextDocument,
      //     range: Range,
      //     context: CodeActionContext,
      //     token: CancellationToken,
      //     _: ProvideCodeActionsSignature,
      //   ) {
      //     const params: CodeActionParams = {
      //       textDocument: client.code2ProtocolConverter.asTextDocumentIdentifier(document),
      //       range: client.code2ProtocolConverter.asRange(range),
      //       context: client.code2ProtocolConverter.asCodeActionContext(context),
      //     }

      //     return client.sendRequest(CodeActionRequest.type, params, token).then(
      //       (values: any) => {
      //         if (values === null) {
      //           return undefined
      //         }
      //         const result: (CodeAction | Command)[] = []
      //         for (const item of values) {
      //           if (lsCodeAction.is(item)) {
      //             const action = client.protocol2CodeConverter.asCodeAction(item)
      //             if (
      //               isSnippetEdit(item, client.code2ProtocolConverter.asTextDocumentIdentifier(document)) &&
      //               item.edit !== undefined
      //             ) {
      //               action.command = {
      //                 command: 'baml.applySnippetWorkspaceEdit',
      //                 title: '',
      //                 arguments: [action.edit],
      //               }
      //               action.edit = undefined
      //             }
      //             result.push(action)
      //           } else {
      //             const command = client.protocol2CodeConverter.asCommand(item)
      //             result.push(command)
      //           }
      //         }
      //         return result
      //       },
      //       (_) => undefined,
      //     )
      //   },
      // } as any, // eslint-disable-line @typescript-eslint/no-explicit-any
    }
    console.log('clientOptions', clientOptions)

    context.subscriptions.push(
      // when the file watcher settings change, we need to ensure they are applied
      workspace.onDidChangeConfiguration((event) => {
        // if (event.affectsConfiguration('prisma.fileWatcher')) {
        //   setGenerateWatcher(!!workspace.getConfiguration('prisma').get('fileWatcher'));
        // }
      }),

      commands.registerCommand('baml.restartLanguageServer', async () => {
        client = await restartClient(context, client, serverOptions, clientOptions)
        window.showInformationMessage('Baml language server restarted.') // eslint-disable-line @typescript-eslint/no-floating-promises
      }),


    )

    activateClient(context, serverOptions, clientOptions)
    console.log('activated')

    if (!isDebugOrTest) {
      // eslint-disable-next-line
      const extensionId = 'Gloo.' + packageJson.name
      // eslint-disable-next-line
      const extensionVersion: string = packageJson.version

      telemetry = new TelemetryReporter(extensionId, extensionVersion)

      // context.subscriptions.push(telemetry)

      await telemetry.sendTelemetryEvent()

      if (extensionId === 'baml.baml-insider') {
        // checkForOtherPrismaExtension()
      }
    }

    checkForMinimalColorTheme()
  },
  deactivate: async () => {
    if (!client) {
      return undefined
    }

    if (!isDebugOrTestSession()) {
      telemetry.dispose() // eslint-disable-line @typescript-eslint/no-floating-promises
    }

    return client.stop()
  },
}
export default plugin
