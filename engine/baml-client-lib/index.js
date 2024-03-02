"use strict";

const {
  initTracer,
  trace,
  setTags,
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
  console.assert(store !== undefined, 'getSpan: store is undefined');
  return store.at(-1);
}


const tracer = (cb, name, args, asKwargs, returnType) => {
  const traced_cb_wrapper = trace(cb, name, args, asKwargs, returnType);
  return (...cb_args) => traced_cb_wrapper(cb_args);
};

const tracerAsync = (cb, name, args, asKwargs, returnType) => {
  const [trace_inputs, trace_outputs] = traceAsync(cb, args, asKwargs, returnType);
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
        console.log('error:', e);
        throw e;
      }
    });
  };
};

function logLLMEvent(event) {
  logLLMEventNode(event.name, event.data);
}

initTracer();

module.exports = {
  initTracer,
  setTags,
  logLLMEvent,
  trace: tracer,
  traceAsync: tracerAsync,
};
