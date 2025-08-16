import { encodeValue } from './encode.js';
import { decodeValue, TypeMap } from './decode.js';
import { CFFIValueHolder } from './proto/cffi_pb.js';

// Type definitions for WASM exports
interface WasmExports {
  memory: WebAssembly.Memory;
  init_wasm: () => void;
  create_baml_runtime_wasm: (rootPath: string, srcFiles: string, envVars: string) => number;
  destroy_baml_runtime_wasm: (runtime: number) => void;
  call_function_wasm: (runtime: number, functionName: string, argsProto: Uint8Array, callbackId: number) => void;
  call_function_stream_wasm: (runtime: number, functionName: string, argsProto: Uint8Array, streamId: string) => void;
}

export class BamlWasmRuntime {
  private wasmModule: WasmExports | null = null;
  private runtime: number = 0;
  private typeMap: TypeMap;
  
  private constructor() {
    this.typeMap = new Map();
  }
  
  static async create(
    rootPath: string,
    srcFiles: Record<string, string>,
    envVars: Record<string, string>
  ): Promise<BamlWasmRuntime> {
    const instance = new BamlWasmRuntime();
    
    // Load WASM module
    try {
      // For TypeScript, we'll load the WASM module from a URL
      // In production, this would be served from your web server
      const wasmUrl = '/wasm/baml_cffi_bg.wasm';
      const wasmResponse = await fetch(wasmUrl);
      const wasmBytes = await wasmResponse.arrayBuffer();
      
      const imports = {
        wbg: {
          __wbg_log_1d3ae0273d8f4f8a: (arg0: number, arg1: number) => {
            console.log(getString(arg0, arg1));
          },
          __wbg_error_f851667af71bcfc6: (arg0: number, arg1: number) => {
            console.error(getString(arg0, arg1));
          }
        }
      };
      
      // Initialize WASM instance
      const wasmInstance = await WebAssembly.instantiate(wasmBytes, imports);
      instance.wasmModule = (wasmInstance.instance as any).exports as WasmExports;
      
      // Initialize WASM runtime
      instance.wasmModule.init_wasm();
      
      // Create BAML runtime
      const srcFilesJson = JSON.stringify(srcFiles);
      const envVarsJson = JSON.stringify(envVars);
      
      instance.runtime = instance.wasmModule.create_baml_runtime_wasm(
        rootPath,
        srcFilesJson,
        envVarsJson
      );
      
      if (!instance.runtime) {
        throw new Error('Failed to create BAML runtime');
      }
      
      return instance;
    } catch (error) {
      throw new Error(`Failed to initialize WASM runtime: ${error}`);
    }
  }
  
  async callFunction<T>(
    functionName: string,
    args: Record<string, any>,
    responseType?: new() => T
  ): Promise<T> {
    if (!this.wasmModule || !this.runtime) {
      throw new Error('Runtime not initialized');
    }
    
    // Encode arguments
    const argsHolder = new CFFIValueHolder();
    argsHolder.value = {
      case: 'classValue',
      value: {
        name: 'Arguments',
        fields: Object.fromEntries(
          Object.entries(args).map(([k, v]) => [k, encodeValue(v)])
        )
      }
    };
    
    // Serialize to protobuf bytes (placeholder - actual implementation needs protobuf serialization)
    const protoBytes = new Uint8Array(JSON.stringify(argsHolder).split('').map(c => c.charCodeAt(0)));
    
    // Create a promise that will be resolved by the callback
    const callbackId = Math.floor(Math.random() * 1000000);
    
    return new Promise<T>((resolve, reject) => {
      // Register callback
      (globalThis as any)[`__baml_callback_${callbackId}`] = (data: Uint8Array, isError: boolean) => {
        if (isError) {
          const errorMsg = new TextDecoder().decode(data);
          reject(new Error(errorMsg));
        } else {
          try {
            // Parse result (placeholder - actual implementation needs protobuf deserialization)
            const resultJson = new TextDecoder().decode(data);
            const resultHolder = JSON.parse(resultJson) as CFFIValueHolder;
            
            // Register response type if provided
            if (responseType) {
              this.typeMap.set(responseType.name, responseType);
            }
            
            const result = decodeValue(resultHolder, this.typeMap) as T;
            resolve(result);
          } catch (error) {
            reject(new Error(`Failed to decode result: ${error}`));
          }
        }
        
        // Clean up callback
        delete (globalThis as any)[`__baml_callback_${callbackId}`];
      };
      
      // Call WASM function with callback
      this.wasmModule!.call_function_wasm(
        this.runtime,
        functionName,
        protoBytes,
        callbackId
      );
    });
  }
  
  async* callFunctionStream<T>(
    functionName: string,
    args: Record<string, any>,
    responseType?: new() => T
  ): AsyncGenerator<{ partial: T }, T, undefined> {
    if (!this.wasmModule || !this.runtime) {
      throw new Error('Runtime not initialized');
    }
    
    // Encode arguments
    const argsHolder = new CFFIValueHolder();
    argsHolder.value = {
      case: 'classValue',
      value: {
        name: 'Arguments',
        fields: Object.fromEntries(
          Object.entries(args).map(([k, v]) => [k, encodeValue(v)])
        )
      }
    };
    
    const protoBytes = new Uint8Array(JSON.stringify(argsHolder).split('').map(c => c.charCodeAt(0)));
    
    // Set up streaming with callbacks
    const streamId = Math.random().toString(36).substring(7);
    const chunks: any[] = [];
    let resolver: ((value: any) => void) | null = null;
    let isDone = false;
    
    // Register callback for streaming data
    (globalThis as any).__baml_stream_callbacks = (globalThis as any).__baml_stream_callbacks || {};
    (globalThis as any).__baml_stream_callbacks[streamId] = (data: Uint8Array, done: boolean) => {
      const resultJson = new TextDecoder().decode(data);
      const holder = JSON.parse(resultJson) as CFFIValueHolder;
      const decoded = decodeValue(holder, this.typeMap);
      
      if (done) {
        isDone = true;
      }
      
      if (resolver) {
        resolver({ partial: decoded, done });
        resolver = null;
      } else {
        chunks.push({ partial: decoded, done });
      }
    };
    
    // Start streaming call
    this.wasmModule.call_function_stream_wasm(
      this.runtime,
      functionName,
      protoBytes,
      streamId
    );
    
    // Register response type if provided
    if (responseType) {
      this.typeMap.set(responseType.name, responseType);
    }
    
    // Yield chunks as they arrive
    while (!isDone) {
      if (chunks.length > 0) {
        const chunk = chunks.shift();
        if (chunk.done) {
          return chunk.partial as T;
        }
        yield { partial: chunk.partial as T };
      } else {
        // Wait for next chunk
        await new Promise(resolve => {
          resolver = resolve;
        });
      }
    }
    
    // Clean up callback
    delete (globalThis as any).__baml_stream_callbacks[streamId];
    
    // Return final value
    if (chunks.length > 0) {
      return chunks[chunks.length - 1].partial as T;
    }
    
    throw new Error('Stream ended without final value');
  }
  
  destroy(): void {
    if (this.wasmModule && this.runtime) {
      this.wasmModule.destroy_baml_runtime_wasm(this.runtime);
      this.runtime = 0;
      this.wasmModule = null;
    }
  }
}

// Helper function to read strings from WASM memory
function getString(ptr: number, len: number): string {
  // This would need to be implemented based on actual WASM memory access
  return '';
}