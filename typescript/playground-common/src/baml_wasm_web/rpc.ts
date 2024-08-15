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
  contents?: true
}

export interface GetWebviewUriResponse {
  uri: string
  contents?: string
  readError?: string
}

type ApiPairs = [
  // Echo is included here as an example of what a request/response pair looks like
  [EchoRequest, EchoResponse],
  [GetBamlSrcRequest, GetBamlSrcResponse],
  [GetWebviewUriRequest, GetWebviewUriResponse],
]

// Serialization for binary data (like images)
function serializeBinaryData(uint8Array: Uint8Array): string {
  return uint8Array.reduce((data, byte) => data + String.fromCharCode(byte), '')
}

// Deserialization for binary data
function deserializeBinaryData(serialized: string): Uint8Array {
  return new Uint8Array(serialized.split('').map((char) => char.charCodeAt(0)))
}

// Base64 encoding
function base64Encode(str: string): string {
  const base64chars: string = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'
  let result: string = ''
  let i: number
  for (i = 0; i < str.length; i += 3) {
    const chunk: number = (str.charCodeAt(i) << 16) | (str.charCodeAt(i + 1) << 8) | str.charCodeAt(i + 2)
    result +=
      base64chars.charAt((chunk & 16515072) >> 18) +
      base64chars.charAt((chunk & 258048) >> 12) +
      base64chars.charAt((chunk & 4032) >> 6) +
      base64chars.charAt(chunk & 63)
  }
  if (str.length % 3 === 1) result = result.slice(0, -2) + '=='
  if (str.length % 3 === 2) result = result.slice(0, -1) + '='
  return result
}

// Base64 decoding
function base64Decode(str: string): string {
  const base64chars: string = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'
  while (str[str.length - 1] === '=') {
    str = str.slice(0, -1)
  }
  let result: string = ''
  for (let i = 0; i < str.length; i += 4) {
    const chunk: number =
      (base64chars.indexOf(str[i]) << 18) |
      (base64chars.indexOf(str[i + 1]) << 12) |
      (base64chars.indexOf(str[i + 2]) << 6) |
      base64chars.indexOf(str[i + 3])
    result += String.fromCharCode((chunk & 16711680) >> 16, (chunk & 65280) >> 8, chunk & 255)
  }
  return result.slice(0, result.length - (str.length % 4 ? 4 - (str.length % 4) : 0))
}

export function encodeBuffer(arr: Uint8Array): string {
  return serializeBinaryData(arr)
}

export function decodeBuffer(str: string): Uint8Array {
  return deserializeBinaryData(str)
}

export type WebviewToVscodeRpc = RequestUnion<ApiPairs>
