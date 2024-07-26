// Commands that vscode sends to the webview
export type VscodeToWebviewCommand =
  | {
      command: 'modify_file'
      content: {
        root_path: string
        name: string
        content: string | undefined
      }
    }
  | {
      command: 'add_project'
      content: {
        root_path: string
        files: Record<string, string>
      }
    }
  | {
      command: 'remove_project'
      content: {
        root_path: string
      }
    }
  | {
      command: 'select_function'
      content: {
        root_path: string
        function_name: string
      }
    }
  | {
      command: 'update_cursor'
      content: {
        cursor: {
          fileName: string
          fileText: string
          line: number
          column: number
        }
      }
    }
  | {
      command: 'port_number'
      content: {
        port: number
      }
    }
  | {
      command: 'baml_cli_version'
      content: string
    }
  | {
      command: 'run_test'
      content: {
        test_name: string
      }
    }

// Commands that the webview sends to vscode
type EnsureVSCodeCommand<T> = T extends { vscodeCommand: string } ? T : never

type ExtractRequestType<T> = T extends [infer Req, any] ? EnsureVSCodeCommand<Req> : never

type RequestUnion<T extends [any, any][]> = ExtractRequestType<T[number]>

export interface EchoRequest {
  vscodeCommand: 'ECHO'
  message: string
}

export interface EchoResponse {
  message: string
}

export interface GetBamlSrcRequest {
  vscodeCommand: 'GET_BAML_SRC'
  path: string
}

export interface GetBamlSrcResponse {
  contents: Uint8Array
}

export interface GetWebviewUriRequest {
  vscodeCommand: 'GET_WEBVIEW_URI'
  bamlSrc: string
  path: string
}

export interface GetWebviewUriResponse {
  uri: string
}

type ApiPairs = [
  // Echo is included here as an example of what a request/response pair looks like
  [EchoRequest, EchoResponse],
  [GetBamlSrcRequest, GetBamlSrcResponse],
  [GetWebviewUriRequest, GetWebviewUriResponse],
]

export type WebviewToVscodeRpc = RequestUnion<ApiPairs>
