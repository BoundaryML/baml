/* eslint-disable @typescript-eslint/no-misused-promises */
import * as vscode from 'vscode';
import glooLens from './LanguageToBamlCodeLensProvider';
import StatusBarPanel from './panels/StatusBarPanel';
import { WebviewPanelHost } from './panels/WebviewPanelHost';
import plugins from './plugins';
import {
  getPlaygroundPort,
  requestBamlCLIVersion,
  requestDiagnostics,
} from './plugins/language-server-client';
import {
  runTestInPlayground,
  telemetry,
  viewFunctionInPlayground,
} from './plugins/language-server-client';

const outputChannel = vscode.window.createOutputChannel('baml');
const diagnosticsCollection =
  vscode.languages.createDiagnosticCollection('baml-diagnostics');
const LANG_NAME = 'Baml';

let glowOnDecoration: vscode.TextEditorDecorationType | null = null;
let glowOffDecoration: vscode.TextEditorDecorationType | null = null;
let isGlowOn = true;
let animationTimer: NodeJS.Timeout | null = null;
let highlightRanges: vscode.Range[] = [];

export function activate(context: vscode.ExtensionContext) {
  console.log('BAML extension activating');

  vscode.workspace.getConfiguration('baml');

  context.subscriptions.push(StatusBarPanel.instance);

  // Initialize the highlight effect.
  createDecorations();
  startAnimation();

  // Wrapper function to handle null case from getPlaygroundPort
  const getPort = () => {
    const port = getPlaygroundPort();
    return port ?? 3030; // Default to 3030 if null
  };

  const bamlPlaygroundCommand = vscode.commands.registerCommand(
    'baml.openBamlPanel',
    async (args?: {
      projectId?: string;
      functionName?: string;
      implName?: string;
      showTests?: boolean;
    }) => {
      const config = vscode.workspace.getConfiguration();
      config.update(
        'baml.bamlPanelOpen',
        true,
        vscode.ConfigurationTarget.Global,
      );
      WebviewPanelHost.render(context.extensionUri, getPort, telemetry);

      if (telemetry) {
        telemetry.sendTelemetryEvent({
          event: 'baml.openBamlPanel',
          properties: {},
        });
      }

      // Call the LSP to change function if a function name is provided
      if (args?.functionName) {
        try {
          await viewFunctionInPlayground(args);
        } catch (error) {
          console.error('Failed to notify LSP of function change:', error);
          // Continue execution even if LSP notification fails
        }
      }

      console.info('Opening BAML panel');
    },
  );

  const bamlTestcaseCommand = vscode.commands.registerCommand(
    'baml.runBamlTest',
    async (args?: {
      projectId: string;
      functionName?: string;
      implName?: string;
      showTests?: boolean;
      testCaseName?: string;
    }) => {
      WebviewPanelHost.render(context.extensionUri, getPort, telemetry);
      if (telemetry) {
        telemetry.sendTelemetryEvent({
          event: 'baml.runBamlTest',
          properties: {},
        });
      }

      // Call the LSP to run test if test case name is provided
      if (args?.testCaseName && args?.functionName && args?.projectId) {
        try {
          await runTestInPlayground(args);
        } catch (error) {
          console.error('Failed to notify LSP of test run:', error);
          // Continue execution even if LSP notification fails
        }
      }

      // Call the LSP to run test if test case name is provided
      if (args?.testCaseName && args?.functionName && args?.projectId) {
        try {
          await runTestInPlayground(args);
        } catch (error) {
          console.error('Failed to notify LSP of test run:', error);
          // Continue execution even if LSP notification fails
        }
      }

      // sends project files as well to webview
      requestDiagnostics();

      console.info('Opening BAML panel');
    },
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      'baml.setFlashingRegions',
      (params: {
        content: {
          spans: {
            file_path: string;
            start_line: number;
            start: number;
            end_line: number;
            end: number;
          }[];
        };
      }) => {
        console.log('args:', params);
        // A helpful thing to toggle on for debugging:
        console.log('HANDLER setFlashingRegions', params);
        // vscode.window.showWarningMessage(`setFlashingRegions:` + JSON.stringify(params))

        // Focus the editor to ensure styling updates are applied rapidly.
        if (vscode.window.activeTextEditor) {
          vscode.window.showTextDocument(
            vscode.window.activeTextEditor.document,
            vscode.window.activeTextEditor.viewColumn,
          );
        }

        context.subscriptions.push({
          dispose: () => {
            stopAnimation();
            if (glowOnDecoration) glowOnDecoration.dispose();
            if (glowOffDecoration) glowOffDecoration.dispose();
          },
        });
        const ranges = params.content.spans.map((span) => {
          const start = new vscode.Position(span.start_line, span.start);
          const end = new vscode.Position(span.end_line, span.end);
          return new vscode.Range(start, end);
        });
        highlightRanges = ranges;
        updateHighlight();
      },
    ),
  );

  context.subscriptions.push(bamlPlaygroundCommand);
  console.log('pushing glooLens');

  const pythonSelector = { language: 'python', scheme: 'file' };
  const typescriptSelector = { language: 'typescript', scheme: 'file' };
  const reactSelector = { language: 'typescriptreact', scheme: 'file' };

  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider(pythonSelector, glooLens),
    vscode.languages.registerCodeLensProvider(typescriptSelector, glooLens),
    vscode.languages.registerCodeLensProvider(reactSelector, glooLens),
  );

  context.subscriptions.push(diagnosticsCollection);

  vscode.window.onDidChangeActiveTextEditor((event) => {
    // makes it so we reload the project. Could probably be called reloadProjectFiles or something. This is because we may be clicking into a different file in a separate baml_src.
    requestDiagnostics();
  });

  // Add cursor movement listener
  vscode.window.onDidChangeTextEditorSelection((event) => {
    const position = event.selections[0]?.active ?? new vscode.Position(0, 0);

    const editor = vscode.window.activeTextEditor;

    if (editor) {
      const name = editor.document.fileName;
      if (name.endsWith('.baml')) {
        const text = editor.document.getText();

        // TODO: buggy when used with multiple functions, needs a fix.
        // Cursor position tracking removed since we're using iframe approach
      }
    }
  });

  const config = vscode.workspace.getConfiguration('editor', {
    languageId: 'baml',
  });
  if (!config.get('defaultFormatter')) {
    // TODO: once the BAML formatter is stable, we should auto-prompt people to set it as the default formatter.
    // void vscode.commands.executeCommand('baml.setDefaultFormatter')
  }

  // Listen for messages from the webview

  plugins.map(async (plugin) => {
    try {
      const enabled = await plugin.enabled();
      if (enabled) {
        console.log(`Activating ${plugin.name}`);
        if (plugin.activate) {
          await plugin.activate(context, outputChannel);
        }
      } else {
        console.log(`${plugin.name} is Disabled`);
      }
    } catch (error) {
      console.error(`Error activating ${plugin.name}:`, error);
    }
  });

  if (process.env.VSCODE_DEBUG_MODE === 'true') {
    console.log(`vscode env: ${JSON.stringify(process.env, null, 2)}`);
    vscode.commands.executeCommand('baml.openBamlPanel');
  }

  setInterval(() => {
    requestBamlCLIVersion();
  }, 30000);

  // TODO: Reactivate linter.
  // runDiagnostics();
}

export function deactivate(): void {
  console.log('BAML extension deactivating');
  diagnosticsCollection.clear();
  diagnosticsCollection.dispose();
  StatusBarPanel.instance.dispose();
  for (const plugin of plugins) {
    if (plugin.deactivate) {
      void plugin.deactivate();
    }
  }
  // server?.close();
}

// Create our two decoration states
function createDecorations() {
  // Bright neon color for the glow effect (bright green)
  const glowColor = '#00FF00';
  const offColor = '#009900';

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
  });

  // Glow OFF - text glow with textDecoration property.
  glowOffDecoration = vscode.window.createTextEditorDecorationType({
    color: offColor,
    fontWeight: 'bold',
    backgroundColor: 'transparent',
    textDecoration: 'none; ',
    // Try using before/after elements to reinforce the glow effect
    before: {
      contentText: '',
      textDecoration: 'none; ',
      color: offColor,
    },
    after: {
      contentText: '',
      textDecoration: 'none; ',
      color: offColor,
    },
  });
}

// Update the highlight based on current state
function updateHighlight() {
  // vscode.window.showWarningMessage(`updateHighlight:` +  isGlowOn)
  const editor = vscode.window.activeTextEditor;
  if (!editor) return;

  // Clear both decorations
  // Apply appropriate decoration based on state
  if (glowOnDecoration && glowOffDecoration && isGlowOn) {
    editor.setDecorations(glowOffDecoration, []);
    editor.setDecorations(glowOnDecoration, highlightRanges);
  }
  if (glowOnDecoration && glowOffDecoration && !isGlowOn) {
    editor.setDecorations(glowOnDecoration, []);
    editor.setDecorations(glowOffDecoration, highlightRanges);
  }
}

// Start the simple toggling animation
function startAnimation() {
  console.log('startAnimation');
  if (animationTimer) return;

  // Toggle every 500ms (2 times per second)
  animationTimer = setInterval(() => {
    // Toggle between on and off states
    isGlowOn = !isGlowOn;

    // Update the highlight
    updateHighlight();
  }, 500); // 500ms = half a second
}

// Stop animation
function stopAnimation(): void {
  if (animationTimer) {
    clearInterval(animationTimer);
    animationTimer = null;
  }
}
