import * as vscode from "vscode";
import { KeywordCompletionProvider } from "./KeywordCompletionProvider";
import { GlooCodeLensProvider } from "./GlooCodeLensProvider";

const { exec } = require("child_process");

function runGlooTest(uri: vscode.Uri): void {
  const terminal = vscode.window.createTerminal(`Gloo Test: ${uri.fsPath}`);
  terminal.sendText(`cd ${uri.fsPath}`);
  terminal.sendText("gloo_cli test");
  terminal.show();
}

export function activate(context: vscode.ExtensionContext) {
  const selector: vscode.DocumentSelector = {
    pattern: "**/*.gloo",
    scheme: "file",
  };
  const config = vscode.workspace.getConfiguration("gloo");
  const glooPath = config.get<string>("path", "gloo");

  let disposable = vscode.workspace.onDidSaveTextDocument((document) => {
    if (document.fileName.endsWith(".gloo")) {
      runBuildScript(document, glooPath);
    }
  });

  context.subscriptions.push(disposable);
}

const diagnosticsCollection =
  vscode.languages.createDiagnosticCollection("gloo");

const outputChannel = vscode.window.createOutputChannel("Gloo");

function runBuildScript(document: vscode.TextDocument, glooPath: string): void {
  let buildCommand = `${glooPath} build`;

  let workspaceFolder = vscode.workspace.getWorkspaceFolder(document.uri);

  if (!workspaceFolder) {
    return;
  }
  let options = {
    cwd: workspaceFolder.uri.fsPath,
  };

  exec(
    buildCommand,
    options,
    (error: Error | null, stdout: string, stderr: string) => {
      if (stdout) {
        outputChannel.appendLine(stdout);
      }
      if (error) {
        vscode.window
          .showErrorMessage(
            `Error running the build script: ${error}`,
            "Show Details"
          )
          .then((selection) => {
            if (selection === "Show Details") {
              outputChannel.appendLine(
                `Error running the build script: ${error}`
              );
              outputChannel.show(true);
            }
          });
        return;
      }
      if (stderr) {
        // Parse the error message to extract line number
        const lineMatch = stderr.match(/Line (\d+),/);
        if (lineMatch) {
          const lineNumber = parseInt(lineMatch[1], 10) - 1;

          const range = new vscode.Range(
            lineNumber,
            0,
            lineNumber,
            Number.MAX_VALUE
          );
          const diagnostic = new vscode.Diagnostic(
            range,
            stderr,
            vscode.DiagnosticSeverity.Error
          );

          diagnosticsCollection.set(document.uri, [diagnostic]);
        } else {
          vscode.window
            .showErrorMessage(`Build error: ${stderr}`, "Show Details")
            .then((selection) => {
              if (selection === "Show Details") {
                const outputChannel = vscode.window.createOutputChannel("Gloo");
                outputChannel.appendLine(
                  `Error running the build script: ${stderr}`
                );
                outputChannel.show(true);
              }
            });
        }
        return;
      }

      // Clear any diagnostics if the build was successful
      diagnosticsCollection.clear();

      const infoMessage = vscode.window.showInformationMessage(
        "Gloo build was successful"
      );

      setTimeout(() => {
        infoMessage.then;
      }, 5000);
    }
  );
}

export function deactivate() {}
