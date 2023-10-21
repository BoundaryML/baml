import {
  TextDocuments,
  Diagnostic,
  InitializeParams,
  InitializeResult,
  CodeActionKind,
  CodeActionParams,
  HoverParams,
  CompletionItem,
  CompletionParams,
  DeclarationParams,
  RenameParams,
  DocumentFormattingParams,
  DidChangeConfigurationNotification,
  Connection,
  DocumentSymbolParams,
  TextDocumentSyncKind,
} from 'vscode-languageserver'
import { createConnection, IPCMessageReader, IPCMessageWriter } from 'vscode-languageserver/node'
import { TextDocument } from 'vscode-languageserver-textdocument'

import * as MessageHandler from './lib/MessageHandler'
import type { LSOptions, LSSettings } from './lib/types'
import { getVersion, getEnginesVersion, getCliVersion } from './lib/wasm/internals'
import { BamlDirCache } from './file/fileCache'
import { LinterInput } from './lib/wasm/lint'

const packageJson = require('../../package.json') // eslint-disable-line
console.log('Server-side -- packageJson', packageJson)
function getConnection(options?: LSOptions): Connection {
  let connection = options?.connection
  if (!connection) {
    connection = process.argv.includes('--stdio')
      ? createConnection(process.stdin, process.stdout)
      : createConnection(new IPCMessageReader(process), new IPCMessageWriter(process))
  }
  return connection
}

let hasCodeActionLiteralsCapability = false
let hasConfigurationCapability = false

/**
 * Starts the language server.
 *
 * @param options Options to customize behavior
 */
export function startServer(options?: LSOptions): void {
  console.log('Server-side -- startServer()')
  // Source code: https://github.com/microsoft/vscode-languageserver-node/blob/main/server/src/common/server.ts#L1044
  const connection: Connection = getConnection(options)

  console.log = connection.console.log.bind(connection.console)
  console.error = connection.console.error.bind(connection.console)

  console.log('Starting Baml Language Server...')

  const documents: TextDocuments<TextDocument> = new TextDocuments(TextDocument)
  const bamlCache = new BamlDirCache();

  connection.onInitialize((params: InitializeParams) => {
    // Logging first...
    connection.console.info(`Default version of Prisma 'prisma-schema-wasm': ${getVersion()}`)

    connection.console.info(
      // eslint-disable-next-line
      `Extension name ${packageJson?.name} with version ${packageJson?.version}`,
    )
    const prismaEnginesVersion = getEnginesVersion()
    connection.console.info(`Prisma Engines version: ${prismaEnginesVersion}`)
    // const prismaCliVersion = getCliVersion()
    // connection.console.info(`Prisma CLI version: ${prismaCliVersion}`)

    // ... and then capabilities of the language server
    const capabilities = params.capabilities

    hasCodeActionLiteralsCapability = Boolean(capabilities?.textDocument?.codeAction?.codeActionLiteralSupport)
    hasConfigurationCapability = Boolean(capabilities?.workspace?.configuration)

    const result: InitializeResult = {
      capabilities: {
        definitionProvider: true,
        documentFormattingProvider: true,
        completionProvider: {
          resolveProvider: true,
          triggerCharacters: ['@', '"', '.'],
        },
        hoverProvider: true,
        renameProvider: true,
        documentSymbolProvider: true,
      },
    }

    if (hasCodeActionLiteralsCapability) {
      result.capabilities.codeActionProvider = {
        codeActionKinds: [CodeActionKind.QuickFix],
      }
    }

    return result
  })

  connection.onInitialized(() => {
    console.log('initialized')

    if (hasConfigurationCapability) {
      // Register for all configuration changes.
      // eslint-disable-next-line @typescript-eslint/no-floating-promises
      connection.client.register(DidChangeConfigurationNotification.type, undefined)
    }
  })

  // The global settings, used when the `workspace/configuration` request is not supported by the client or is not set by the user.
  // This does not apply to VS Code, as this client supports this setting.
  // const defaultSettings: LSSettings = {}
  // let globalSettings: LSSettings = defaultSettings // eslint-disable-line

  // Cache the settings of all open documents
  const documentSettings: Map<string, Thenable<LSSettings>> = new Map<string, Thenable<LSSettings>>()

  connection.onDidChangeConfiguration((_change) => {
    connection.console.info('Configuration changed.')
    if (hasConfigurationCapability) {
      // Reset all cached document settings
      documentSettings.clear()
    } else {
      // globalSettings = <LSSettings>(change.settings.prisma || defaultSettings) // eslint-disable-line @typescript-eslint/no-unsafe-member-access
    }

    // Revalidate all open prisma schemas
    documents.all().forEach(validateTextDocument) // eslint-disable-line @typescript-eslint/no-misused-promises
  })

  documents.onDidOpen((e) => {
    // TODO: revalidate if something changed
    bamlCache.refreshDirectory(e.document);
    bamlCache.addDocument(e.document);
  });

  // Only keep settings for open documents
  documents.onDidClose((e) => {
    bamlCache.refreshDirectory(e.document);
    // Revalidate all open files since this one may have been deleted.
    // we could be smarter and only do this if the doc was deleted, not just closed.
    documents.all().forEach(validateTextDocument)
    documentSettings.delete(e.document.uri)
  })


  // function getDocumentSettings(resource: string): Thenable<LSSettings> {
  //   if (!hasConfigurationCapability) {
  //     connection.console.info(
  //       `hasConfigurationCapability === false. Defaults will be used.`,
  //     )
  //     return Promise.resolve(globalSettings)
  //   }

  //   let result = documentSettings.get(resource)
  //   if (!result) {
  //     result = connection.workspace.getConfiguration({
  //       scopeUri: resource,
  //       section: 'prisma',
  //     })
  //     documentSettings.set(resource, result)
  //   }
  //   return result
  // }

  // Note: VS Code strips newline characters from the message
  function showErrorToast(errorMessage: string): void {
    connection.window.showErrorMessage(errorMessage)
  }

  function throwError(errorMessage: string): void {
    throw new Error(errorMessage)
  }

  function validateTextDocument(textDocument: TextDocument) {
    try {
      const srcDocs = bamlCache.getDocuments(textDocument);
      const rootPath = bamlCache.getBamlDir(textDocument);
      if (!rootPath) {
        console.error("Could not find root path for " + textDocument.uri);
        return;
      }
      const linterInput: LinterInput = {
        root_path: rootPath,
        files: srcDocs.map((doc) => {
          return {
            path: doc.uri,
            content: doc.getText(),
          }
        }),
      }
      const diagnostics: Diagnostic[] = MessageHandler.handleDiagnosticsRequest(documents, linterInput, showErrorToast)
      void connection.sendDiagnostics({ uri: textDocument.uri, diagnostics })
    } catch (e: any) {
      if (e instanceof Error) {
        console.log("Error validating doc" + e.message + " " + e.stack);
      }
    }
  }

  documents.onDidChangeContent((change: { document: TextDocument }) => {
    validateTextDocument(change.document)
  })



  function getDocument(uri: string): TextDocument | undefined {
    return documents.get(uri)
  }

  // connection.onDefinition((params: DeclarationParams) => {
  //   const doc = getDocument(params.textDocument.uri)
  //   if (doc) {
  //     return MessageHandler.handleDefinitionRequest(doc, params)
  //   }
  // })

  connection.onCompletion((params: CompletionParams) => {
    const doc = getDocument(params.textDocument.uri)
    if (doc) {
      return MessageHandler.handleCompletionRequest(params, doc, showErrorToast)
    }
  })

  // This handler resolves additional information for the item selected in the completion list.
  connection.onCompletionResolve((completionItem: CompletionItem) => {
    return MessageHandler.handleCompletionResolveRequest(completionItem)
  })



  connection.onHover((params: HoverParams) => {
    const doc = getDocument(params.textDocument.uri)
    if (doc) {
      return MessageHandler.handleHoverRequest(doc, params)
    }
  })

  connection.onDocumentFormatting((params: DocumentFormattingParams) => {
    const doc = getDocument(params.textDocument.uri)
    if (doc) {
      return MessageHandler.handleDocumentFormatting(params, doc, showErrorToast)
    }
  })

  connection.onCodeAction((params: CodeActionParams) => {
    const doc = getDocument(params.textDocument.uri)
    if (doc) {
      return MessageHandler.handleCodeActions(params, doc, showErrorToast)
    }
  })

  connection.onRenameRequest((params: RenameParams) => {
    const doc = getDocument(params.textDocument.uri)
    if (doc) {
      return MessageHandler.handleRenameRequest(params, doc)
    }
  })

  connection.onDocumentSymbol((params: DocumentSymbolParams) => {
    const doc = getDocument(params.textDocument.uri)
    if (doc) {
      return MessageHandler.handleDocumentSymbol(params, doc)
    }
  })

  console.log('Server-side -- listening to connection')
  // Make the text document manager listen on the connection
  // for open, change and close text document events
  documents.listen(connection)

  connection.listen()
}
