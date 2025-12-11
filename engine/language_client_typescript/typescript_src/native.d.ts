// Re-export the generated native bindings so TypeScript sources under
// typescript_src can resolve the relative import. Implementation lives one
// directory up (built by napi).
export * from '../native'
