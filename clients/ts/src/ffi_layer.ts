import { BamlScopeGuard, BamlTracer as BamlNodeTracer } from "@boundaryml/baml-core-ffi";
import { LLMException } from "./client_manager/errors";
import { AsyncLocalStorage } from 'async_hooks';

const asyncThreadScopeGuard = new AsyncLocalStorage<BamlScopeGuard | undefined>();


const createGuard = (functionName: string, returnType: string, parameters: Array<[string, string]>, asKwarg: boolean): BamlScopeGuard => {
  const current = asyncThreadScopeGuard.getStore();
  if (current) {
    return current.child(functionName, returnType, parameters, asKwarg);
  }
  return BamlScopeGuard.create(functionName, returnType, parameters, asKwarg);
}

const getGuard = (): BamlScopeGuard | undefined => {
  return asyncThreadScopeGuard.getStore();
}

const tracer = new BamlNodeTracer();

process.on('exit', (code) => {
  const now = Date.now();
  console.log(`${now} Flushing tracer: ${code}`);
  tracer.flush();
  const now12 = Date.now();
  console.log(`${now12} Done tracer: ${code}`);
});

// Automatically start the tracer
tracer.start();

const BamlTracer = {
  start: () => tracer.start(),
  stop: () => tracer.stop(),
  flush: () => tracer.flush(),
};

type FunctionCallback<T, R> = (param: T) => R;

const serializeArg = <T>(arg_type: 'positional' | 'named', arg: T): any[] | Record<string, any> => {
  if (arg_type === 'named') {
    if (typeof arg !== 'object' || Array.isArray(arg)) {
      throw new Error('Named arguments must be an object');
    }
    if (arg === null) {
      return {};
    }
    return Object.fromEntries(Object.entries(arg).map(([key, value]) => [key, JSON.stringify(value)]));
  } else {
    return [JSON.stringify(arg)];
  }
}

const trace = <T, R>(functionName: string, returnType: string, parameters: Array<[string, string]>, arg_type: 'positional' | 'named', cb: FunctionCallback<T, R>): FunctionCallback<T, R> => {
  return (arg: T) => {
    const scopeGuard = createGuard(functionName, returnType, parameters, arg_type === 'named');
    scopeGuard.logInputs(serializeArg(arg_type, arg));
    try {
      const result = cb(arg);
      scopeGuard.logOutput(JSON.stringify(result));
      return result;
    } catch (error) {
      if (error instanceof Error) {
        const code = error instanceof LLMException ? error.code : -2;
        scopeGuard.logError(code, error.message, error.stack);
      }
      throw error;
    } finally {
      scopeGuard.close();
    }
  };
};

const traceAsync = <T, R>(
  functionName: string,
  returnType: string,
  parameters: Array<[string, string]>,
  arg_type: 'positional' | 'named',
  cb: FunctionCallback<T, Promise<R>>
): FunctionCallback<T, Promise<R>> => {
  return async (arg: T) => {
    const scopeGuard = createGuard(functionName, returnType, parameters, arg_type === 'named');
    return await asyncThreadScopeGuard.run(scopeGuard, async () => {
      scopeGuard.logInputs(serializeArg(arg_type, arg));
      try {
        const result = await cb(arg);
        scopeGuard.logOutput(JSON.stringify(result));
        return result;
      } catch (error) {
        if (error instanceof Error) {
          const code = error instanceof LLMException ? error.code : -2;
          scopeGuard.logError(code, error.message, error.stack);
        } else {
          scopeGuard.logError(-2, `${error}`);
        }
        throw error;
      }
    }).then((result) => {
      scopeGuard.close()
      return result;
    }).catch((error) => {
      scopeGuard.close()
      throw error;
    });
  }
}

const FireBamlEvent = {
  tags(event: { [key: string]: string | null }) {
    const guard = getGuard();
    if (guard) {
      guard.setTags(event);
    }
  },
  variant(event: string) {
    const guard = getGuard();
    if (guard) {
      guard.logVariant(event);
    }
  },
  llmStart(event: {
    prompt:
    | string
    | {
      role: string
      content: string
    }[]
    provider: string
  }) {
    const guard = getGuard();
    if (guard) {
      guard.logLlmStart(event);
    }
  },
  llmEnd(event: { model_name: string; generated: string; metadata: any }) {
    const guard = getGuard();
    if (guard) {
      guard.logLlmEnd(event);
    }
  },
  llmError(event: { error_code: number; message?: string; traceback?: string }) {
    const guard = getGuard();
    if (guard) {
      guard.logLlmError(event);
    }
  },
  llmCacheHit(event: number) {
    const guard = getGuard();
    if (guard) {
      guard.logLlmCacheHit(event);
    }
  },
  llmArgs(args: { [key: string]: any }) {
    const guard = getGuard();
    if (guard) {
      guard.logLlmArgs(args);
    }
  },
  llmTemplateArgs(args: {
    template:
    | string
    | {
      role: string
      content: string
    }[]
    template_args: {
      [key: string]: string
    }
  }) {
    const guard = getGuard();
    if (guard) {
      guard.logLlmTemplateArgs(args);
    }
  },
}

export { trace, traceAsync, FireBamlEvent, BamlTracer };
