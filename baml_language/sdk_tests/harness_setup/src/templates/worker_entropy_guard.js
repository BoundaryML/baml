const originalGetRandomValues = globalThis.crypto.getRandomValues.bind(
  globalThis.crypto,
);
let entropyCalls = 0;

Object.defineProperty(globalThis.crypto, "getRandomValues", {
  configurable: true,
  value(array) {
    entropyCalls += 1;
    return originalGetRandomValues(array);
  },
});

export function entropyCallCount() {
  return entropyCalls;
}
