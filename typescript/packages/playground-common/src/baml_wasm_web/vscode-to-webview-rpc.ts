import { BamlConfigAtom } from "./bamlConfig";

// Commands that vscode sends to the webview
export type VscodeToWebviewCommand =
    | {
        source: 'ide_message';
        payload: | {
            command: 'update_cursor';
            content: {
                fileName: string;
                line: number;
                column: number;
            };
        }
        | {
            command: 'baml_cli_version';
            content: string;
        }
        | {
            command: 'baml_settings_updated';
            content: BamlConfigAtom;
        }
    }
    | {
        source: 'lsp_message';
        payload: | {
            // This is the core event that drives synchronization between edits/changes to files
            // in the IDE and the runtime in the webview - yes, we really do just constantly
            // re-compile users' entire codebases on every single keystroke
            method: "runtime_updated";
            params: {
                root_path: string;
                files: Record<string, string>;
            };
        }
        | {
            // Used by jetbrains/zed, in theory
            // At the time of this writing, settings wiring is not done
            method: 'baml_settings_updated';
            params: Partial<BamlConfigAtom>;
        }
        | {
            // In VSCode we simulate this forwarding; in Jetbrains/Zed this is
            // forwarded directly.
            method: "workspace/executeCommand";
            params: {
                command: "baml.openBamlPanel";
                arguments: [
                    {
                        functionName: string;
                    }
                ]
            }
        }
        | {
            // In VSCode we simulate this forwarding; in Jetbrains/Zed this is
            // forwarded directly.
            method: "workspace/executeCommand";
            params: {
                command: "baml.runBamlTest";
                arguments: [
                    {
                        functionName: string;
                        testCaseName: string;
                    }
                ]
            }
        } | {
            // Used by Zed instead of update_cursor, because it doesn't have
            // support for custom cursor update listeners.
            method: "textDocument/codeAction";
            params: {
                textDocument: {
                    uri: string;
                };
                range: {
                    start: {
                        line: number;
                        character: number;
                    };
                    end: {
                        line: number;
                        character: number;
                    };
                };
            }
        }
    }