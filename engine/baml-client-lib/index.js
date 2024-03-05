"use strict";

const {
  initTracer,
  trace,
  setVariant: setVariantNode,
  setTags: setTagsNode,
  traceAsync,
  logLLMEvent: logLLMEventNode,
  getSpanForAsync
} = require("./index.node");
const {AsyncLocalStorage} = require('async_hooks');


// Create a new instance of AsyncLocalStorage
const asyncLocalStorage = new AsyncLocalStorage();

const createSpan = (name) => {
  let store = asyncLocalStorage.getStore();

  console.assert(store !== undefined, 'getSpan: store is undefined');
  if (store.length > 0) {
    const span = getSpanForAsync(name, store.at(-1));
    store.push(span);
  } else {
    store.push(getSpanForAsync(name));
  }
  return store.at(-1);
}

const getSpan = () => {
  let store = asyncLocalStorage.getStore();
  if (store === undefined) {
    return undefined;
  }
  return store.at(-1);
}


const tracer = (cb, name, args, asKwargs, returnType) => {
  const traced_cb_wrapper = trace(cb, name, args, asKwargs, returnType);
  return (...cb_args) => traced_cb_wrapper(cb_args);
};

const tracerAsync = (cb, name, args, asKwargs, returnType) => {
  const [trace_inputs, trace_outputs, trace_errors] = traceAsync(cb, args, asKwargs, returnType);
  return async (...cb_args) => {
    let store = asyncLocalStorage.getStore() ?? [];
    return await asyncLocalStorage.run([...store], async () => {
      let span = createSpan(name);
      trace_inputs(span, cb_args);
      try {
        const result = await cb(...cb_args);
        trace_outputs(span, result);
        return result;
      } catch (e) {
        if (e instanceof Error) {
          trace_errors(span, 2, e.message, e.stack?.split('\n').slice(1).join('\n'));
        } else {
          trace_errors(span, 2, `${e}`);
        }
        throw e;
      }
    });
  };
};

function logLLMEvent(event) {
  const span = getSpan();
  if (span === undefined) {
    console.warn('BAML: Attempting to call an LLM event without an active span. Ignoring.');
    return;
  }
  logLLMEventNode(span, {
    name: event.name, 
    meta: JSON.stringify(event.data)
  });
}

function setTags(tags) {
  const span = getSpan();
  if (span) {
    setTagsNode(span, tags);
  } else {
    setTagsNode(tags);
  }
}

function setVariant(variant) {
  const span = getSpan();
  if (span) {
    setVariantNode(span, variant);
  } else {
    setVariantNode(variant);
  }
}

initTracer();

module.exports = {
  initTracer,
  setTags,
  setVariant,
  logLLMEvent,
  trace: tracer,
  traceAsync: tracerAsync,
};
