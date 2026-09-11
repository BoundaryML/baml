/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { getRuntime, getTypeMap, withTypeMap } from './typemap.js';
// Host proxy for a live ai.FunctionSpec<Out> value.
import { newFunctionCall as nativeNewFunctionCall } from './native.js';
import { decodeCallResult, encodeCallArgs } from './proto.js';
function newFunctionCall() {
    return BigInt(nativeNewFunctionCall());
}
function suppliedOptions(options) {
    return Object.fromEntries(Object.entries(options ?? {}).filter(([, value]) => value !== undefined));
}
/** An opaque, bound LLM recipe owned by the engine that created it. */
export class BamlFunctionSpec {
    _typeMap = getTypeMap();
    _encodeCallArgs(...args) { return withTypeMap(this._typeMap, () => encodeCallArgs(...args)); }
    _decodeCallResult(...args) { return withTypeMap(this._typeMap, () => decodeCallResult(...args)); }
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
    name() {
        return this._callSync('ai.FunctionSpec.name');
    }
    async nameAsync() {
        return await this._callAsync('ai.FunctionSpec.name');
    }
    arguments() {
        return this._callSync('ai.FunctionSpec.arguments');
    }
    async argumentsAsync() {
        return await this._callAsync('ai.FunctionSpec.arguments');
    }
    outputType() {
        return this._callSync('ai.FunctionSpec.output_type');
    }
    async outputTypeAsync() {
        return await this._callAsync('ai.FunctionSpec.output_type');
    }
    prompt() {
        return this._callSync('ai.FunctionSpec.prompt');
    }
    async promptAsync() {
        return await this._callAsync('ai.FunctionSpec.prompt');
    }
    tools() {
        return this._callSync('ai.FunctionSpec.tools');
    }
    async toolsAsync() {
        return await this._callAsync('ai.FunctionSpec.tools');
    }
    clientId() {
        return this._callSync('ai.FunctionSpec.client_id');
    }
    async clientIdAsync() {
        return await this._callAsync('ai.FunctionSpec.client_id');
    }
    buildRequest(options) {
        return this._callSync('ai.FunctionSpec.build_request', suppliedOptions(options));
    }
    async buildRequestAsync(options) {
        return await this._callAsync('ai.FunctionSpec.build_request', suppliedOptions(options));
    }
    parse(json) {
        return this._callSync('ai.FunctionSpec.parse', { json });
    }
    async parseAsync(json) {
        return await this._callAsync('ai.FunctionSpec.parse', { json });
    }
    call(options) {
        return this._callSync('ai.FunctionSpec.call', suppliedOptions(options));
    }
    async callAsync(options) {
        return await this._callAsync('ai.FunctionSpec.call', suppliedOptions(options));
    }
    _callSync(fqn, kwargs = {}) {
        const argsProto = this._encodeCallArgs({ self: this, ...kwargs }, { syncMode: true, callId: newFunctionCall(), functionName: fqn });
        return this._decodeCallResult((this._typeMap.runtime ?? getRuntime()).callFunctionSync(argsProto, null, null));
    }
    async _callAsync(fqn, kwargs = {}) {
        const argsProto = this._encodeCallArgs({ self: this, ...kwargs }, { callId: newFunctionCall(), functionName: fqn });
        return this._decodeCallResult(await (this._typeMap.runtime ?? getRuntime()).callFunction(argsProto, null, null));
    }
    toString() {
        return '<BamlFunctionSpec>';
    }
}
//# sourceMappingURL=function_spec.js.map