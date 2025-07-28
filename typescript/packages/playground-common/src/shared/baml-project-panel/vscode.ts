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

// Define WebviewApi type for VSCode webview context
interface WebviewApi<T> {
  postMessage(message: any): void;
  getState(): T | undefined;
  setState<U extends T>(newState: U): U;
}

// Declare the global acquireVsCodeApi function provided by VSCode webviews
declare global {
  function acquireVsCodeApi(): any
}

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
  private readonly vsCodeApi: any | undefined

  private rpcTable: Map<number, { resolve: (resp: unknown) => void }>;
  private rpcId: number;
  private wsRpc: WebSocket | null = null;
  private wsConnecting: Promise<WebSocket> | null = null;
  public isConnected: boolean = false;
  public onOpen?: () => void;

  constructor() {
    // Check if the acquireVsCodeApi function exists in the current development
    // context (i.e. VS Code development window or web browser)
    if (typeof acquireVsCodeApi === 'function' && typeof window !== 'undefined') {
      this.vsCodeApi = acquireVsCodeApi()
      window.addEventListener('message', this.listenForRpcResponses.bind(this))
    }

    this.rpcTable = new Map();
    this.rpcId = 0;
  }

  private async ensureWebSocketRpcConnection(): Promise<WebSocket> {
    if (this.wsRpc && this.wsRpc.readyState === WebSocket.OPEN) {
      return this.wsRpc;
    }

    // If already connecting, wait for that connection
    if (this.wsConnecting) {
      return this.wsConnecting;
    }

    this.wsConnecting = new Promise((resolve, reject) => {
      const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
      const ws = new WebSocket(`${scheme}://${window.location.host}/rpc`);

      ws.onopen = () => {
        console.log('RPC WebSocket Opened');
        this.wsRpc = ws;
        this.isConnected = true;
        this.wsConnecting = null;
        if (this.onOpen) this.onOpen();
        resolve(ws);
      };

      ws.onerror = (error) => {
        console.error('RPC WebSocket error', error);
        this.isConnected = false;
        this.wsConnecting = null;
        reject(new Error('Failed to connect to language server RPC WebSocket'));
      };

      ws.onclose = () => {
        console.log('RPC WebSocket Closed');
        this.isConnected = false;
        this.wsRpc = null;
        this.wsConnecting = null;
      };

      ws.onmessage = (event) => {
        try {
          const response = JSON.parse(event.data);
          if (response.rpcId && this.rpcTable.has(response.rpcId)) {
            const entry = this.rpcTable.get(response.rpcId);
            if (entry) {
              entry.resolve(response.data);
              this.rpcTable.delete(response.rpcId);
            }
          }
        } catch (e) {
          console.error('Error parsing WebSocket RPC response:', e);
        }
      };

      // Timeout for connection
      setTimeout(() => {
        if (ws.readyState !== WebSocket.OPEN) {
          ws.close();
          this.wsConnecting = null;
          reject(new Error('WebSocket RPC connection timeout'));
        }
      }, 5000);
    });

    return this.wsConnecting;
  }

  public isVscode() {
    return this.vsCodeApi !== undefined
  }

  public async readFile(path: string): Promise<Uint8Array> {
    const uri = await this.readLocalFile('', path);

    // Debug logging to understand the response structure
    console.log('readFile response for path:', path, 'response:', uri);

    if (uri.readError) {
      throw new Error(`Failed to read file: ${path}\n${uri.readError}`);
    }
    if (uri.contents) {
      const contents = uri.contents;
      return decodeBuffer(contents);
    }

    // Handle malformed response - if we have a uri but no contents or readError,
    // it likely means the file doesn't exist or couldn't be read
    if (uri.uri) {
      throw new Error(`File not found or unable to read: '${path}'`);
    }

    // More detailed error message with response info for completely malformed responses
    throw new Error(`Malformed response for file: '${path}'. Response received: ${JSON.stringify(uri)}`);
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
    return new Promise(async (resolve, reject) => {
      const rpcId = this.rpcId++;
      this.rpcTable.set(rpcId, { resolve: resolve as (resp: unknown) => void });

      const message = {
        rpcMethod: (data as unknown as { vscodeCommand: string }).vscodeCommand,
        rpcId,
        data,
      };

      try {
        if (this.isVscode()) {
          // Use VSCode webview messaging
          this.postMessage(message);
        } else {
          // Use WebSocket RPC for other editors (like Zed)
          const ws = await this.ensureWebSocketRpcConnection();
          ws.send(JSON.stringify(message));
        }
      } catch (error) {
        this.rpcTable.delete(rpcId);
        reject(error);
        return;
      }

      // Timeout to prevent hanging requests
      setTimeout(() => {
        if (this.rpcTable.has(rpcId)) {
          this.rpcTable.delete(rpcId);
          reject(
            new Error(
              `${this.isVscode() ? 'VSCode' : 'WebSocket'} RPC request timed out after ${RPC_TIMEOUT_MS}ms: ${(data as any).vscodeCommand}`,
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

  /**
   * Post a message (i.e. send arbitrary data) to the owner of the webview.
   *
   * @remarks When running webview code inside a web browser, postMessage will instead
   * log the given message to the console.
   *
   * @param message Abitrary data (must be JSON serializable) to send to the extension context.
   */
  public postMessage(message: unknown) {
    if (this.vsCodeApi) {
      this.vsCodeApi.postMessage(message)
    } else {
      window.postMessage(message)
    }
  }

  /**
   * Get the persistent state stored for this webview.
   *
   * @remarks When running webview source code inside a web browser, getState will retrieve state
   * from local storage (https://developer.mozilla.org/en-US/docs/Web/API/Window/localStorage).
   *
   * @return The current state or `undefined` if no state has been set.
   */
  public getState(): unknown | undefined {
    if (this.vsCodeApi) {
      return this.vsCodeApi.getState()
    } else {
      const state = localStorage.getItem('vscodeState')
      return state ? JSON.parse(state) : undefined
    }
  }

  /**
   * Set the persistent state stored for this webview.
   *
   * @remarks When running webview source code inside a web browser, setState will set the given
   * state using local storage (https://developer.mozilla.org/en-US/docs/Web/API/Window/localStorage).
   *
   * @param newState New persisted state. This must be a JSON serializable object. Can be retrieved
   * using {@link getState}.
   *
   * @return The new state.
   */
  public setState<T extends unknown | undefined>(newState: T): T {
    if (this.vsCodeApi) {
      return this.vsCodeApi.setState(newState)
    } else {
      localStorage.setItem('vscodeState', JSON.stringify(newState))
      return newState
    }
  }
}

// Create a singleton instance of the wrapper class and export it for use across the webview
export const vscode = new VSCodeAPIWrapper()