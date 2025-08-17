// Global callback registry
window.__baml_callbacks = window.__baml_callbacks || {};

export function js_result_callback(callId, isDone, data) {
  const callback = window.__baml_callbacks[callId];
  if (callback) {
    callback.onResult(data, isDone);
    if (isDone) {
      delete window.__baml_callbacks[callId];
    }
  }
}

export function js_error_callback(callId, error) {
  const callback = window.__baml_callbacks[callId];
  if (callback) {
    callback.onError(error);
    delete window.__baml_callbacks[callId];
  }
}

export function js_on_tick_callback(callId) {
  const callback = window.__baml_callbacks[callId];
  if (callback && callback.onTick) {
    callback.onTick();
  }
}