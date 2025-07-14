import * as os from 'node:os';

import type {} from '@baml/common';
import semver from 'semver';
import {
  type ExtensionContext,
  type OutputChannel,
  commands,
  window,
  workspace,
} from 'vscode';
import * as vscode from 'vscode';
import type { LanguageClientOptions } from 'vscode-languageclient';
import {
  type LanguageClient,
  RevealOutputChannelOn,
  type ServerOptions,
} from 'vscode-languageclient/node';
import { URI } from 'vscode-uri';
import { z } from 'zod';
import packageJson from '../../../package.json';
import { getCurrentOpenedFile } from '../../helpers/get-open-file';
import StatusBarPanel from '../../panels/StatusBarPanel';
import { WebviewPanelHost } from '../../panels/WebviewPanelHost';
import TelemetryReporter from '../../telemetryReporter';
import {
  checkForMinimalColorTheme,
  createLanguageServer,
  isDebugOrTestSession,
} from '../../util';
import type { BamlVSCodePlugin } from '../types';
import {
  BAML_CONFIG_SINGLETON,
  refreshBamlConfigSingleton,
} from './bamlConfig';
import { resolveCliPath } from './cliDownloader';

export { BAML_CONFIG_SINGLETON as bamlConfig };
let clientReady = false;

let client: LanguageClient;
let telemetry: TelemetryReporter;
const intervalTimers: NodeJS.Timeout[] = [];
let bamlOutputChannel: OutputChannel;
// Variable to store the path of the currently executing CLI
let currentExecutingCliPath: string | null = null;

const isDebugMode = () => process.env.VSCODE_DEBUG_MODE === 'true';
const isE2ETestOnPullRequest = () => process.env.PRISMA_USE_LOCAL_LS === 'true';

const debugOptions = {
  execArgv: ['--nolazy', '--inspect=6009'],
  env: {
    DEBUG: true,
    RUST_BACKTRACE: 'full',
    ...process.env,
  },
};

const getClientOptions = (): LanguageClientOptions => ({
  documentSelector: [
    { scheme: 'file', language: 'baml' },
    { language: 'json', pattern: '**/baml_src/**' },
  ],
  outputChannel: vscode.window.createOutputChannel('Baml Language Server'),
  revealOutputChannelOn: RevealOutputChannelOn.Never,
  synchronize: {
    fileEvents: workspace.createFileSystemWatcher('**/baml_src/**/*.baml'),
  },
});

export const requestDiagnostics = async () => {
  const currentFile = getCurrentOpenedFile();
  if (!currentFile) {
    console.warn('no current baml file');
    return;
  }
  if (!currentFile.endsWith('.baml')) {
    return;
  }
  if (!clientReady) {
    console.warn('client not ready');
    return;
  }
  await client?.sendRequest('requestDiagnostics', { projectId: currentFile });
};

export const requestBamlCLIVersion = async (): Promise<string | undefined> => {
  if (!clientReady) {
    console.warn('Client not ready for CLI version request')
    return undefined
  }
  try {
    const response = await client.sendRequest('version')
    console.log('CLI version response:', response)
    return String(response)
  } catch (e) {
    console.error('Error getting CLI version:', e)
    return undefined
  }
}

export const getBAMLFunctions = async (): Promise<
  {
    name: string;
    span: { file_path: string; start: number; end: number };
  }[]
> => {
  if (!clientReady) {
    console.warn('Client not ready for getBAMLFunctions request');
    return [];
  }
  try {
    return await client.sendRequest('getBAMLFunctions');
  } catch (e) {
    console.error('Failed to get BAML functions:', e);
    return [];
  }
};

const LatestVersions = z.object({
  cli: z.object({
    current_version: z.string(),
    latest_version: z.string().nullable(),
    recommended_update: z.string().nullable(),
  }),
  generators: z.array(
    z.object({
      name: z.string(),
      current_version: z.string(),
      latest_version: z.string().nullable(),
      recommended_update: z.string().nullable(),
      language: z.string(),
    }),
  ),
  vscode: z.object({
    latest_version: z.string().nullable(),
  }),
});
type LatestVersions = z.infer<typeof LatestVersions>;

const checkForUpdates = ({ showIfNoUpdates }: { showIfNoUpdates: boolean }) => {
  console.log(
    'checkForUpdates called (currently stubbed). showIfNoUpdates:',
    showIfNoUpdates,
  );
  try {
    if (telemetry) {
      telemetry.sendTelemetryEvent({
        event: 'baml.checkForUpdates',
        properties: { stub: 'true' },
      });
    }
    bamlOutputChannel?.appendLine('Checked for updates (stubbed).');
  } catch (e) {
    console.error('Failed to check for updates', e);
  }
};

interface BAMLMessage {
  type: 'warn' | 'info' | 'error';
  message: string;
  durationMs?: number;
}

const sleep = (time: number) => {
  return new Promise<void>((resolve) => {
    setTimeout(() => resolve(), time)
  })
}

export const registerClientEventHandlers = (client: LanguageClient, context: ExtensionContext) => {
  client.onNotification('baml/showLanguageServerOutput', () => {
    // need to append line for the show to work for some reason.
    // dont delete this.
    client.outputChannel.appendLine('\n');
    client.outputChannel.show(true);
  });

  client.onNotification('baml/message', (message: BAMLMessage) => {
    console.log('baml/message', message);
    client.outputChannel.appendLine(
      `baml/message ${JSON.stringify(message, null, 2)}`,
    );
    let msg: Thenable<any>;
    switch (message.type) {
      case 'warn': {
        msg = window.showWarningMessage(message.message);
        break;
      }
      case 'info': {
        window.withProgress(
          {
            location: vscode.ProgressLocation.Notification,
            cancellable: false,
          },
          async (progress, token) => {
            let customCancellationToken: vscode.CancellationTokenSource | null =
              null;
            const rest = new Promise<null>((resolve) => {
              customCancellationToken = new vscode.CancellationTokenSource();

              customCancellationToken.token.onCancellationRequested(() => {
                customCancellationToken?.dispose();
                customCancellationToken = null;

                // vscode.window.showInformationMessage('Cancelled the progress')
                resolve(null);
                return;
              });

              const totalMs = message.durationMs || 1500; // Total duration in milliseconds (2 seconds)
              const updateCount = 50; // Number of updates
              const intervalMs = totalMs / updateCount; // Interval between updates
              (async () => {
                for (let i = 0; i < updateCount; i++) {
                  const prog = ((i + 1) / updateCount) * 100;
                  progress.report({
                    increment: prog,
                    message: message.message,
                  });
                  await sleep(intervalMs);
                }
                resolve(null);
              })();
            });

            return rest;
          },
        );
        break;
      }
      case 'error': {
        window
          .showErrorMessage(message.message, { modal: false }, 'Show Output')
          .then((selection) => {
            if (selection === 'Show Output') {
              client.outputChannel.show(true);
            }
          });
        break;
      }
      default: {
        throw new Error('Invalid message type');
      }
    }
  });

  client.onNotification(
    'runtime_diagnostics',
    (params: { errors: number; warnings: number }) => {
      console.log('runtime_diagnostics', params);
      try {
        if (params.errors > 0) {
          StatusBarPanel.instance.setStatus({
            status: 'fail',
            count: params.errors,
          });
        } else if (params.warnings > 0) {
          StatusBarPanel.instance.setStatus({
            status: 'warn',
            count: params.warnings,
          });
        } else {
          StatusBarPanel.instance.setStatus('pass');
        }
      } catch (e) {
        console.error('Error updating status bar', e);
      }
    },
  );

  client.onRequest('executeCommand', async (command: string) => {
    try {
      console.log('Executing command requested by LSP:', command);
      await vscode.commands.executeCommand(command);
    } catch (e) {
      console.error(
        `Error executing command '${command}' requested by LSP:`,
        e,
      );
    }
  });

  client.onRequest('baml_settings_updated', (config: typeof BAML_CONFIG_SINGLETON) => {
    console.log('Received baml_settings_updated from LSP:', config)
    BAML_CONFIG_SINGLETON.config = config.config
    BAML_CONFIG_SINGLETON.cliVersion = config.cliVersion
    WebviewPanelHost.currentPanel?.postMessage('baml_settings_updated', BAML_CONFIG_SINGLETON)
  })

  const handleRuntimeUpdated = (params: { root_path: string; files: Record<string, string> }) => {
    const activeEditor =
      window.activeTextEditor || (window.visibleTextEditors.length > 0 ? window.visibleTextEditors[0] : null)
    if (activeEditor) {
      try {
        const currentFilePath = URI.parse(activeEditor.document.uri.toString()).fsPath
        const rootPathUri = URI.file(params.root_path).fsPath
        if (currentFilePath.startsWith(rootPathUri)) {
          console.log('Forwarding runtime_updated to WebviewPanelHost')
          WebviewPanelHost.currentPanel?.postMessage('add_project', {
            ...params,
            root_path: URI.file(params.root_path).toString(),
          })
        } else {
          console.log('runtime_updated ignored: root path does not match active editor', currentFilePath, rootPathUri)
        }
      } catch (e) {
        console.error('Error processing runtime_updated:', e)
      }
    } else {
      console.log('runtime_updated ignored: no active editor')
    }
  }

  client.onRequest('runtime_updated', handleRuntimeUpdated);
  client.onNotification('runtime_updated', handleRuntimeUpdated);

  // eslint-disable-next-line @typescript-eslint/no-misused-promises
  client.onNotification(
    'baml_src_generator_version',
    async (payload: { version: string; root_path: string }) => {
      try {
        bamlOutputChannel.appendLine(
          `============ baml_src_generator_version notification: ${payload.version} ${payload.root_path}`,
        );

        const syncExtensionToGeneratorVersion =
          BAML_CONFIG_SINGLETON.config?.syncExtensionToGeneratorVersion;
        if (syncExtensionToGeneratorVersion === 'never') {
          bamlOutputChannel.appendLine(
            `Skipping version update as syncExtensionToGeneratorVersion is set to 'never'`,
          );
          return;
        }

        // Skip updating on Windows when setting is 'auto'
        if (
          syncExtensionToGeneratorVersion === 'auto' &&
          os.platform() === 'win32'
        ) {
          bamlOutputChannel.appendLine(
            `Skipping version update on Windows with 'auto' setting. Current platform: ${os.platform()}`,
          );
          return;
        }

        const version = payload.version;

        if (!semver.valid(version)) {
          console.error(`Received invalid version string from LSP: ${version}`);
          window.showErrorMessage(
            `BAML Language Server requested an invalid version: ${version}. Cannot update.`,
          );
          return;
        }

        // Check if the requested version is less than 0.86.0
        // Only LSPs 0.86.0 and above send out the baml_src_generator_version notification
        // so older LSPs will not be able to request an update
        if (semver.lt(version, '0.86.0')) {
          bamlOutputChannel.appendLine(
            `Ignoring version update request for ${version} as it's less than minimum required version 0.86.0`,
          );
          return;
        }

        console.log(
          `============ Attempting to resolve CLI path for requested version ${version}...`,
        );
        bamlOutputChannel.appendLine(
          `============ Attempting to resolve CLI path for requested version ${version}...`,
        );
        const targetCliPath = await resolveCliPath(
          context,
          version,
          bamlOutputChannel,
        );

        if (!targetCliPath) {
          console.error(
            `Failed to resolve CLI path for version ${version}. LSP restart aborted.`,
          );
          bamlOutputChannel?.appendLine(
            `ERROR: Failed to resolve CLI path for version ${version}. LSP restart aborted.`,
          );
          return;
        }

        console.log(`Resolved target CLI path: ${targetCliPath}`);
        // Use the module-level variable to check the current path
        console.log(
          `Currently executing CLI path: ${currentExecutingCliPath ?? 'Unknown'}`,
        );

        // Compare target path with the stored current path
        if (targetCliPath !== currentExecutingCliPath) {
          bamlOutputChannel.appendLine(
            `Target path (${targetCliPath}) differs from current (${currentExecutingCliPath}). Restarting LSP...`,
          );

          const serverOptionsForRestart: ServerOptions = {
            run: {
              command: targetCliPath,
              args: ['lsp'],
              options: { env: process.env },
            },
            debug: {
              command: targetCliPath,
              args: ['lsp'],
              options: debugOptions,
            },
          };

          const clientOptionsForRestart = getClientOptions();

          window.withProgress(
            {
              location: vscode.ProgressLocation.Notification,
              cancellable: false,
              title: `Restarting BAML Language Server (v${version})...`,
            },
            // eslint-disable-next-line @typescript-eslint/require-await
            async () => {
              try {
                console.log('Calling restartClient utility...');
                // clientReady will be managed by activateClient's onReady handlers.
                // activateClient also handles stopping the previous client.
                activateClient(
                  context,
                  serverOptionsForRestart,
                  clientOptionsForRestart,
                );
                console.log(`activateClient called for version ${version}.`);
                currentExecutingCliPath = targetCliPath;
                BAML_CONFIG_SINGLETON.cliVersion = version; // This might be better set after onReady, or via a message from the client

                bamlOutputChannel?.appendLine(
                  `BAML Language Server reload initiated for version ${version}.`,
                );
              } catch (e) {
                clientReady = false; // Ensure clientReady is false if restart fails
                console.error('Error restarting client:', e);
                // Ensure error message is a string
                const errorMessage = e instanceof Error ? e.message : String(e);
                bamlOutputChannel?.appendLine(
                  `ERROR: Error restarting client for version ${version}: ${errorMessage}`,
                );
                window.showErrorMessage(
                  `Failed to restart BAML Language Server to version ${version}.`,
                );
              }
            },
          );
        } else {
          // bamlOutputChannel?.appendLine(
          //   `Resolved path is the same as current. No LSP restart needed for version ${version}.`,
          // )
          if (BAML_CONFIG_SINGLETON.cliVersion !== version) {
            bamlOutputChannel?.appendLine(
              `Updating BAML config singleton version to ${version} (no restart needed).`,
            );
            BAML_CONFIG_SINGLETON.cliVersion = version;
          }
        }
      } catch (e: any) {
        console.error('Error processing baml_src_generator_version:', e);
        bamlOutputChannel?.appendLine(
          `ERROR: Error processing baml_src_generator_version: ${e}`,
        );
      }
    },
  );
};

const activateClient = (
  context: ExtensionContext,
  serverOptions: ServerOptions,
  clientOptions: LanguageClientOptions,
) => {
  refreshBamlConfigSingleton();
  console.log('Activating BAML Language Client...');
  console.log('Server Options:', JSON.stringify(serverOptions, null, 2));

  if (client?.needsStop()) {
    console.log('Stopping existing client before activating new one...');
    client
      .stop()
      .catch((e) => console.error('Error stopping existing client:', e));
  }

  client = createLanguageServer(serverOptions, clientOptions);
  console.log('Language client instance created.');

  client
    .onReady()
    .then(() => {
      console.log('Language client is ready.');
      clientReady = true;

      registerClientEventHandlers(client, context);
      console.log('Client event handlers registered.');

      requestDiagnostics().catch((e) =>
        console.error('Error requesting initial diagnostics:', e),
      );

      if (isDebugMode()) {
        client.outputChannel.show(true);
      }

      if (intervalTimers.length === 0) {
        console.log('Setting up periodic update checks.');
        checkForUpdates({ showIfNoUpdates: false });
        intervalTimers.push(
          setInterval(
            () => {
              console.log(
                `Periodic check for updates triggered: ${new Date().toString()}`,
              );
              checkForUpdates({ showIfNoUpdates: false });
            },
            6 * 60 * 60 * 1000 /* 6h */,
          ),
        );
      }
    })
    .catch((error) => {
      console.error('Language client failed to become ready:', error);
      clientReady = false;
      window.showErrorMessage('BAML Language Server failed to initialize.');
    });

  console.log('Starting language client...');
  const disposable = client.start();
  console.log('Client start initiated.');

  context.subscriptions.push(disposable);
};

const onFileChange = (filepath: string) => {
  console.debug(`File ${filepath} has changed, restarting TS Server.`);
  void commands.executeCommand('typescript.restartTsServer');
};

const plugin: BamlVSCodePlugin = {
  name: 'baml-language-server',
  enabled: () => true,
  activate: async (context, _outputChannel) => {
    const isDebugOrTest = isDebugOrTestSession();
    bamlOutputChannel = _outputChannel;
    context.subscriptions.push(bamlOutputChannel);
    bamlOutputChannel.appendLine('Activating BAML Language Server plugin...');

    console.log('Activating BAML Language Server plugin...');
    bamlOutputChannel.appendLine(`Debug/Test Session: ${isDebugOrTest}`);
    bamlOutputChannel.appendLine(
      `Bundled Extension Version: ${packageJson.version}`,
    );

    let serverAbsolutePath: string | null = null;
    try {
      console.log(
        `Resolving initial CLI path using bundled version: ${packageJson.version}`,
      );
      bamlOutputChannel.appendLine(
        `Resolving initial CLI path using bundled version: ${packageJson.version}`,
      );
      serverAbsolutePath = await resolveCliPath(
        context,
        packageJson.version,
        bamlOutputChannel,
      );
    } catch (e) {
      console.error('Error resolving initial CLI path during activation:', e);
      // Ensure error message is a string
      const activationErrorMessage = e instanceof Error ? e.message : String(e);
      bamlOutputChannel.appendLine(
        `ERROR: Error resolving initial CLI path during activation: ${activationErrorMessage}`,
      );
    }

    if (!serverAbsolutePath) {
      console.error(
        'BAML Language Server activation failed: Could not resolve executable path.',
      );
      bamlOutputChannel.appendLine(
        'ERROR: BAML Language Server activation failed: Could not resolve executable path.',
      );
      window.showErrorMessage(
        'BAML Language Server failed to start: Could not find or download required executable.',
      );
      return;
    }
    console.log(
      `Initial BAML Language Server path resolved to: ${serverAbsolutePath}`,
    );
    bamlOutputChannel.appendLine(
      `Initial BAML Language Server path resolved to: ${serverAbsolutePath}`,
    );
    // Initialize the module-level variable with the initial path
    currentExecutingCliPath = serverAbsolutePath;

    const serverOptions: ServerOptions = {
      run: {
        command: serverAbsolutePath,
        args: ['lsp'],
        options: { env: process.env },
      },
      debug: {
        command: serverAbsolutePath,
        args: ['lsp'],
        options: debugOptions,
      },
    };
    const clientOptions = getClientOptions();

    context.subscriptions.push(
      commands.registerCommand('baml.restartLanguageServer', async () => {
        console.log("Manual 'baml.restartLanguageServer' command triggered.");
        window.showInformationMessage('Restarting BAML Language Server...');
        try {
          const currentVersion =
            BAML_CONFIG_SINGLETON.cliVersion || packageJson.version;
          console.log(
            `Manual restart: Resolving CLI path for version ${currentVersion}`,
          );
          bamlOutputChannel.appendLine(
            `Manual restart: Resolving CLI path for version ${currentVersion}`,
          );
          const resolvedPath = await resolveCliPath(
            context,
            currentVersion,
            bamlOutputChannel,
          );

          if (resolvedPath) {
            const restartServerOptions: ServerOptions = {
              run: {
                command: resolvedPath,
                args: ['lsp'],
                options: { env: process.env },
              },
              debug: {
                command: resolvedPath,
                args: ['lsp'],
                options: debugOptions,
              },
            };
            const restartClientOptions = getClientOptions();
            // activateClient will handle stopping the old client, creating/starting the new one,
            // and managing clientReady, event handlers, and diagnostics via its onReady handlers.
            activateClient(context, restartServerOptions, restartClientOptions);
            currentExecutingCliPath = resolvedPath;
            window.showInformationMessage(
              `BAML Language Server (v${currentVersion}) restarted manually.`,
            );
            bamlOutputChannel.appendLine(
              `BAML Language Server (v${currentVersion}) restarted manually.`,
            );
          } else {
            window.showErrorMessage(
              `Manual restart failed: Could not resolve executable path for version ${currentVersion}.`,
            );
            bamlOutputChannel.appendLine(
              `ERROR: Manual restart failed: Could not resolve executable path for version ${currentVersion}.`,
            );
          }
        } catch (e) {
          console.error('Error during manual restart:', e);
          // Ensure error message is a string
          const manualRestartErrorMessage =
            e instanceof Error ? e.message : String(e);
          bamlOutputChannel.appendLine(
            `ERROR: Error during manual restart: ${manualRestartErrorMessage}`,
          );
          window.showErrorMessage(
            'Failed to manually restart Baml language server.',
          );
        }
      }),

      commands.registerCommand('baml.checkForUpdates', () => {
        checkForUpdates({ showIfNoUpdates: true });
      }),

      commands.registerCommand(
        'baml.selectTestCase',
        async (test_request: {
          functionName?: string;
          testCaseName?: string;
        }) => {
          const { functionName, testCaseName } = test_request;
          if (!functionName || !testCaseName) {
            console.warn(
              'selectTestCase command called without functionName or testCaseName',
            );
            return;
          }
          if (!clientReady) {
            console.warn('Client not ready for selectTestCase');
            return;
          }
          try {
            console.log(
              'Sending selectTestCase request:',
              functionName,
              testCaseName,
            );
            await client.sendRequest('selectTestCase', {
              functionName,
              testCaseName,
            });
          } catch (e) {
            console.error('selectTestCase request failed:', e);
          }
        },
      ),

      commands.registerCommand(
        'baml.jumpToDefinition',
        async (args: { file_path: string; start: number; end: number }) => {
          if (!args || !args.file_path) {
            vscode.window.showErrorMessage(
              'Jump to definition failed: Invalid arguments provided.',
            );
            return;
          }
          try {
            const uri = vscode.Uri.file(args.file_path);
            const doc = await vscode.workspace.openTextDocument(uri);
            const start = doc.positionAt(args.start);
            const end = doc.positionAt(args.end);
            const range = new vscode.Range(start, end);
            await vscode.window.showTextDocument(doc, {
              selection: range,
              viewColumn: vscode.ViewColumn.Beside,
            });
          } catch (error: any) {
            vscode.window.showErrorMessage(
              `Error navigating to function definition: ${error.message || error}`,
            );
          }
        },
      ),

      commands.registerCommand('baml.setDefaultFormatter', async () => {
        enum AutoFormatChoice {
          Yes = 'Yes (always)',
          OnlyInWorkspace = 'Yes (in workspace)',
          No = 'No',
        }
        const selection = await vscode.window.showInformationMessage(
          'Would you like to auto-format BAML files on save?',
          { modal: true },
          AutoFormatChoice.Yes,
          AutoFormatChoice.OnlyInWorkspace,
          AutoFormatChoice.No,
        );
        if (selection === AutoFormatChoice.No) {
          return;
        }

        const config = vscode.workspace.getConfiguration('editor', {
          languageId: 'baml',
        });

        const configTarget =
          selection === AutoFormatChoice.Yes
            ? vscode.ConfigurationTarget.Global
            : vscode.ConfigurationTarget.Workspace;
        const overrideInLanguage = true;

        for (const [key, value] of Object.entries({
          defaultFormatter: 'Boundary.baml-extension',
          formatOnSave: true,
        })) {
          await config.update(key, value, configTarget, overrideInLanguage);
        }

        switch (selection) {
          case AutoFormatChoice.Yes:
            vscode.window.showInformationMessage(
              'BAML files will now be auto-formatted on save (updated user settings).',
            );
            break;
          case AutoFormatChoice.OnlyInWorkspace:
            vscode.window.showInformationMessage(
              'BAML files will now be auto-formatted on save (updated workspace settings).',
            );
            break;
        }
      }),
    );

    activateClient(context, serverOptions, clientOptions);

    if (!isDebugOrTest) {
      try {
        const extensionId = `Boundary.${packageJson.name}`;
        const extensionVersion: string = packageJson.version;
        console.log(
          `Initializing telemetry for ${extensionId} v${extensionVersion}`,
        );
        telemetry = new TelemetryReporter(extensionId, extensionVersion);
        context.subscriptions.push(telemetry);
        await telemetry.initialize();
        console.log('Telemetry initialized.');
      } catch (err) {
        console.error('Failed to initialize telemetry:', err);
      }
    }

    checkForMinimalColorTheme();
    console.log('BAML Language Server plugin activation finished.');
  },
  deactivate: async () => {
    console.log('Deactivating BAML Language Server plugin...');
    if (client?.needsStop()) {
      try {
        await client.stop();
        console.log('Language client stopped.');
      } catch (error) {
        console.error('Error stopping language client:', error);
      }
    } else {
      console.log('Client not running or already stopped.');
    }

    if (!isDebugOrTestSession() && telemetry) {
      console.log('Disposing telemetry.');
      await telemetry.dispose().catch((err) => {
        console.error('Error disposing telemetry:', err);
      });
    }

    while (intervalTimers.length > 0) {
      clearInterval(intervalTimers.pop());
    }
    console.log('Cleared interval timers.');
    console.log('Deactivation finished.');
    return undefined;
  },
};

export { telemetry };
export default plugin;
