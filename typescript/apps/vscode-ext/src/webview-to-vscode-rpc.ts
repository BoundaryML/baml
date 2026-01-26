import type { AwsCredentialIdentity } from '@smithy/types';

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

export interface UpdateSettingsRequest {
  vscodeCommand: 'UPDATE_SETTINGS';
  settings: Record<string, any>;
}


export interface EchoResponse {
  message: string;
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
  featureFlags: string[];
}

export interface GetPlaygroundPortRequest {
  vscodeCommand: 'GET_PLAYGROUND_PORT';
}

export interface GetPlaygroundPortResponse {
  port: number;
}

export interface LoadEnvRequest {
  vscodeCommand: 'LOAD_ENV';
}

export interface LoadEnvResponse {
  envVars: Record<string, string>;
  error?: string;
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
      projectId: string | null;
    };
  }
  | {
    error: {
      name: string;
      message: string;
    };
  };

export interface JumpToFileRequest {
  vscodeCommand: 'JUMP_TO_FILE';
  span: {
    file_path: string;
    start_line: number;
    start_column: number;
  };
}

export interface JumpToFileResponse {
  ok: true;
}

export interface InitializedRequest {
  vscodeCommand: 'INITIALIZED';
}

export interface InitializedResponse {
  ack: true;
}

export interface SetFlashingRegionsRequest {
  vscodeCommand: 'SET_FLASHING_REGIONS';
  spans: {
    file_path: string;
    start_line: number;
    start: number;
    end_line: number;
    end: number;
  }[];
}

export interface SetFlashingRegionsResponse {
  ack: true;
}

type ApiPairs = [
  // Echo is included here as an example of what a request/response pair looks like
  [EchoRequest, EchoResponse],
  [UpdateSettingsRequest, void],
  [GetBamlSrcRequest, GetBamlSrcResponse],
  [GetWebviewUriRequest, GetWebviewUriResponse],
  [GetVSCodeSettingsRequest, GetVSCodeSettingsResponse],
  [GetPlaygroundPortRequest, GetPlaygroundPortResponse],
  [LoadEnvRequest, LoadEnvResponse],
  [LoadAwsCredsRequest, LoadAwsCredsResponse],
  [LoadGcpCredsRequest, LoadGcpCredsResponse],
  [InitializedRequest, InitializedResponse],
  [JumpToFileRequest, JumpToFileResponse],
  [SetFlashingRegionsRequest, SetFlashingRegionsResponse],
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
