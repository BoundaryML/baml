/**
 * Message Handlers for EventListener
 *
 * These functions translate IDE/LSP messages into SDK method calls.
 * Platform-specific quirks are kept here (not in SDK).
 *
 * Phase 5: EventListener Refactor
 */

import type { BAMLSDK } from '../sdk';
import type { VscodeToWebviewCommand } from './vscode-to-webview-rpc';

type IDEPayload = Extract<
  VscodeToWebviewCommand,
  { source: 'ide_message' }
>['payload'];

type UpdateCursorContent = Extract<
  IDEPayload,
  { command: 'update_cursor' }
>['content'];

/**
 * Handle IDE messages (source: 'ide_message')
 */
export async function handleIDEMessage(
  sdk: BAMLSDK,
  payload: IDEPayload,
  debouncedUpdateCursor: (cursor: UpdateCursorContent) => void,
  // Non-core state setters (atoms that SDK doesn't manage)
  setBamlCliVersion: (version: string) => void,
  setBamlConfig: (config: any) => void
): Promise<void> {
  const { command, content } = payload;

  console.debug('aaron: [MessageHandler] IDE message:', command);

  switch (command) {
    case 'update_cursor':
      // Use SDK navigation update cursor method
      const updateCursorHandler =
        debouncedUpdateCursor ??
        ((cursor: UpdateCursorContent) => {
          sdk.navigation.updateCursor(cursor);
        });
      updateCursorHandler(content);
      break;

    case 'baml_settings_updated':
      // Update config atom directly (non-core state)
      setBamlConfig(content);
      // Also update SDK's vscodeSettingsAtom so proxyUrlAtom updates correctly
      if (content?.config) {
        sdk.settings.updateVSCodeSettings({
          enablePlaygroundProxy: content.config.enablePlaygroundProxy,
          featureFlags: content.config.featureFlags,
        });
      }
      break;

    case 'baml_cli_version':
      // Update CLI version atom directly (non-core state)
      setBamlCliVersion(content);
      break;

    default:
      console.warn('[MessageHandler] Unknown IDE command:', command);
  }
}

/**
 * Handle LSP messages (source: 'lsp_message')
 */
export async function handleLSPMessage(
  sdk: BAMLSDK,
  payload: Extract<VscodeToWebviewCommand, { source: 'lsp_message' }>['payload'],
  // Debounced file update function (platform quirk)
  debouncedUpdateFiles: (files: Record<string, string>) => void,
  // Non-core state setters
  setBamlConfig: (config: any | ((prev: any) => any)) => void
): Promise<void> {
  const { method, params } = payload;

  console.debug('[MessageHandler] LSP message:', method);

  switch (method) {
    case 'runtime_updated':
      const simpleBamlEntry = Object.entries(params.files).find(([name]) => name.endsWith('simple.baml'));
      if (simpleBamlEntry) {
        const [name, content] = simpleBamlEntry;
      }
      // Debounce file updates to prevent excessive WASM recompilation
      // This is a platform quirk (LSP sends rapid updates during typing)
      debouncedUpdateFiles(
        Object.fromEntries(
          Object.entries(params.files).map(([name, content]) => [name, content])
        )
      );
      break;

    case 'baml_settings_updated':
      // Update config atom via merge (non-core state)
      setBamlConfig(({ config: prevConfig, ...prevRest }: any) => {
        const newConfig = {
          ...prevRest,
          config: { ...prevConfig, ...params },
        };
        console.debug('[MessageHandler] baml_settings_updated', {
          prevConfig,
          params,
          newConfig,
        });
        return newConfig;
      });
      // Also update SDK's vscodeSettingsAtom so proxyUrlAtom updates correctly
      // params is Partial<BamlConfigAtom>, so config is nested
      if (params?.config) {
        sdk.settings.updateVSCodeSettings({
          enablePlaygroundProxy: params.config.enablePlaygroundProxy,
          featureFlags: params.config.featureFlags,
        });
      }
      break;

    case 'workspace/executeCommand':
      await handleWorkspaceCommand(sdk, params);
      break;

    case 'textDocument/codeAction': {
      const { textDocument, range } = params;
      // TODO: This needs testing for escaped file paths!
      const fileName = textDocument.uri.replace('file://', '');
      sdk.navigation.updateCursorFromRange({
        fileName,
        start: { line: range.start.line, character: range.start.character },
        end: { line: range.end.line, character: range.end.character },
      });
      break;
    }

    default:
      console.warn('[MessageHandler] Unknown LSP method:', method);
  }
}

/**
 * Handle workspace commands (workspace/executeCommand)
 */
async function handleWorkspaceCommand(
  sdk: BAMLSDK,
  params: {
    command: string;
    arguments: Array<{
      functionName?: string;
      testCaseName?: string;
      workflowId?: string;
      inputs?: Record<string, unknown>;
    }>;
  }
): Promise<void> {
  const { command, arguments: args } = params;
  const [firstArg] = args;

  console.debug('[MessageHandler] Workspace command:', command, firstArg);

  try {
    switch (command) {
      case 'baml.openBamlPanel': {
        if (!firstArg?.functionName) {
          console.warn('[MessageHandler] baml.openBamlPanel: missing functionName');
          return;
        }
        // Use SDK navigation method
        sdk.navigation.selectFunction(firstArg.functionName);
        break;
      }

      case 'baml.runBamlTest': {
        if (!firstArg?.functionName || !firstArg?.testCaseName) {
          console.warn('[MessageHandler] baml.runBamlTest: missing functionName or testCaseName');
          return;
        }

        // If runtime isn't ready yet (webview is initializing), queue the command
        // to be executed after runtime initialization completes
        if (!sdk.isRuntimeReady()) {
          sdk.queueTestCommand(firstArg.functionName, firstArg.testCaseName);
          return;
        }

        // First select the function
        sdk.navigation.selectFunction(firstArg.functionName);

        // NB(sam): without this timeout, jetbrains hits "recursive use of an object"
        // This is a JetBrains-specific platform quirk
        setTimeout(async () => {
          try {
            await sdk.tests.runAll([{ functionName: firstArg.functionName!, testName: firstArg.testCaseName! }]);
          } catch (error) {
            console.error('[MessageHandler] Test execution failed:', error);
          }
        }, 1000);
        break;
      }

      case 'baml.executeWorkflow': {
        if (!firstArg?.workflowId) {
          console.warn('[MessageHandler] baml.executeWorkflow: missing workflowId');
          return;
        }
        // Use SDK execution method
        await sdk.executions.start(
          firstArg.workflowId,
          firstArg.inputs || {},
          { clearCache: false }
        );
        break;
      }

      default:
        console.warn('[MessageHandler] Unknown workspace command:', command);
    }
  } catch (error) {
    console.error('[MessageHandler] Command execution failed:', {
      command,
      args: firstArg,
      error,
    });
    // Don't throw - let EventListener continue processing other messages
  }
}
