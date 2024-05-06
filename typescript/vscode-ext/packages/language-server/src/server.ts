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
  CodeLensParams,
  Command,
  Position,
  Range,
  CodeLens,
  DidChangeWatchedFilesNotification,
  FileSystemWatcher,
} from 'vscode-languageserver'
import { URI } from 'vscode-uri'

import debounce from 'lodash/debounce'
import { createConnection, IPCMessageReader, IPCMessageWriter } from 'vscode-languageserver/node'
import { TextDocument } from 'vscode-languageserver-textdocument'

import * as MessageHandler from './lib/MessageHandler'
import type { LSOptions, LSSettings } from './lib/types'
// import { getVersion, getEnginesVersion } from './lib/wasm/internals'
import { BamlDirCache } from './file/fileCache'
// import { cliBuild, cliCheckForUpdates, cliVersion } from './baml-cli'
import { ParserDatabase, TestRequest } from '@baml/common'
// import { FileChangeType } from 'vscode'
import fs from 'fs'
import { z } from 'zod'
import BamlProjectManager from './lib/baml_project_manager'

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

const BamlConfig = z.optional(
  z.object({
    path: z.string().optional(),
    trace: z.optional(
      z.object({
        server: z.string(),
      }),
    ),
  }),
)
type BamlConfig = z.infer<typeof BamlConfig>
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
  const bamlProjectManager = new BamlProjectManager((params) => {
    switch (params.type) {
      case 'runtime_updated':
        // console.log(`runtime_updated today! ${Object.keys(params.files).length}: ${Object.entries(params.files).length}`)
        connection.sendRequest('runtime_updated', params)
        break
      case 'diagnostic':
        params.errors.forEach(([uri, diagnostics]) => {
          connection.sendDiagnostics({ uri, diagnostics })
        })
        break
      case 'error':
      case 'warn':
      case 'info':
        connection.sendNotification('baml/message', { message: params.message, type: params.type ?? 'warn' })
        break
      default:
        console.error(`Unknown notification type ${params}`)
    }
  })

  console.log = connection.console.log.bind(connection.console)
  console.error = connection.console.error.bind(connection.console)

  console.log('Starting Baml Language Server...')

  const documents: TextDocuments<TextDocument> = new TextDocuments(TextDocument)
  const bamlCache = new BamlDirCache()

  connection.onInitialize((params: InitializeParams) => {
    connection.console.info(
      // eslint-disable-next-line
      `Extension '${packageJson?.name}': ${packageJson?.version}`,
    )
    // connection.console.info(`Using 'baml-wasm': ${getVersion()}`)
    // const prismaEnginesVersion = getEnginesVersion()

    // ... and then capabilities of the language server
    const capabilities = params.capabilities

    hasCodeActionLiteralsCapability = Boolean(capabilities?.textDocument?.codeAction?.codeActionLiteralSupport)
    hasConfigurationCapability = Boolean(capabilities?.workspace?.configuration)

    const result: InitializeResult = {
      capabilities: {
        textDocumentSync: TextDocumentSyncKind.Full,
        definitionProvider: true,
        documentFormattingProvider: false,
        // completionProvider: {
        //   resolveProvider: false,
        //   triggerCharacters: ['@', '"', '.'],
        // },
        hoverProvider: true,
        renameProvider: false,
        documentSymbolProvider: true,
        codeLensProvider: {
          resolveProvider: true,
        },
        workspace: {
          fileOperations: {
            didCreate: {
              filters: [
                {
                  scheme: 'file',
                  pattern: {
                    glob: '**/*.{baml, json}',
                  },
                },
              ],
            },
            didDelete: {
              filters: [
                {
                  scheme: 'file',
                  pattern: {
                    glob: '**/*.{baml, json}',
                  },
                },
              ],
            },
            didRename: {
              filters: [
                {
                  scheme: 'file',
                  pattern: {
                    glob: '**/*.{baml, json}',
                  },
                },
              ],
            },
          },
        },
      },
    }

    const hasWorkspaceFolderCapability = !!(capabilities.workspace && !!capabilities.workspace.workspaceFolders)
    if (hasWorkspaceFolderCapability) {
      result.capabilities.workspace = {
        workspaceFolders: {
          supported: true,
        },
      }
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
    getConfig()

    if (hasConfigurationCapability) {
      // Register for all configuration changes.
      // eslint-disable-next-line @typescript-eslint/no-floating-promises
      connection.client.register(DidChangeConfigurationNotification.type)
      connection.client.register(DidChangeWatchedFilesNotification.type)
    }
  })

  // Cache the settings of all open documents
  const documentSettings: Map<string, Thenable<LSSettings>> = new Map<string, Thenable<LSSettings>>()

  const getConfig = async () => {
    try {
      console.log('getting config')
      const configResponse = await connection.workspace.getConfiguration('baml')
      console.log('configResponse ' + JSON.stringify(configResponse, null, 2))
      config = BamlConfig.parse(configResponse)
    } catch (e: any) {
      if (e instanceof Error) {
        console.log('Error getting config' + e.message + ' ' + e.stack)
      } else {
        console.log('Error getting config' + e)
      }
    }
  }

  function getLanguageExtension(uri: string): string | undefined {
    const languageExtension = uri.split('.').pop()
    if (!languageExtension) {
      console.log('Could not find language extension for ' + uri)
      return
    }
    return languageExtension
  }

  connection.onDidChangeWatchedFiles(async (params) => {
    // let deleted_files = params.changes.filter((change) =>
    //   change.type == FileChangeType.Deleted
    // ).map((change) => change.uri);
    // let created_files = params.changes.filter((change) =>
    //   change.type == FileChangeType.Created
    // ).map((change) => change.uri);
    // let changed_files = params.changes.filter((change) =>
    //   change.type == FileChangeType.Changed
    // ).map((change) => change.uri);

    // TODO: @hellovai be more efficient about this (only revalidate the files that changed)
    // If anything changes, then we need to revalidate all documents
    // let hasChanges = deleted_files.length > 0 || created_files.length > 0 || changed_files.length > 0;

    let hasChanges = params.changes.length > 0
    if (hasChanges) {
      // TODO: @hellovai we should technically get all possible root paths
      // (someone could delete mutliple baml_src dirs at once)
      await bamlProjectManager.reload_project_files(URI.parse(params.changes[0].uri))
    }
  })

  connection.onDidChangeConfiguration((_change) => {
    getConfig()
    if (hasConfigurationCapability) {
      // Reset all cached document settings
      documentSettings.clear()
    } else {
      // globalSettings = <LSSettings>(change.settings.prisma || defaultSettings) // eslint-disable-line @typescript-eslint/no-unsafe-member-access
    }

    // documents.all().forEach(debouncedValidateTextDocument) // eslint-disable-line @typescript-eslint/no-misused-promises
  })

  documents.onDidOpen(async (e) => {
    await bamlProjectManager.touch_project(URI.parse(e.document.uri))

    // e.document.uri
    // try {
    //   // TODO: revalidate if something changed
    //   bamlCache.addDocument(e.document)
    //   debouncedValidateTextDocument(e.document)
    // } catch (e: any) {
    //   if (e instanceof Error) {
    //     console.log('Error opening doc' + e.message + ' ' + e.stack)
    //   } else {
    //     console.log('Error opening doc' + e)
    //   }
    // }
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

  // TODO: dont actually debounce for now or strange out of sync things happen..
  // so we currently set to 0
  const updateClientDB = (rootPath: URI, db: ParserDatabase) => {
    void connection.sendRequest('set_database', { rootPath: rootPath.fsPath, db })
  }

  documents.onDidChangeContent(async (change: { document: TextDocument }) => {
    const textDocument = change.document
    await bamlProjectManager.upsert_file(URI.parse(textDocument.uri), textDocument.getText())
    // TODO: @hellovai Consider debouncing this
    // debounce(validateTextDocument, 800, {
    //   maxWait: 4000,
    //   leading: true,
    //   trailing: true,
    // })
  })

  documents.onDidSave(async (change: { document: TextDocument }) => {
    await bamlProjectManager.save_file(URI.parse(change.document.uri), change.document.getText())
    // connection.sendNotification('baml/message', {
    //   type: 'info',
    //   message: 'Saved BAML client!',
    // });

    // try {
    //   const cliPath = config?.path || 'baml'
    //   let bamlDir = bamlCache.getBamlDir(change.document)
    //   if (!bamlDir) {
    //     console.error(
    //       'Could not find baml_src dir for ' + change.document.uri + '. Make sure your baml files are in baml_src dir',
    //     )
    //     return
    //   }

    //   debouncedCLIBuild(cliPath, bamlDir, showErrorToast, () => {
    //     connection.sendNotification('baml/message', {
    //       type: 'info',
    //       message: 'Generated BAML client successfully!',
    //     })
    //   })
    // } catch (e: any) {
    //   if (e instanceof Error) {
    //     console.log('Error saving doc' + e.message + ' ' + e.stack)
    //   } else {
    //     console.log('Error saving doc' + e)
    //   }
    // }
  })

  function getDocument(uri: string): TextDocument | undefined {
    return documents.get(uri)
  }

  connection.onDefinition((params: DeclarationParams) => {
    return undefined
    // const doc = getDocument(params.textDocument.uri)
    // if (doc) {
    //   const db = bamlCache.getFileCache(doc)
    //   if (db) {
    //     return MessageHandler.handleDefinitionRequest(db, doc, params)
    //   } else if (doc.languageId === 'python') {
    //     const db = bamlCache.lastBamlDir?.cache
    //     console.log(` python: ${doc.uri} files: ${db?.getDocuments().length}`)
    //     if (db) {
    //       return MessageHandler.handleDefinitionRequest(db, doc, params)
    //     }
    //   }
    // }
  })

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

  connection.onHover((params: HoverParams) => {
    return undefined
    // const doc = getDocument(params.textDocument.uri)
    // if (doc) {
    //   const db = bamlCache.getFileCache(doc)
    //   if (db) {
    //     return MessageHandler.handleHoverRequest(db, doc, params)
    //   }
    // }
  })

  connection.onCodeLens((params: CodeLensParams) => {
    return undefined

    // const document = getDocument(params.textDocument.uri)
    // const codeLenses: CodeLens[] = []
    // if (!document) {
    //   console.log('No text document available to compute codelens ' + params.textDocument.uri.toString())
    //   return codeLenses
    // }
    // bamlCache.addDocument(document)
    // // Dont debounce this! We need to give VSCode the most up to date info.
    // // VSCode will actually do adaptive debouncing for us https://github.com/microsoft/vscode/issues/106267
    // validateTextDocument(document)

    // const db = bamlCache.getParserDatabase(document)
    // const docFsPath = URI.parse(document.uri).fsPath
    // const baml_dir = bamlCache.getBamlDir(document)
    // if (!db) {
    //   console.log('No db for ' + document.uri + '. There may be a linter error or out of sync file')
    //   return codeLenses
    // }

    // for (const fn of db.functions) {
    //   if (fn.name.source_file !== docFsPath) {
    //     continue;
    //   }

    //   const range = Range.create(document.positionAt(fn.name.start), document.positionAt(fn.name.end))
    //   const command: Command = {
    //     title: '▶️ Open Playground',
    //     command: 'baml.openBamlPanel',
    //     arguments: [
    //       {
    //         projectId: baml_dir?.fsPath || '',
    //         functionName: fn.name.value,
    //         showTests: true,
    //       },
    //     ],
    //   }
    //   codeLenses.push({
    //     range,
    //     command,
    //   })

    //   switch (fn.syntax) {
    //     case "Version2":
    //       continue;

    //     case "Version1":
    //       for (const impl of fn.impls) {
    //         codeLenses.push({
    //           range: Range.create(document.positionAt(impl.name.start), document.positionAt(impl.name.end)),
    //           command: {
    //             title: '▶️ Open Playground',
    //             command: 'baml.openBamlPanel',
    //             arguments: [
    //               {
    //                 projectId: baml_dir?.fsPath || '',
    //                 functionName: fn.name.value,
    //                 implName: impl.name.value,
    //                 showTests: true,
    //               },
    //             ],
    //           },
    //         })
    //         codeLenses.push({
    //           range: Range.create(document.positionAt(impl.prompt_key.start), document.positionAt(impl.prompt_key.end)),
    //           command: {
    //             title: '▶️ Open Live Preview',
    //             command: 'baml.openBamlPanel',
    //             arguments: [
    //               {
    //                 projectId: baml_dir?.fsPath || '',
    //                 functionName: fn.name.value,
    //                 implName: impl.name.value,
    //                 showTests: false,
    //               },
    //             ],
    //           },
    //         })
    //       }
    //       break;

    //   }
    // }

    // const testCases = db.functions
    //   .flatMap((f) =>
    //     f.test_cases.map((t) => {
    //       return {
    //         value: t.name.value,
    //         start: t.name.start,
    //         end: t.name.end,
    //         source_file: t.name.source_file,
    //         function: f.name.value,
    //       }
    //     }),
    //   )
    //   .filter((x) => x.source_file === docFsPath)
    // testCases.forEach((name) => {
    //   const range = Range.create(document.positionAt(name.start), document.positionAt(name.end))
    //   const command: Command = {
    //     title: '▶️ Open Playground',
    //     command: 'baml.openBamlPanel',
    //     arguments: [
    //       {
    //         projectId: baml_dir?.fsPath || '',
    //         functionName: name.function,
    //         testCaseName: name.value,
    //         showTests: true,
    //       },
    //     ],
    //   }
    //   codeLenses.push({
    //     range,
    //     command,
    //   })
    // })
    // return codeLenses
  })

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

  connection.onDocumentSymbol((params: DocumentSymbolParams) => {
    return undefined
    // const doc = getDocument(params.textDocument.uri)
    // if (doc) {
    //   const db = bamlCache.getFileCache(doc)
    //   if (db) {
    //     let symbols = MessageHandler.handleDocumentSymbol(db, params, doc)
    //     return symbols
    //   }
    // }
  })

  connection.onRequest(
    'selectTestCase',
    ({
      functionName,
      testCaseName,
    }: {
      functionName: string
      testCaseName: string
    }) => {
      return
      // console.log('selectTestCase ' + functionName + ' ' + testCaseName)
      // let lastDb = bamlCache.lastPaserDatabase;

      // if (!lastDb) {
      //   console.log('No last db found');
      //   return
      // }

      // const selectedTests = Object.fromEntries(lastDb.db.functions.map((fn) => {
      //   let uniqueTestNames = new Set(fn.impls.flatMap((impl) => impl.prompt.test_case).filter((t): t is string => t !== undefined && t !== null));
      //   const testCases = new Array(...uniqueTestNames);
      //   let testCaseName = testCases.length > 0 ? testCases[0] : undefined;
      //   if (testCaseName === undefined) {
      //     return undefined;
      //   }
      //   return [fn.name.value, testCaseName]
      // }).filter((t): t is [string, string] => t !== undefined) ?? []);
      // selectedTests[functionName] = testCaseName;

      // const response = MessageHandler.handleDiagnosticsRequest(lastDb.root_path, lastDb.cache.getDocuments(), selectedTests, showErrorToast)
      // for (const [uri, diagnosticList] of response.diagnostics) {
      //   void connection.sendDiagnostics({ uri, diagnostics: diagnosticList })
      // }

      // bamlCache.addDatabase(lastDb.root_path, response.state)
      // if (response.state) {
      //   lastDb.cache.setDB(response.state)

      //   updateClientDB(lastDb.root_path, response.state)
      // } else {
      //   void connection.sendRequest('rm_database', lastDb.root_path)
      // }
    },
  )

  connection.onRequest('getDefinition', ({ sourceFile, name }: { sourceFile: string; name: string }) => {
    return
    // const fileCache = bamlCache.getCacheForUri(sourceFile)
    // if (fileCache) {
    //   let match = fileCache.define(name)
    //   if (match) {
    //     return {
    //       targetUri: match.uri.toString(),
    //       targetRange: match.range,
    //       targetSelectionRange: match.range,
    //     }
    //   }
    // }
  })

  connection.onRequest('cliVersion', async () => {
    console.log('Checking baml version at ' + config?.path)
    try {
      // const res = await new Promise<string>((resolve, reject) => {
      //   cliVersion(config?.path || 'baml', reject, (ver) => {
      //     resolve(ver)
      //   })
      // })

      // return res
      return undefined
    } catch (e: any) {
      if (e instanceof Error) {
        console.log('Error getting cli version' + e.message + ' ' + e.stack)
      } else {
        console.log('Error getting cli version' + e)
      }
      return undefined
    }
  })

  connection.onRequest('cliCheckForUpdates', async () => {
    console.log('Calling baml version --check using ' + config?.path)
    try {
      // const res = await new Promise<string>((resolve, reject) => {
      //   cliCheckForUpdates(config?.path || 'baml', reject, (ver) => {
      //     resolve(ver)
      //   })
      // })

      // return res
      return undefined
    } catch (e: any) {
      if (e instanceof Error) {
        console.log('Error getting cli version' + e.message + ' ' + e.stack)
      } else {
        console.log('Error getting cli version' + e)
      }
      return undefined
    }
  })

  console.log('Server-side -- listening to connection')
  // Make the text document manager listen on the connection
  // for open, change and close text document events
  documents.listen(connection)

  connection.listen()
}
