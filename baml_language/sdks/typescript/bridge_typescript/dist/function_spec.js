/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// Host proxy for a live ai.FunctionSpec<Out> value.
import { invokeTarget } from './proto.js';
function suppliedOptions(options) {
    return Object.fromEntries(Object.entries(options ?? {}).filter(([key, value]) => key !== '$baml' && value !== undefined));
}
/** An opaque, bound LLM recipe owned by the engine that created it. */
export class BamlFunctionSpec {
    handle;
    constructor(handle) {
        this.handle = handle;
    }
    /** Internal: construct a FunctionSpec proxy from a tagged heap handle. */
    static _fromHandle(handle, _classFqn) {
        return new BamlFunctionSpec(handle);
    }
    /** Internal: expose the inner handle for inbound encoding. */
    _toHandle() {
        return this.handle;
    }
    name(options) {
        return this._callSync('ai.FunctionSpec.name', {}, options);
    }
    async nameAsync(options) {
        return await this._callAsync('ai.FunctionSpec.name', {}, options);
    }
    arguments(options) {
        return this._callSync('ai.FunctionSpec.arguments', {}, options);
    }
    async argumentsAsync(options) {
        return await this._callAsync('ai.FunctionSpec.arguments', {}, options);
    }
    outputType(options) {
        return this._callSync('ai.FunctionSpec.output_type', {}, options);
    }
    async outputTypeAsync(options) {
        return await this._callAsync('ai.FunctionSpec.output_type', {}, options);
    }
    prompt(options) {
        return this._callSync('ai.FunctionSpec.prompt', {}, options);
    }
    async promptAsync(options) {
        return await this._callAsync('ai.FunctionSpec.prompt', {}, options);
    }
    tools(options) {
        return this._callSync('ai.FunctionSpec.tools', {}, options);
    }
    async toolsAsync(options) {
        return await this._callAsync('ai.FunctionSpec.tools', {}, options);
    }
    clientId(options) {
        return this._callSync('ai.FunctionSpec.client_id', {}, options);
    }
    async clientIdAsync(options) {
        return await this._callAsync('ai.FunctionSpec.client_id', {}, options);
    }
    buildRequest(options) {
        return this._callSync('ai.FunctionSpec.build_request', suppliedOptions(options), options);
    }
    async buildRequestAsync(options) {
        return await this._callAsync('ai.FunctionSpec.build_request', suppliedOptions(options), options);
    }
    parse(json, options) {
        return this._callSync('ai.FunctionSpec.parse', { json }, options);
    }
    async parseAsync(json, options) {
        return await this._callAsync('ai.FunctionSpec.parse', { json }, options);
    }
    call(options) {
        return this._callSync('ai.FunctionSpec.call', suppliedOptions(options), options);
    }
    async callAsync(options) {
        return await this._callAsync('ai.FunctionSpec.call', suppliedOptions(options), options);
    }
    _callSync(fqn, kwargs = {}, options) {
        return invokeTarget(fqn, { self: this, ...kwargs }, options, false);
    }
    async _callAsync(fqn, kwargs = {}, options) {
        return await invokeTarget(fqn, { self: this, ...kwargs }, options, true);
    }
    toString() {
        return '<BamlFunctionSpec>';
    }
}
//# sourceMappingURL=function_spec.js.map