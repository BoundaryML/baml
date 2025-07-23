import type { BamlRuntime, RuntimeContextManager, ClientRegistry, Collector } from './native';
import { BamlStream } from './stream';
import { toBamlError } from './errors';

/**
 * Extended options for BAML function calls with abort signal support
 */
export interface BamlCallOptions {
  tb?: any; // TypeBuilder
  clientRegistry?: ClientRegistry;
  collector?: Collector | Collector[];
  env?: Record<string, string | undefined>;
  signal?: AbortSignal; // Added signal parameter
}

/**
 * Base class for BamlStreamClient implementations
 * This provides the core functionality for creating streams with abort support
 */
export class BaseBamlStreamClient {
  protected runtime: BamlRuntime;
  protected ctxManager: RuntimeContextManager;
  protected bamlOptions: BamlCallOptions;

  constructor(runtime: BamlRuntime, ctxManager: RuntimeContextManager, bamlOptions?: BamlCallOptions) {
    this.runtime = runtime;
    this.ctxManager = ctxManager;
    this.bamlOptions = bamlOptions || {};
  }

  /**
   * Creates a BamlStream with abort support
   */
  protected createStream<PartialType, FinalType>(
    functionName: string,
    args: Record<string, any>,
    partialCoerce: (result: any) => PartialType,
    finalCoerce: (result: any) => FinalType,
    options?: BamlCallOptions
  ): BamlStream<PartialType, FinalType> {
    try {
      const mergedOptions = { ...this.bamlOptions, ...(options || {}) };
      const collector = mergedOptions.collector 
        ? (Array.isArray(mergedOptions.collector) ? mergedOptions.collector : [mergedOptions.collector]) 
        : [];
      
      const rawEnv = mergedOptions.env 
        ? { ...process.env, ...mergedOptions.env } 
        : { ...process.env };
      
      const env: Record<string, string> = Object.fromEntries(
        Object.entries(rawEnv).filter(([_, value]) => value !== undefined) as [string, string][]
      );
      
      const raw = this.runtime.streamFunction(
        functionName,
        args,
        undefined,
        this.ctxManager.cloneContext(),
        mergedOptions.tb?.__tb?.(),
        mergedOptions.clientRegistry,
        collector,
        env
      );
      
      // Create the stream with signal support
      return new BamlStream<PartialType, FinalType>(
        raw,
        partialCoerce,
        finalCoerce,
        this.ctxManager.cloneContext(),
        { signal: mergedOptions.signal }
      );
    } catch (error) {
      throw toBamlError(error);
    }
  }
}
