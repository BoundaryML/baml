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
import { URI } from 'vscode-uri'

import debounce from 'lodash/debounce'
import { createConnection, IPCMessageReader, IPCMessageWriter } from 'vscode-languageserver/node'
import { TextDocument } from 'vscode-languageserver-textdocument'

import * as MessageHandler from './lib/MessageHandler'
import type { LSOptions, LSSettings } from './lib/types'
import { getVersion, getEnginesVersion, getCliVersion } from './lib/wasm/internals'
import { BamlDirCache } from './file/fileCache'
import { LinterInput } from './lib/wasm/lint'
import { cliBuild, cliVersion } from './baml-cli'
import { TestRequest } from '@baml/common'
import generate_test_file from './lib/wasm/generate_test_file'

const packageJson = require('../../package.json') // eslint-disable-line
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
let hasConfigurationCapability = true

type BamlConfig = {
  path?: string
  trace: {
    server: string
  }
}
let config: BamlConfig | null = null

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
  const bamlCache = new BamlDirCache()

  connection.onInitialize((params: InitializeParams) => {
    // Logging first...

    connection.console.info(
      // eslint-disable-next-line
      `Extension '${packageJson?.name}': ${packageJson?.version}`,
    )
    connection.console.info(`Using 'baml-wasm': ${getVersion()}`)
    const prismaEnginesVersion = getEnginesVersion()

    // ... and then capabilities of the language server
    const capabilities = params.capabilities

    hasCodeActionLiteralsCapability = Boolean(capabilities?.textDocument?.codeAction?.codeActionLiteralSupport)
    hasConfigurationCapability = Boolean(capabilities?.workspace?.configuration)

    const result: InitializeResult = {
      capabilities: {
        definitionProvider: false,

        documentFormattingProvider: false,
        // completionProvider: {
        //   resolveProvider: false,
        //   triggerCharacters: ['@', '"', '.'],
        // },
        hoverProvider: false,
        renameProvider: false,
        documentSymbolProvider: false,
      },
    }

    // if (hasCodeActionLiteralsCapability) {
    //   result.capabilities.codeActionProvider = {
    //     codeActionKinds: [CodeActionKind.QuickFix],
    //   }
    // }

    return result
  })

  connection.onInitialized(() => {
    console.log('initialized')

    if (hasConfigurationCapability) {
      // Register for all configuration changes.
      // eslint-disable-next-line @typescript-eslint/no-floating-promises
      connection.client.register(DidChangeConfigurationNotification.type)
      getConfig()
    }
  })

  // The global settings, used when the `workspace/configuration` request is not supported by the client or is not set by the user.
  // This does not apply to VS Code, as this client supports this setting.
  // const defaultSettings: LSSettings = {}
  // let globalSettings: LSSettings = defaultSettings // eslint-disable-line

  // Cache the settings of all open documents
  const documentSettings: Map<string, Thenable<LSSettings>> = new Map<string, Thenable<LSSettings>>()

  const getConfig = async () => {
    try {
      const configResponse = await connection.workspace.getConfiguration('baml')
      console.log('configResponse ' + JSON.stringify(configResponse, null, 2))
      config = configResponse as BamlConfig
    } catch (e: any) {
      if (e instanceof Error) {
        console.log('Error getting config' + e.message + ' ' + e.stack)
      } else {
        console.log('Error getting config' + e)
      }
    }
  }

  connection.onDidChangeConfiguration((_change) => {
    connection.console.info('Configuration changed.' + JSON.stringify(_change, null, 2))
    getConfig()
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
    try {
      // TODO: revalidate if something changed
      bamlCache.refreshDirectory(e.document)
      bamlCache.addDocument(e.document)
      debouncedValidateTextDocument(e.document)
    } catch (e: any) {
      if (e instanceof Error) {
        console.log('Error opening doc' + e.message + ' ' + e.stack)
      } else {
        console.log('Error opening doc' + e)
      }
    }
  })

  // Only keep settings for open documents
  documents.onDidClose((e) => {
    try {
      bamlCache.refreshDirectory(e.document)
      // Revalidate all open files since this one may have been deleted.
      // we could be smarter and only do this if the doc was deleted, not just closed.
      documents.all().forEach(debouncedValidateTextDocument)
      documentSettings.delete(e.document.uri)
    } catch (e: any) {
      if (e instanceof Error) {
        console.log('Error closing doc' + e.message + ' ' + e.stack)
      } else {
        console.log('Error closing doc' + e)
      }
    }
  })

  // Note: VS Code strips newline characters from the message
  function showErrorToast(errorMessage: string): void {
    connection.window
      .showErrorMessage(errorMessage, {
        title: 'Show Details',
      })
      .then((item) => {
        if (item?.title === 'Show Details') {
          connection.sendNotification('baml/showLanguageServerOutput')
        }
      })
  }

  function generateTestFile(test_request: TestRequest) {
    try {
      const { cache, root_path: rootPath } = bamlCache.lastBamlDir
      if (!rootPath || !cache) {
        console.error('Could not find root path')
        connection.sendNotification('baml/message', {
          type: 'error',
          message: 'Could not find a baml_src directory for root path',
        })
        return
      }
      const srcDocs = cache.getDocuments()
      const linterInput: LinterInput = {
        root_path: rootPath.fsPath,
        files: srcDocs.map(({ path, doc }) => {
          return {
            path,
            content: doc.getText(),
          }
        }),
      }

      if (srcDocs.length === 0) {
        console.log('No BAML files found in the workspace.')
        connection.sendNotification('baml/message', {
          type: 'warn',
          message: 'Unable to find BAML files. See Output panel -> BAML Language Server for more details.',
        })
      }
      const response = MessageHandler.handleGenerateTestFile(srcDocs, linterInput, test_request, showErrorToast)
      if (response.status === 'ok') {
        return response.content
      } else {
        showErrorToast(response.message)
      }
    } catch (e: any) {
      if (e instanceof Error) {
        console.log('Error generating test file' + e.message + ' ' + e.stack)
      } else {
        console.log('Error generating test file' + e)
      }
    }
  }

  function validateTextDocument(textDocument: TextDocument) {
    try {
      const rootPath = bamlCache.getBamlDir(textDocument)
      if (!rootPath) {
        console.error('Could not find root path for ' + textDocument.uri)
        connection.sendNotification('baml/message', {
          type: 'error',
          message: 'Could not find a baml_src directory for ' + textDocument.uri.toString(),
        })
        return
      }
      const srcDocs = bamlCache.getDocuments(textDocument)
      if (srcDocs.length === 0) {
        console.log(`No BAML files found in the workspace. ${rootPath}`)
        connection.sendNotification('baml/message', {
          type: 'warn',
          message: `Empty baml_src directory found: ${rootPath.fsPath}. See Output panel -> BAML Language Server for more details.`,
        })
        return
      }

      const response = MessageHandler.handleDiagnosticsRequest(rootPath, srcDocs, showErrorToast)
      for (const [uri, diagnosticList] of response.diagnostics) {
        void connection.sendDiagnostics({ uri, diagnostics: diagnosticList })
      }

      bamlCache.addDatabase(rootPath, response.state)
      if (response.state) {
        void connection.sendRequest('set_database', { rootPath: rootPath.fsPath, db: response.state })
      } else {
        void connection.sendRequest('rm_database', rootPath)
      }
    } catch (e: any) {
      if (e instanceof Error) {
        console.log('Error validating doc' + e.message + ' ' + e.stack)
      } else {
        console.log('Error validating doc' + e)
      }
    }
  }

  const debouncedValidateTextDocument = debounce(validateTextDocument, 400, {
    maxWait: 4000,
    leading: true,
    trailing: true,
  })

  documents.onDidChangeContent((change: { document: TextDocument }) => {
    debouncedValidateTextDocument(change.document)
  })

  const debouncedCLIBuild = debounce(cliBuild, 1000, {
    leading: true,
    trailing: true,
  })

  documents.onDidSave((change: { document: TextDocument }) => {
    try {
      const cliPath = config?.path || 'baml'
      let bamlDir = bamlCache.getBamlDir(change.document)
      if (!bamlDir) {
        console.error(
          'Could not find baml_src dir for ' + change.document.uri + '. Make sure your baml files are in baml_src dir',
        )
        return
      }

      debouncedCLIBuild(cliPath, bamlDir, showErrorToast, () => {
        connection.sendNotification('baml/message', {
          type: 'info',
          message: 'Generated BAML client successfully!',
        })
      })
    } catch (e: any) {
      if (e instanceof Error) {
        console.log('Error saving doc' + e.message + ' ' + e.stack)
      } else {
        console.log('Error saving doc' + e)
      }
    }
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

  // connection.onCompletion((params: CompletionParams) => {
  //   const doc = getDocument(params.textDocument.uri)
  //   if (doc) {
  //     return MessageHandler.handleCompletionRequest(params, doc, showErrorToast)
  //   }
  // })

  // This handler resolves additional information for the item selected in the completion list.
  // connection.onCompletionResolve((completionItem: CompletionItem) => {
  //   return MessageHandler.handleCompletionResolveRequest(completionItem)
  // })

  // connection.onHover((params: HoverParams) => {
  //   const doc = getDocument(params.textDocument.uri)
  //   if (doc) {
  //     return MessageHandler.handleHoverRequest(doc, params)
  //   }
  // })

  // connection.onDocumentFormatting((params: DocumentFormattingParams) => {
  //   const doc = getDocument(params.textDocument.uri)
  //   if (doc) {
  //     return MessageHandler.handleDocumentFormatting(params, doc, showErrorToast)
  //   }
  // })

  // connection.onCodeAction((params: CodeActionParams) => {
  //   const doc = getDocument(params.textDocument.uri)
  //   if (doc) {
  //     return MessageHandler.handleCodeActions(params, doc, showErrorToast)
  //   }
  // })

  // connection.onRenameRequest((params: RenameParams) => {
  //   const doc = getDocument(params.textDocument.uri)
  //   if (doc) {
  //     return MessageHandler.handleRenameRequest(params, doc)
  //   }
  // })

  // connection.onDocumentSymbol((params: DocumentSymbolParams) => {
  //   return [];
  //   // const doc = getDocument(params.textDocument.uri)
  //   // if (doc) {
  //   //   return MessageHandler.handleDocumentSymbol(params, doc)
  //   // }
  // })
  connection.onRequest('cliVersion', async () => {
    try {
      const res = await new Promise<string>((resolve, reject) => {
        cliVersion(config?.path || 'baml', reject, (ver) => {
          resolve(ver)
        })
      })

      return res
    } catch (e: any) {
      return undefined
    }
  })

  connection.onRequest('generatePythonTests', (params: TestRequest) => {
    return generateTestFile(params)
  })
  connection.onRequest('registerFileChange', ({ fileUri, language }: { fileUri: string; language: string }) => {
    // TODO: revalidate if something changed
    // create textdocument from file:
    const textDocument = TextDocument.create(fileUri, language, 1, '')
    bamlCache.refreshDirectory(textDocument)
    bamlCache.getDocuments(textDocument).forEach(({ doc }) => {
      debouncedValidateTextDocument(doc)
    })
  })

  console.log('Server-side -- listening to connection')
  // Make the text document manager listen on the connection
  // for open, change and close text document events
  documents.listen(connection)

  connection.listen()
}
