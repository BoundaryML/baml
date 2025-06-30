import {
  type GetPlaygroundPortRequest,
  type GetPlaygroundPortResponse,
  type GetWebviewUriRequest,
  type GetWebviewUriResponse,
  type InitializedRequest,
  type InitializedResponse,
  type LoadAwsCredsRequest,
  type LoadAwsCredsResponse,
  type LoadGcpCredsRequest,
  type LoadGcpCredsResponse,
  type SetProxySettingsRequest,
  decodeBuffer,
} from './vscode-rpc';

const RPC_TIMEOUT_MS = 5000;

interface RpcResponse {
  rpcMethod: string;
  rpcId: number;
  data: unknown;
}

const isRpcResponse = (eventData: unknown): eventData is RpcResponse => {
  return (
    typeof eventData === 'object' &&
    eventData !== null &&
    'rpcId' in eventData &&
    typeof (eventData as RpcResponse).rpcMethod === 'string' &&
    typeof (eventData as RpcResponse).rpcId === 'number'
  );
};

/**
 * A utility wrapper around the acquireVsCodeApi() function, which enables
 * message passing and state management between the webview and extension
 * contexts.
 *
 * This utility also enables webview code to be run in a web browser-based
 * dev server by using native web browser features that mock the functionality
 * enabled by acquireVsCodeApi.
 */
class VSCodeAPIWrapper {
  private ws: WebSocket | undefined;
  private wsReady: Promise<void> | undefined;
  private wsReadyResolve: (() => void) | undefined;
  private rpcTable: Map<number, { resolve: (resp: unknown) => void }>;
  private rpcId: number;
  public isConnected = false;
  public onOpen?: () => void;

  constructor() {
    if (typeof window !== 'undefined') {
      // Use robust WebSocket setup like EventListener
      const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
      const host = window.location.host;
      const url = `${scheme}://${host}/rpc`;
      // biome-ignore lint/suspicious/noAssignInExpressions: <explanation>
      this.wsReady = new Promise((resolve) => (this.wsReadyResolve = resolve));
      this.ws = new WebSocket(url);
      this.ws.onopen = () => {
        console.log('RPC WebSocket Opened');
        this.isConnected = true;
        this.wsReadyResolve?.();
        if (this.onOpen) this.onOpen();
      };
      this.ws.onclose = () => {
        console.log('RPC WebSocket Closed');
        this.isConnected = false;
      };
      this.ws.onerror = (e) => {
        console.error('RPC WebSocket error', e);
        this.isConnected = false;
      };
      this.ws.onmessage = (event) => {
        console.log('RPC WebSocket message received:', event.data);
        let data = event.data;
        try {
          data = JSON.parse(event.data);
        } catch {}
        this.listenForRpcResponses({ data });
      };
    }
    this.rpcTable = new Map();
    this.rpcId = 0;
  }

  public async readFile(path: string): Promise<Uint8Array> {
    const uri = await this.readLocalFile('', path);

    if (uri.readError) {
      throw new Error(`Failed to read file: ${path}\n${uri.readError}`);
    }
    if (uri.contents) {
      const contents = uri.contents;
      return decodeBuffer(contents);
    }

    throw new Error(`Unknown error: '${path}'`);
  }

  async readLocalFile(
    bamlSrc: string,
    path: string,
  ): Promise<GetWebviewUriResponse> {
    const resp = await this.rpc<GetWebviewUriRequest, GetWebviewUriResponse>({
      vscodeCommand: 'GET_WEBVIEW_URI',
      bamlSrc,
      path,
      contents: true,
    });

    return resp;
  }

  public async asWebviewUri(bamlSrc: string, path: string): Promise<string> {
    const resp = await this.rpc<GetWebviewUriRequest, GetWebviewUriResponse>({
      vscodeCommand: 'GET_WEBVIEW_URI',
      bamlSrc,
      path,
    });

    return resp.uri;
  }

  public async getPlaygroundPort() {
    const resp = await this.rpc<
      GetPlaygroundPortRequest,
      GetPlaygroundPortResponse
    >({
      vscodeCommand: 'GET_PLAYGROUND_PORT',
    });
    return resp.port;
  }

  public async setProxySettings(proxyEnabled: boolean) {
    await this.rpc<SetProxySettingsRequest, void>({
      vscodeCommand: 'SET_PROXY_SETTINGS',
      proxyEnabled,
    });
  }

  public loadAwsCreds = async (profile: string | null) => {
    const resp = await this.rpc<LoadAwsCredsRequest, LoadAwsCredsResponse>({
      vscodeCommand: 'LOAD_AWS_CREDS',
      profile,
    });
    return resp;
  };

  public loadGcpCreds = async () => {
    const resp = await this.rpc<LoadGcpCredsRequest, LoadGcpCredsResponse>({
      vscodeCommand: 'LOAD_GCP_CREDS',
    });
    return resp;
  };

  public async markInitialized() {
    try {
      await this.rpc<InitializedRequest, InitializedResponse>({
        vscodeCommand: 'INITIALIZED',
      });
    } catch (e) {
      console.error('Error marking initialized', e);
    }
  }

  public rpc<TRequest, TResponse>(data: TRequest): Promise<TResponse> {
    return new Promise((resolve, reject) => {
      const rpcId = this.rpcId++;
      this.rpcTable.set(rpcId, { resolve: resolve as (resp: unknown) => void });

      const message = {
        rpcMethod: (data as unknown as { vscodeCommand: string }).vscodeCommand,
        rpcId,
        data,
      };
      this.postMessage(message);

      // Timeout to prevent hanging requests
      setTimeout(() => {
        if (this.rpcTable.has(rpcId)) {
          this.rpcTable.delete(rpcId);
          reject(
            new Error(
              `VSCode RPC request timed out after ${RPC_TIMEOUT_MS}ms: ${(data as any).vscodeCommand}`,
            ),
          );
        }
      }, RPC_TIMEOUT_MS);
    });
  }

  private listenForRpcResponses(event: any) {
    if (isRpcResponse(event.data)) {
      const rpcData = event.data as RpcResponse;
      const entry = this.rpcTable.get(rpcData.rpcId);
      if (entry) {
        entry.resolve(rpcData.data);
        this.rpcTable.delete(rpcData.rpcId);
      }
    }
  }

  public postMessage(message: unknown) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(message));
    } else if (this.ws && this.wsReady) {
      // biome-ignore lint/suspicious/noAssignInExpressions: dont' worry!
      // biome-ignore lint/style/noNonNullAssertion: dont' worry!
      this.wsReady.then(() => this.ws!.send(JSON.stringify(message)));
    }
  }

  public getState(): unknown | undefined {
    const state = localStorage.getItem('vscodeState');
    return state ? JSON.parse(state) : undefined;
  }

  public setState<T extends unknown | undefined>(newState: T): T {
    localStorage.setItem('vscodeState', JSON.stringify(newState));
    return newState;
  }
}

// Exports class singleton to prevent multiple invocations of acquireVsCodeApi.
export const vscode = new VSCodeAPIWrapper();
