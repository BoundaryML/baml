import * as os from 'node:os';
import * as path from 'node:path';

import type { } from '@baml/common';
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
import { getCurrentOpenedFile, LAST_ACTIVE_BAML_FILE } from '../../helpers/get-open-file';
import StatusBarPanel from '../../panels/StatusBarPanel';
import { WebviewPanelHost } from '../../panels/WebviewPanelHost';
import TelemetryReporter from '../../telemetryReporter';
import {
  checkForMinimalColorTheme,
  createLanguageServer,
} from '../../util';
import type { BamlVSCodePlugin } from '../types';
import {
  BAML_CONFIG_SINGLETON,
  refreshBamlConfigSingleton,
} from './bamlConfig';
import { resolveCliPath } from './cliDownloader';

export { BAML_CONFIG_SINGLETON as bamlConfig };

/**
 * Path comparison that handles Windows case insensitivity.
 * @param childPath The path that should be inside the parent
 * @param parentPath The parent/root path
 * @returns true if childPath is within parentPath
 */
const isPathWithinParent = (childPath: string, parentPath: string): boolean => {
  try {
    // Normalize both paths to handle case sensitivity and path separators
    const normalizedChild = path.resolve(childPath);
    const normalizedParent = path.resolve(parentPath);

    // Use path.relative to check containment
    const relativePath = path.relative(normalizedParent, normalizedChild);

    // If the relative path doesn't start with ".." and isn't an absolute path,
    // then the child is within the parent
    return !relativePath.startsWith('..') && !path.isAbsolute(relativePath);
  } catch (e) {
    console.error('Error comparing paths:', e);
    return false;
  }
};

/**
 * Shared helper for LSP restart logic.
 * @param options Configuration for the restart operation
 * @returns Promise that resolves when restart is complete or fails
 */
const executeLanguageServerRestart = async (options: {
  context: ExtensionContext;
  version: string;
  targetCliPath: string;
  isManualRestart?: boolean;
  reason?: string;
}): Promise<void> => {
  const { context, version, targetCliPath, isManualRestart = false, reason } = options;

  // Prevent concurrent restarts
  if (isRestarting) {
    const message = isManualRestart
      ? 'BAML Language Server restart already in progress. Please wait...'
      : `baml_src_generator_version ignored: LSP restart already in progress for version ${version}`;

    if (isManualRestart) {
      window.showWarningMessage(message);
    }
    bamlOutputChannel.appendLine(message);
    return;
  }

  // Set the restarting flag
  isRestarting = true;

  try {
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

    const progressTitle = isManualRestart
      ? `Restarting BAML Language Server (v${version})...`
      : `Restarting BAML Language Server (v${version})...`;

    const operation = async () => {
      console.log(`Calling activateClient for ${isManualRestart ? 'manual' : 'automatic'} restart...`);
      // clientReady will be managed by activateClient's onReady handlers.
      // activateClient also handles stopping the previous client.
      activateClient(context, serverOptionsForRestart, clientOptionsForRestart);
      console.log(`activateClient called for version ${version}.`);
      currentExecutingCliPath = targetCliPath;
      BAML_CONFIG_SINGLETON.cliVersion = version; // This might be better set after onReady, or via a message from the client

      const successMessage = isManualRestart
        ? `BAML Language Server (v${version}) restarted manually.`
        : `BAML Language Server reload initiated for version ${version}.`;

      bamlOutputChannel?.appendLine(successMessage);

      if (isManualRestart) {
        window.showInformationMessage(successMessage);
      }
    };

    if (isManualRestart) {
      await operation();
    } else {
      await window.withProgress(
        {
          location: vscode.ProgressLocation.Notification,
          cancellable: false,
          title: progressTitle,
        },
        async () => {
          await operation();
        },
      );
    }
  } catch (e) {
    clientReady = false; // Ensure clientReady is false if restart fails
    console.error(`Error during ${isManualRestart ? 'manual' : 'automatic'} restart:`, e);
    // Ensure error message is a string
    const errorMessage = e instanceof Error ? e.message : String(e);
    const logMessage = `ERROR: Error during ${isManualRestart ? 'manual' : 'automatic'} restart for version ${version}: ${errorMessage}`;
    bamlOutputChannel?.appendLine(logMessage);

    const userMessage = isManualRestart
      ? 'Failed to manually restart Baml language server.'
      : `Failed to restart BAML Language Server to version ${version}.`;

    window.showErrorMessage(userMessage);
  } finally {
    // Clear the restarting flag regardless of success or failure
    isRestarting = false;
  }
};

let clientReady = false;

let client: LanguageClient;
let telemetry: TelemetryReporter;
const intervalTimers: NodeJS.Timeout[] = [];
let bamlOutputChannel: OutputChannel;
// Variable to store the path of the currently executing CLI
let currentExecutingCliPath: string | null = null;
// Flag to prevent concurrent LSP restarts
let isRestarting = false;
// Flag to ensure periodic version report interval is only set once
let periodicVersionReportScheduled = false;

// Track last known CLI version and generator info for telemetry
let lastKnownCliVersion: string | null = null;
let lastKnownGenerators: Array<{ name: string; output_type: string }> = [];

const isDebugMode = () => process.env.VSCODE_DEBUG_MODE === 'true';
const isE2ETestOnPullRequest = () => process.env.PRISMA_USE_LOCAL_LS === 'true';

/**
 * Helper to track CLI resolution telemetry
 */
const trackCliResolution = async (
  version: string,
  resolveFn: () => Promise<string | null>,
): Promise<string | null> => {
  const startTime = Date.now();
  try {
    const result = await resolveFn();
    const duration = Date.now() - startTime;

    if (result) {
      // Update last known version on successful resolution
      lastKnownCliVersion = version;

      telemetry?.sendTelemetryEvent({
        event: 'baml.cli.resolve.success',
        properties: {
          version,
          duration_ms: duration,
        },
      });
    } else {
      telemetry?.sendTelemetryEvent({
        event: 'baml.cli.resolve.failure',
        properties: {
          version,
          duration_ms: duration,
        },
      });
    }

    return result;
  } catch (error) {
    const duration = Date.now() - startTime;
    telemetry?.sendTelemetryEvent({
      event: 'baml.cli.resolve.error',
      properties: {
        version,
        duration_ms: duration,
        error: error instanceof Error ? error.message : String(error),
      },
    });
    throw error;
  }
};

const debugOptions = {
  execArgv: ['--nolazy', '--inspect=6009'],
  env: {
    DEBUG: true,
    RUST_BACKTRACE: 'full',
    ...process.env,
  },
};

const getClientOptions = (): LanguageClientOptions => {
  // Get current BAML settings for initialization
  const currentBamlSettings = workspace.getConfiguration('baml');

  return {
    documentSelector: [
      { scheme: 'file', language: 'baml' },
      { language: 'json', pattern: '**/baml_src/**' },
    ],
    outputChannel: vscode.window.createOutputChannel('Baml Language Server'),
    revealOutputChannelOn: RevealOutputChannelOn.Never,
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher('**/baml_src/**/*.baml'),
    },
    initializationOptions: {
      baml: {
        ...currentBamlSettings,
        lspMethodsToForwardToWebview: [], // Empty - VSCode handles webview communication directly
      }
    },
  };
};

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

export const publishBamlVersionReport = async (): Promise<string | undefined> => {
  if (!clientReady) {
    console.warn('Client not ready for CLI version request')
    return undefined
  }
  try {
    // Send periodic telemetry with version and generators
    if (telemetry && lastKnownCliVersion) {
      let generatorLanguage: string | undefined = undefined;
      if (Array.isArray(lastKnownGenerators)) {
        if (lastKnownGenerators.length === 0) {
          console.warn('No generators found for telemetry');
          bamlOutputChannel.appendLine('no_generator found for baml telemetry');
          generatorLanguage = 'no_generator';
        } else {
          try {
            const first = lastKnownGenerators[0];
            const allHaveOutputType = lastKnownGenerators.every(
              g => g && typeof g.output_type === 'string'
            );
            if (allHaveOutputType && first && typeof first.output_type === 'string') {
              const allSame = lastKnownGenerators.every(
                g => g.output_type === first.output_type
              );
              generatorLanguage = allSame ? first.output_type : 'multiple';
            } else {
              generatorLanguage = undefined;
            }
          } catch (err) {
            console.warn('Error determining generator language for telemetry:', err);
            generatorLanguage = undefined;
          }
        }
      }

      telemetry.sendTelemetryEvent({
        event: 'baml.version_report',
        properties: {
          extension: "vscode",
          ide_version: vscode.version,
          cli_version: lastKnownCliVersion,
          generators: lastKnownGenerators,
          generator_language: generatorLanguage,
        },
      });
    } else {
      console.warn('No telemetry reporter or last known CLI version');
    }

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

  // This seems to be unused, I don't see these log lines anywhere
  // client.onRequest('executeCommand', async (command: string) => {
  //   try {
  //     console.log('Executing command requested by LSP:', command);
  //     await vscode.commands.executeCommand(command);
  //   } catch (e) {
  //     console.error(
  //       `Error executing command '${command}' requested by LSP:`,
  //       e,
  //     );
  //   }
  // });

  client.onRequest('baml_settings_updated', (config: typeof BAML_CONFIG_SINGLETON) => {
    console.log('Received baml_settings_updated from LSP:', config)
    BAML_CONFIG_SINGLETON.config = config.config
    BAML_CONFIG_SINGLETON.cliVersion = config.cliVersion
    WebviewPanelHost.currentPanel?.sendCommandToWebview({
      source: 'ide_message',
      payload: {
        command: 'baml_settings_updated',
        content: BAML_CONFIG_SINGLETON,
      }
    })
  })

  const handleRuntimeUpdated = (params: { root_path: string; files: Record<string, string> }) => {
    WebviewPanelHost.currentPanel?.sendCommandToWebview({
      source: 'lsp_message',
      payload: {
        method: 'runtime_updated',
        params: params,
      }
    })
  }

  client.onRequest('runtime_updated', handleRuntimeUpdated);
  client.onNotification('runtime_updated', handleRuntimeUpdated);

  // eslint-disable-next-line @typescript-eslint/no-misused-promises
  client.onNotification(
    'baml_src_generator_version',
    async (payload: {
      version: string;
      root_path: string;
      generators?: Array<{ name: string; output_type: string }>;
    }) => {
      try {
        bamlOutputChannel.appendLine(
          `============ baml_src_generator_version notification: ${payload.version} ${payload.root_path} ${JSON.stringify(payload.generators ?? [])}`,
        );

        // Store the version and generators for telemetry
        lastKnownCliVersion = payload.version;
        if (payload.generators) {
          lastKnownGenerators = payload.generators;
        }

        // Check if this version update is for the currently active baml_src directory
        const activeEditor = LAST_ACTIVE_BAML_FILE.uri;
        if (activeEditor) {
          try {

            const currentFilePath = activeEditor.fsPath;

            const rootPathUri = URI.file(payload.root_path).fsPath;
            if (!isPathWithinParent(currentFilePath, rootPathUri)) {
              bamlOutputChannel.appendLine(
                `baml_src_generator_version ignored: root path does not match active editor ${currentFilePath} root: ${rootPathUri}`,
              );
              return;
            }
          } catch (e) {
            console.error('Error checking if root path matches active editor:', e);
            bamlOutputChannel.appendLine(
              `ERROR: Error checking if root path matches active editor: ${e}`,
            );
            return;
          }
        } else {
          bamlOutputChannel.appendLine(
            'baml_src_generator_version ignored: no active editor',
          );
          return;
        }

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
        const targetCliPath = await trackCliResolution(version, () =>
          resolveCliPath(context, version, bamlOutputChannel),
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

          await executeLanguageServerRestart({
            context,
            version,
            targetCliPath,
            isManualRestart: false,
            reason: 'generator version update',
          });
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
  console.log('Client Options initialization options:', JSON.stringify(clientOptions.initializationOptions, null, 2));

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
      // Clear the restarting flag when client is ready
      isRestarting = false;

      registerClientEventHandlers(client, context);
      console.log('Client event handlers registered.');

      // Set up configuration change listener to send settings to LSP
      context.subscriptions.push(
        workspace.onDidChangeConfiguration((event) => {
          if (event.affectsConfiguration('baml')) {
            const config = workspace.getConfiguration('baml');
            const featureFlags = config.get('featureFlags', ['beta']);

            const bamlSettings = {
              featureFlags: featureFlags,
              enablePlaygroundProxy: config.get('enablePlaygroundProxy', true),
              generateCodeOnSave: config.get('generateCodeOnSave', 'always'),
              restartTSServerOnSave: config.get('restartTSServerOnSave', false),
              fileWatcher: config.get('fileWatcher', false),
              trace: config.get('trace', { server: 'off' }),
            };
            console.log('Constructed bamlSettings:', JSON.stringify(bamlSettings, null, 2));
            console.log('Sending configuration update to LSP:', bamlSettings);
            client.sendNotification('workspace/didChangeConfiguration', {
              settings: { baml: bamlSettings }
            });
          }
        })
      );

      // Send initial configuration after a small delay to ensure LSP is fully ready
      setTimeout(() => {
        const config = workspace.getConfiguration('baml');
        const initialBamlSettings = {
          featureFlags: config.get('featureFlags', ['beta']),
          enablePlaygroundProxy: config.get('enablePlaygroundProxy', true),
          generateCodeOnSave: config.get('generateCodeOnSave', 'always'),
          restartTSServerOnSave: config.get('restartTSServerOnSave', false),
          fileWatcher: config.get('fileWatcher', false),
          trace: config.get('trace', { server: 'off' }),
        };

        console.log('Sending initial configuration to LSP:', initialBamlSettings);
        client.sendNotification('workspace/didChangeConfiguration', {
          settings: { baml: initialBamlSettings }
        });
      }, 100); // 100ms delay

      requestDiagnostics().catch((e) =>
        console.error('Error requesting initial diagnostics:', e),
      );

      if (isDebugMode()) {
        client.outputChannel.show(true);
      }

      if (!periodicVersionReportScheduled) {
        periodicVersionReportScheduled = true;
        console.log('Setting up periodic update checks.');
        // Publish the first version report after 5 min of extension activation.
        setTimeout(() => {
          publishBamlVersionReport();
        }, 5 * 60 * 1000);

        intervalTimers.push(
          setInterval(
            () => {
              console.log(
                `Periodic version report triggered: ${new Date().toString()}`,
              );
              publishBamlVersionReport();
            },
            2 * 60 * 60 * 1000 /* 2h */,
          ),
        );
      }
    })
    .catch((error) => {
      console.error('Language client failed to become ready:', error);
      clientReady = false;
      // Clear the restarting flag on failure as well
      isRestarting = false;
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
    const isDebugOrTest = isDebugMode();
    bamlOutputChannel = _outputChannel;
    context.subscriptions.push(bamlOutputChannel);
    bamlOutputChannel.appendLine('Activating BAML Language Server plugin...');


    console.log('Activating BAML Language Server plugin...');
    bamlOutputChannel.appendLine(`Debug/Test Session: ${isDebugOrTest}`);
    bamlOutputChannel.appendLine(
      `Bundled Extension Version: ${packageJson.version}`,
    );

    let serverAbsolutePath: string | null = null;
    if (isDebugOrTest) {
      console.log('Using debug cli in debug mode');
      bamlOutputChannel.append('Using debug cli in debug mode');
      serverAbsolutePath = process.env.VSCODE_DEBUG_BAML_CLI_PATH || null;
    } else {
      try {
        console.log(
          `Resolving initial CLI path using bundled version: ${packageJson.version}`,
        );
        bamlOutputChannel.appendLine(
          `Resolving initial CLI path using bundled version: ${packageJson.version}`,
        );
        serverAbsolutePath = await trackCliResolution(packageJson.version, () =>
          resolveCliPath(context, packageJson.version, bamlOutputChannel),
        );
      } catch (e) {
        console.error('Error resolving initial CLI path during activation:', e);
        // Ensure error message is a string
        const activationErrorMessage = e instanceof Error ? e.message : String(e);
        bamlOutputChannel.appendLine(
          `ERROR: Error resolving initial CLI path during activation: ${activationErrorMessage}`,
        );
      }
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

        const currentVersion =
          BAML_CONFIG_SINGLETON.cliVersion || packageJson.version;
        console.log(
          `Manual restart: Resolving CLI path for version ${currentVersion}`,
        );
        bamlOutputChannel.appendLine(
          `Manual restart: Resolving CLI path for version ${currentVersion}`,
        );

        try {
          const resolvedPath = await trackCliResolution(currentVersion, () =>
            resolveCliPath(context, currentVersion, bamlOutputChannel),
          );

          if (resolvedPath) {
            await executeLanguageServerRestart({
              context,
              version: currentVersion,
              targetCliPath: resolvedPath,
              isManualRestart: true,
              reason: 'manual restart command',
            });
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

    if (!isDebugMode() && telemetry) {
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
