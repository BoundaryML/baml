/* eslint-disable @typescript-eslint/no-var-requires */
/* eslint-disable @typescript-eslint/no-unsafe-assignment */
import * as vscode from 'vscode'

import plugins from './plugins'
import { WebPanelView } from './panels/WebPanelView'
import { BamlDB } from './plugins/language-server'
import testExecutor from './panels/execute_test'
import glooLens from './GlooCodeLensProvider'
import { telemetry } from './plugins/language-server'
import axios from 'axios';
const outputChannel = vscode.window.createOutputChannel('baml')
const diagnosticsCollection = vscode.languages.createDiagnosticCollection('baml-diagnostics')
const LANG_NAME = 'Baml'
let timeout: NodeJS.Timeout | undefined;
let statusBarItem: vscode.StatusBarItem;

function scheduleDiagnostics(): void {
  if (timeout) {
    clearTimeout(timeout);
  }
  timeout = setTimeout(() => {
    statusBarItem.show();

    runDiagnostics();
  }, 1000);  // 4 seconds after the last keystroke
}


interface LintRequest {
  lintingRules: string[];
  promptTemplate: string;
  promptVariables: { [key: string]: string };
}

interface LinterOutput {
  exactPhrase: string;
  reason: string;
  severity: string;
  recommendation?: string;
  recommendation_reason?: string;
  fix?: string;
}

interface LinterRuleOutput {
  diagnostics: LinterOutput[];
  ruleName: string;
}

async function runDiagnostics(): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    statusBarItem.hide();
    return;
  }

  console.log("Running diagnostics")

  statusBarItem.text = `$(sync~spin) Running AI Linter...`;
  statusBarItem.backgroundColor = '##9333ea';
  statusBarItem.color = '#ffffff';
  const text = editor.document.getText();

  const lintRequest: LintRequest = {
    lintingRules: ['Rule1', 'Rule2'],
    promptTemplate: text,
    promptVariables: {}
  };
  const diagnostics: vscode.Diagnostic[] = [];

  try {
    const response = await axios.post<LinterRuleOutput[]>('http://localhost:8000/lint', lintRequest);
    console.log('Got response:', response.data);
    const results = response.data;

    results.forEach(rule => {
      let found = false;

      rule.diagnostics.forEach(output => {
        const escapedPhrase = output.exactPhrase.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        const phrase = output.exactPhrase;
        let index = 0;
        // Find all occurrences of the phrase
        while ((index = text.indexOf(phrase, index)) !== -1) {
          found = true;
          const startPos = editor.document.positionAt(index);
          const endPos = editor.document.positionAt(index + phrase.length);
          const range = new vscode.Range(startPos, endPos);

          const diagnostic = new vscode.Diagnostic(
            range,
            `${output.reason}${output.recommendation ? ` - ${output.recommendation}` : ''}`,
            output.severity === 'error' ? vscode.DiagnosticSeverity.Error : vscode.DiagnosticSeverity.Warning
          );

          if (output.fix) {
            diagnostic.code = "[linter]" + output.fix;
          }
          diagnostic.source = rule.ruleName;

          diagnostics.push(diagnostic);
          index += phrase.length; // Move index to the end of the current found phrase to continue searching
        }

        if (!found && phrase.length > 100) {
          let subPhrase = phrase.substring(0, 100);
          index = 0; // Reset index for new search
          while ((index = text.indexOf(subPhrase, index)) !== -1) {
            const startPos = editor.document.positionAt(index);
            const endPos = editor.document.positionAt(index + subPhrase.length);
            const range = new vscode.Range(startPos, endPos);

            const diagnostic = new vscode.Diagnostic(
              range,
              `${output.reason}${output.recommendation ? ` - ${output.recommendation}` : ''}`,
              output.severity === 'error' ? vscode.DiagnosticSeverity.Error : vscode.DiagnosticSeverity.Warning
            );

            if (output.fix) {
              diagnostic.code = "[linter]" + output.fix;
            }
            diagnostic.source = rule.ruleName;

            diagnostics.push(diagnostic);
            index += subPhrase.length; // Move index to the end of the current found phrase to continue searching
          }
        }

        // const newRegex = new RegExp(`\\b${}\\b`, 'gi');
      });
    });
    console.log('Pushing test errorrrr');

    console.log('Diagnostics:', diagnostics);
    diagnosticsCollection.clear();
    diagnosticsCollection.set(editor.document.uri, diagnostics);
  } catch (error) {
    console.error('Failed to run diagnostics:', error);
    vscode.window.showErrorMessage('Failed to run diagnostics');
  }
  statusBarItem.text = `AI Linter Ready`;

  statusBarItem.hide();
}



export function activate(context: vscode.ExtensionContext) {
  console.log("BAML extension activating")

  vscode.workspace.getConfiguration('baml')
  // TODO: Reactivate linter.
  // statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  // statusBarItem.text = `AI Linter Ready`;
  // statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  const provider = new DiagnosticCodeActionProvider();
  const selector: vscode.DocumentSelector = { scheme: 'file', language: 'baml' }; // Adjust language as necessary
  const codeActionProvider = vscode.languages.registerCodeActionsProvider(selector, provider, {
    providedCodeActionKinds: [vscode.CodeActionKind.QuickFix]
  });

  context.subscriptions.push(codeActionProvider);

  const bamlPlaygroundCommand = vscode.commands.registerCommand(
    'baml.openBamlPanel',
    (args?: { projectId?: string; functionName?: string; implName?: string; showTests?: boolean }) => {
      const projectId = args?.projectId
      const initialFunctionName = args?.functionName
      const initialImplName = args?.implName
      const showTests = args?.showTests
      const config = vscode.workspace.getConfiguration()
      config.update('baml.bamlPanelOpen', true, vscode.ConfigurationTarget.Global)
      WebPanelView.render(context.extensionUri)
      telemetry.sendTelemetryEvent({
        event: 'baml.openBamlPanel',
        properties: {},
      })

      WebPanelView.currentPanel?.postMessage('setDb', Array.from(BamlDB.entries()))
      // send another request for reliability on slower machines
      // A more resilient way is to get a msg for it to finish loading but this is good enough for now
      setTimeout(() => {
        WebPanelView.currentPanel?.postMessage('setDb', Array.from(BamlDB.entries()))
      }, 2000);
      WebPanelView.currentPanel?.postMessage('setSelectedResource', {
        projectId: projectId,
        functionName: initialFunctionName,
        implName: initialImplName,
        testCaseName: undefined,
        showTests: showTests,
      })
    },
  )

  context.subscriptions.push(bamlPlaygroundCommand)
  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider({ scheme: 'file', language: 'python' }, glooLens),
  )
  context.subscriptions.push(diagnosticsCollection)

  // vscode.workspace.onDidChangeTextDocument((event) => {
  //   if (vscode.window.activeTextEditor && event.document === vscode.window.activeTextEditor.document) {
  //     scheduleDiagnostics();

  //   }
  // }, null, context.subscriptions);





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

  testExecutor.start()

  if (process.env.VSCODE_DEBUG_MODE === "true") {
    console.log(`vscode env: ${JSON.stringify(process.env, null, 2)}`)
    vscode.commands.executeCommand('baml.openBamlPanel')
  }

  // TODO: Reactivate linter.
  // runDiagnostics();
}

export function deactivate(): void {
  console.log("BAML extension deactivating")
  testExecutor.close()
  diagnosticsCollection.clear()
  diagnosticsCollection.dispose()
  statusBarItem.dispose();
  plugins.forEach((plugin) => {
    if (plugin.deactivate) {
      void plugin.deactivate()
    }
  })
}



class DiagnosticCodeActionProvider implements vscode.CodeActionProvider {
  public provideCodeActions(document: vscode.TextDocument, range: vscode.Range, context: vscode.CodeActionContext, token: vscode.CancellationToken): vscode.ProviderResult<vscode.CodeAction[]> {
    const codeActions: vscode.CodeAction[] = [];

    context.diagnostics.forEach(diagnostic => {
      if (diagnostic.code?.toString().startsWith('[linter]')) {
        const fixString = diagnostic.code.toString().replace('[linter]', '');
        const fixAction = new vscode.CodeAction(`Apply fix: ${fixString}`, vscode.CodeActionKind.QuickFix);
        fixAction.edit = new vscode.WorkspaceEdit();
        fixAction.diagnostics = [diagnostic];
        fixAction.isPreferred = true;


        const edit = new vscode.TextEdit(diagnostic.range, fixString);
        fixAction.edit.set(document.uri, [edit]);

        codeActions.push(fixAction);
      }
    });

    console.log('Code actions:', codeActions);
    return codeActions;
  }
}