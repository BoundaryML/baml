import type { AwsCredentialIdentity } from '@smithy/types';

// Commands that vscode sends to the webview
export type VscodeToWebviewCommand =
  | {
      command: 'modify_file';
      content: {
        root_path: string;
        name: string;
        content: string | undefined;
      };
    }
  | {
      command: 'add_project';
      content: {
        root_path: string;
        files: Record<string, string>;
      };
    }
  | {
      command: 'remove_project';
      content: {
        root_path: string;
      };
    }
  | {
      command: 'select_function';
      content: {
        root_path: string;
        function_name: string;
      };
    }
  | {
      command: 'update_cursor';
      content: {
        cursor: {
          fileName: string;
          fileText: string;
          line: number;
          column: number;
        };
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
      command: 'run_test';
      content: {
        test_name: string;
      };
    };

// Commands that the webview sends to vscode
type EnsureVSCodeCommand<T> = T extends { vscodeCommand: string } ? T : never;

type ExtractRequestType<T> = T extends [infer Req, any]
  ? EnsureVSCodeCommand<Req>
  : never;

type RequestUnion<T extends [any, any][]> = ExtractRequestType<T[number]>;

export interface EchoRequest {
  vscodeCommand: 'ECHO';
  message: string;
}

export interface EchoResponse {
  message: string;
}

export interface SetProxySettingsRequest {
  vscodeCommand: 'SET_PROXY_SETTINGS';
  proxyEnabled: boolean;
}

export interface GetBamlSrcRequest {
  vscodeCommand: 'GET_BAML_SRC';
  path: string;
}

export interface GetBamlSrcResponse {
  contents: Uint8Array;
}

export interface GetWebviewUriRequest {
  vscodeCommand: 'GET_WEBVIEW_URI';
  bamlSrc: string;
  path: string;
  contents?: true;
}

export interface GetWebviewUriResponse {
  uri: string;
  contents?: string;
  readError?: string;
}

export interface GetVSCodeSettingsRequest {
  vscodeCommand: 'GET_VSCODE_SETTINGS';
}

export interface GetVSCodeSettingsResponse {
  enablePlaygroundProxy: boolean;
}

export interface GetPlaygroundPortRequest {
  vscodeCommand: 'GET_PLAYGROUND_PORT';
}

export interface GetPlaygroundPortResponse {
  port: number;
}

export interface LoadAwsCredsRequest {
  vscodeCommand: 'LOAD_AWS_CREDS';
  profile: string | null;
}

export type LoadAwsCredsResponse =
  | {
      ok: AwsCredentialIdentity;
    }
  | {
      error: {
        name: string;
        message: string;
      };
    };

export interface LoadGcpCredsRequest {
  vscodeCommand: 'LOAD_GCP_CREDS';
}

export type LoadGcpCredsResponse =
  | {
      ok: {
        accessToken: string;
        projectId: string;
      };
    }
  | {
      error: {
        name: string;
        message: string;
      };
    };

export interface InitializedRequest {
  vscodeCommand: 'INITIALIZED';
}

export interface InitializedResponse {
  ack: true;
}

export interface OpenPlaygroundRequest {
  vscodeCommand: 'OPEN_PLAYGROUND';
}

export interface OpenPlaygroundResponse {
  success: boolean;
  url?: string;
  error?: string;
}

type ApiPairs = [
  // Echo is included here as an example of what a request/response pair looks like
  [EchoRequest, EchoResponse],
  [SetProxySettingsRequest, void],
  [GetBamlSrcRequest, GetBamlSrcResponse],
  [GetWebviewUriRequest, GetWebviewUriResponse],
  [GetVSCodeSettingsRequest, GetVSCodeSettingsResponse],
  [GetPlaygroundPortRequest, GetPlaygroundPortResponse],
  [LoadAwsCredsRequest, LoadAwsCredsResponse],
  [LoadGcpCredsRequest, LoadGcpCredsResponse],
  [InitializedRequest, InitializedResponse],
  [OpenPlaygroundRequest, OpenPlaygroundResponse],
];

// Serialization for binary data (like images)
function serializeBinaryData(uint8Array: Uint8Array): string {
  return uint8Array.reduce(
    (data, byte) => data + String.fromCharCode(byte),
    '',
  );
}

// Deserialization for binary data
function deserializeBinaryData(serialized: string): Uint8Array {
  return new Uint8Array(serialized.split('').map((char) => char.charCodeAt(0)));
}

export function encodeBuffer(arr: Uint8Array): string {
  return serializeBinaryData(arr);
}

export function decodeBuffer(str: string): Uint8Array {
  return deserializeBinaryData(str);
}

export type WebviewToVscodeRpc = RequestUnion<ApiPairs>;
