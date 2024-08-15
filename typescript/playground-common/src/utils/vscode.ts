import { decodeBuffer, GetWebviewUriRequest, GetWebviewUriResponse } from '../baml_wasm_web/rpc'
import type { WebviewApi } from 'vscode-webview'

const RPC_TIMEOUT_MS = 5000

interface RpcResponse {
  rpcMethod: string
  rpcId: number
  data: unknown
}

const isRpcResponse = (eventData: unknown): eventData is RpcResponse => {
  return (
    typeof eventData === 'object' &&
    eventData !== null &&
    'rpcId' in eventData &&
    typeof (eventData as RpcResponse).rpcMethod === 'string' &&
    typeof (eventData as RpcResponse).rpcId === 'number'
  )
}

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
  private readonly vsCodeApi: WebviewApi<unknown> | undefined

  private rpcTable: Map<number, { resolve: (resp: unknown) => void }>
  private rpcId: number

  constructor() {
    // Check if the acquireVsCodeApi function exists in the current development
    // context (i.e. VS Code development window or web browser)
    if (typeof acquireVsCodeApi === 'function') {
      this.vsCodeApi = acquireVsCodeApi()
      window.addEventListener('message', this.listenForRpcResponses.bind(this))
    }

    this.rpcTable = new Map()
    this.rpcId = 0
  }

  public async readFile(path: string): Promise<Uint8Array> {
    const uri = await this.readLocalFile('', path)
    console.log('read file', uri)

    if (uri.readError) {
      throw new Error(`Failed to read file: ${path}\n${uri.readError}`)
    }
    if (uri.contents) {
      const contents = uri.contents
      // throw new Error(`not implemented: ${Array.isArray(contents)}: \n ${JSON.stringify(contents)}`)
      return decodeBuffer(contents)
    }

    throw new Error(`Unknown error: '${path}'`)
  }

  async readLocalFile(bamlSrc: string, path: string): Promise<GetWebviewUriResponse> {
    const resp = await this.rpc<GetWebviewUriRequest, GetWebviewUriResponse>({
      vscodeCommand: 'GET_WEBVIEW_URI',
      bamlSrc,
      path,
      contents: true,
    })

    return resp
  }

  public async asWebviewUri(bamlSrc: string, path: string): Promise<string> {
    const resp = await this.rpc<GetWebviewUriRequest, GetWebviewUriResponse>({
      vscodeCommand: 'GET_WEBVIEW_URI',
      bamlSrc,
      path,
    })

    return resp.uri
  }

  public rpc<TRequest, TResponse>(data: TRequest): Promise<TResponse> {
    return new Promise((resolve, reject) => {
      const rpcId = this.rpcId++
      this.rpcTable.set(rpcId, { resolve: resolve as (resp: unknown) => void })

      this.postMessage({
        rpcMethod: (data as unknown as { vscodeCommand: string }).vscodeCommand,
        rpcId,
        data,
      })

      // Timeout to prevent hanging requests
      setTimeout(() => {
        if (this.rpcTable.has(rpcId)) {
          this.rpcTable.delete(rpcId)
          reject(new Error(`VSCode RPC request timed out after ${RPC_TIMEOUT_MS}ms: ${(data as any).vscodeCommand}`))
        }
      }, RPC_TIMEOUT_MS)
    })
  }

  private listenForRpcResponses(event: any) {
    // console.log('unfiltered messages to webview', event.data)

    if (isRpcResponse(event.data)) {
      // console.log('filtered to RPC responses', event.data)
      const rpcData = event.data as RpcResponse
      const entry = this.rpcTable.get(rpcData.rpcId)
      if (entry) {
        entry.resolve(rpcData.data)
        this.rpcTable.delete(rpcData.rpcId)
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
      console.log('posting message' + JSON.stringify(message))
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

// Exports class singleton to prevent multiple invocations of acquireVsCodeApi.
export const vscode = new VSCodeAPIWrapper()
