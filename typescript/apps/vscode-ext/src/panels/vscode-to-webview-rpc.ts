import type { BamlConfigAtom } from "../plugins/language-server-client/bamlConfig";

// Commands that vscode sends to the webview
export type VscodeToWebviewCommand =
	| {
		source: 'ide_message';
		payload: | {
			command: 'proxy_port';
			content: {
				port: number;
			};
		}
		| {
			command: 'update_cursor';
			content: {
				fileName: string;
				fileText: string;
				line: number;
				column: number;
			};
		}
		| {
			command: 'port_number';
			content: {
				port: number;
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
			method: "runtime_updated";
			params: {
				root_path: string;
				files: Record<string, string>;
			};
		}
		| {
			method: "workspace/executeCommand";
			params: {
				command: "baml.openBamlPanel";
				arguments: [
					{
						projectId: string;
						functionName: string;
					}
				]
			}
		}
		| {
			method: "workspace/executeCommand";
			params: {
				command: "baml.runBamlTest";
				arguments: [
					{
						projectId: string;
						functionName: string;
						testCaseName: string;
					}
				]
			}
		} | {
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