/* auto-generated by NAPI-RS */
/* eslint-disable */
export class BamlImage {
  static fromUrl(url: string): BamlImage
  static fromBase64(mediaType: string, base64: string): BamlImage
  isUrl(): boolean
  get url(): string
  get base64(): Array<string>
  toJson(): any
}

export class BamlRuntime {
  static fromDirectory(directory: string, envVars: Record<string, string>): BamlRuntime
  static fromFiles(rootPath: string, files: Record<string, string>, envVars: Record<string, string>): BamlRuntime
  createContextManager(): RuntimeContextManager
  callFunction(functionName: string, args: any, ctx: RuntimeContextManager): Promise<FunctionResult>
  streamFunction(functionName: string, args: any, cb: (err: any, param: FunctionResult) => void, ctx: RuntimeContextManager): FunctionResultStream
  flush(): void
}

export class BamlSpan {
  static new(runtime: BamlRuntime, functionName: string, args: any, ctx: RuntimeContextManager): BamlSpan
  finish(result: any, ctx: RuntimeContextManager): Promise<any>
}

export class FunctionResult {
  parsed(): any
}

export class FunctionResultStream {
  onEvent(func: (err: any, param: FunctionResult) => void): void
  done(rt: BamlRuntime, rctx: RuntimeContextManager): Promise<FunctionResult>
}

export class RuntimeContextManager {
  upsertTags(tags: any): void
  deepClone(): RuntimeContextManager
}

export function invoke_runtime_cli(params: Array<string>): void

