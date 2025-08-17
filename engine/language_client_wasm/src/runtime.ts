import { encodeValue } from './encode.js';
import { decodeValue, TypeMap } from './decode.js';
import { CFFIValueHolder } from './proto/cffi_pb.js';
import { serializeArgs, deserializeResult } from './proto_utils.js';

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
      // Check if WASM module is already loaded globally (for testing)
      if ((globalThis as any).wasmModule) {
        instance.wasmModule = (globalThis as any).wasmModule;
      } else {
        // Load the JS bindings module dynamically to avoid TypeScript errors
        const wasmModule = await import(/* @vite-ignore */ '../wasm/baml_cffi_bg.js') as any;
        
        // Fetch the WASM binary
        const wasmResponse = await fetch('/wasm/baml_cffi_bg.wasm');
        const wasmBinary = await wasmResponse.arrayBuffer();
        
        // Instantiate the WASM module with proper imports
        const wasmInstance = await WebAssembly.instantiate(wasmBinary, {
          './baml_cffi_bg.js': wasmModule
        });
        
        // Set the WASM instance in the bindings
        wasmModule.__wbg_set_wasm(wasmInstance.instance.exports);
        
        // Initialize WASM if init function exists
        if (wasmModule.init_wasm) {
          wasmModule.init_wasm();
        }
        
        instance.wasmModule = wasmModule as WasmExports;
      }
      
      // Create BAML runtime
      const srcFilesJson = JSON.stringify(srcFiles);
      const envVarsJson = JSON.stringify(envVars);
      
      if (!instance.wasmModule) {
        throw new Error('WASM module not loaded');
      }
      
      instance.runtime = instance.wasmModule.create_baml_runtime_wasm(
        rootPath,
        srcFilesJson,
        envVarsJson
      );
      
      if (!instance.runtime || instance.runtime === 0) {
        throw new Error('Failed to create BAML runtime - returned null/0');
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
    
    // Serialize arguments to protobuf bytes
    const protoBytes = serializeArgs(args);
    
    // Create a promise that will be resolved by the callback
    const callbackId = Math.floor(Math.random() * 1000000);
    
    return new Promise<T>((resolve, reject) => {
      // Register callback object using the global __baml_callbacks registry
      (window as any).__baml_callbacks = (window as any).__baml_callbacks || {};
      (window as any).__baml_callbacks[callbackId] = {
        onResult: (data: Uint8Array, isDone: boolean) => {
          try {
            // Deserialize result from protobuf
            const resultHolder = deserializeResult(data);
            
            // Register response type if provided
            if (responseType) {
              this.typeMap.set(responseType.name, responseType);
            }
            
            const result = decodeValue(resultHolder, this.typeMap) as T;
            resolve(result);
          } catch (error) {
            reject(new Error(`Failed to decode result: ${error}`));
          }
        },
        onError: (error: string) => {
          reject(new Error(error));
        },
        onTick: () => {
          console.log(`Function ${functionName} is still processing...`);
        }
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
    
    // Serialize arguments to protobuf bytes
    const protoBytes = serializeArgs(args);
    
    // Set up streaming with callbacks
    const streamId = Math.random().toString(36).substring(7);
    const callbackId = Math.floor(Math.random() * 1000000);
    const chunks: any[] = [];
    let resolver: ((value: any) => void) | null = null;
    let isDone = false;
    let errorOccurred: Error | null = null;
    
    // Register callback for streaming data using the global __baml_callbacks registry
    (window as any).__baml_callbacks = (window as any).__baml_callbacks || {};
    (window as any).__baml_callbacks[callbackId] = {
      onResult: (data: Uint8Array, done: boolean) => {
        try {
          // Deserialize result from protobuf
          const holder = deserializeResult(data);
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
        } catch (error) {
          errorOccurred = new Error(`Failed to decode stream chunk: ${error}`);
          if (resolver) {
            resolver(null);
            resolver = null;
          }
        }
      },
      onError: (error: string) => {
        errorOccurred = new Error(error);
        isDone = true;
        if (resolver) {
          resolver(null);
          resolver = null;
        }
      },
      onTick: () => {
        console.log(`Stream ${functionName} processing...`);
      }
    };
    
    // Start streaming call - use callbackId instead of streamId
    this.wasmModule.call_function_stream_wasm(
      this.runtime,
      functionName,
      protoBytes,
      callbackId.toString()
    );
    
    // Register response type if provided
    if (responseType) {
      this.typeMap.set(responseType.name, responseType);
    }
    
    // Yield chunks as they arrive
    while (!isDone && !errorOccurred) {
      if (chunks.length > 0) {
        const chunk = chunks.shift();
        if (chunk.done) {
          // Clean up callback before returning
          delete (window as any).__baml_callbacks[callbackId];
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
    delete (window as any).__baml_callbacks[callbackId];
    
    // Check if an error occurred
    if (errorOccurred) {
      throw errorOccurred;
    }
    
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