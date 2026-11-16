// Host proxy for a live ai.FunctionSpec<Out> value.

import { BamlHandle } from './native.js';
import { invokeTarget } from './proto.js';
import type { InvocationOptions } from './invocation.js';
import type { BamlPrompt } from './proto.js';
import type { BamlType } from './wire_ty.js';

export interface BamlFunctionSpecCallOptions {
    $baml?: InvocationOptions | null;
    client?: unknown;
    on_event?: unknown;
}

export interface BamlFunctionSpecBuildRequestOptions {
    $baml?: InvocationOptions | null;
    client?: unknown;
}

function suppliedOptions(options: object | undefined): Record<string, unknown> {
    return Object.fromEntries(
        Object.entries(options ?? {}).filter(([key, value]) => key !== '$baml' && value !== undefined),
    );
}

/** An opaque, bound LLM recipe owned by the engine that created it. */
export class BamlFunctionSpec<TOut> {
    private readonly handle: BamlHandle;

    constructor(handle: BamlHandle) {
        this.handle = handle;
    }

    /** Internal: construct a FunctionSpec proxy from a tagged heap handle. */
    static _fromHandle<TOut>(
        handle: BamlHandle,
        _classFqn: string,
    ): BamlFunctionSpec<TOut> {
        return new BamlFunctionSpec<TOut>(handle);
    }

    /** Internal: expose the inner handle for inbound encoding. */
    _toHandle(): BamlHandle {
        return this.handle;
    }

    nameSync(options?: { $baml?: InvocationOptions | null }): string {
        return this._callSync('ai.FunctionSpec.name', {}, options) as string;
    }

    async name(options?: { $baml?: InvocationOptions | null }): Promise<string> {
        return await this._callAsync('ai.FunctionSpec.name', {}, options) as string;
    }

    argumentsSync(options?: { $baml?: InvocationOptions | null }): Record<string, unknown> {
        return this._callSync('ai.FunctionSpec.arguments', {}, options) as Record<string, unknown>;
    }

    async arguments(options?: { $baml?: InvocationOptions | null }): Promise<Record<string, unknown>> {
        return await this._callAsync('ai.FunctionSpec.arguments', {}, options) as Record<string, unknown>;
    }

    outputTypeSync(options?: { $baml?: InvocationOptions | null }): BamlType {
        return this._callSync('ai.FunctionSpec.output_type', {}, options) as BamlType;
    }

    async outputType(options?: { $baml?: InvocationOptions | null }): Promise<BamlType> {
        return await this._callAsync('ai.FunctionSpec.output_type', {}, options) as BamlType;
    }

    promptSync(options?: { $baml?: InvocationOptions | null }): BamlPrompt {
        return this._callSync('ai.FunctionSpec.prompt', {}, options) as BamlPrompt;
    }

    async prompt(options?: { $baml?: InvocationOptions | null }): Promise<BamlPrompt> {
        return await this._callAsync('ai.FunctionSpec.prompt', {}, options) as BamlPrompt;
    }

    toolsSync(options?: { $baml?: InvocationOptions | null }): unknown {
        return this._callSync('ai.FunctionSpec.tools', {}, options);
    }

    async tools(options?: { $baml?: InvocationOptions | null }): Promise<unknown> {
        return await this._callAsync('ai.FunctionSpec.tools', {}, options);
    }

    requestedModelSync(options?: { $baml?: InvocationOptions | null }): string {
        return this._callSync('ai.FunctionSpec.requested_model', {}, options) as string;
    }

    async requestedModel(options?: { $baml?: InvocationOptions | null }): Promise<string> {
        return await this._callAsync('ai.FunctionSpec.requested_model', {}, options) as string;
    }

    buildRequestSync(options?: BamlFunctionSpecBuildRequestOptions): unknown {
        return this._callSync('ai.FunctionSpec.build_request', suppliedOptions(options), options);
    }

    async buildRequest(options?: BamlFunctionSpecBuildRequestOptions): Promise<unknown> {
        return await this._callAsync(
            'ai.FunctionSpec.build_request',
            suppliedOptions(options), options,
        );
    }

    parseSync(json: string, options?: { $baml?: InvocationOptions | null }): TOut {
        return this._callSync('ai.FunctionSpec.parse', { json }, options) as TOut;
    }

    async parse(json: string, options?: { $baml?: InvocationOptions | null }): Promise<TOut> {
        return await this._callAsync('ai.FunctionSpec.parse', { json }, options) as TOut;
    }

    callSync(options?: BamlFunctionSpecCallOptions): TOut {
        return this._callSync('ai.FunctionSpec.call', suppliedOptions(options), options) as TOut;
    }

    async call(options?: BamlFunctionSpecCallOptions): Promise<TOut> {
        return await this._callAsync(
            'ai.FunctionSpec.call',
            suppliedOptions(options), options,
        ) as TOut;
    }

    private _callSync(fqn: string, kwargs: Record<string, unknown> = {}, options?: { $baml?: InvocationOptions | null }): unknown {
        return invokeTarget(fqn, { self: this, ...kwargs }, options, false);
    }
    private async _callAsync(fqn: string, kwargs: Record<string, unknown> = {}, options?: { $baml?: InvocationOptions | null }): Promise<unknown> {
        return await invokeTarget(fqn, { self: this, ...kwargs }, options, true);
    }

    toString(): string {
        return '<BamlFunctionSpec>';
    }
}
