// Host proxy for a live ai.FunctionSpec<Out> value.

import { BamlHandle, getRuntime, newFunctionCall as nativeNewFunctionCall } from './native.js';
import { decodeCallResult, encodeCallArgs } from './proto.js';
import type { BamlPrompt } from './proto.js';
import type { BamlType } from './wire_ty.js';

export interface BamlFunctionSpecCallOptions {
    client?: unknown;
    on_event?: unknown;
}

export interface BamlFunctionSpecBuildRequestOptions {
    client?: unknown;
}

function newFunctionCall(): bigint {
    return BigInt(nativeNewFunctionCall());
}

function suppliedOptions(options: object | undefined): Record<string, unknown> {
    return Object.fromEntries(
        Object.entries(options ?? {}).filter(([, value]) => value !== undefined),
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

    name(): string {
        return this._callSync('ai.FunctionSpec.name') as string;
    }

    async nameAsync(): Promise<string> {
        return await this._callAsync('ai.FunctionSpec.name') as string;
    }

    arguments(): Record<string, unknown> {
        return this._callSync('ai.FunctionSpec.arguments') as Record<string, unknown>;
    }

    async argumentsAsync(): Promise<Record<string, unknown>> {
        return await this._callAsync('ai.FunctionSpec.arguments') as Record<string, unknown>;
    }

    outputType(): BamlType {
        return this._callSync('ai.FunctionSpec.output_type') as BamlType;
    }

    async outputTypeAsync(): Promise<BamlType> {
        return await this._callAsync('ai.FunctionSpec.output_type') as BamlType;
    }

    prompt(): BamlPrompt {
        return this._callSync('ai.FunctionSpec.prompt') as BamlPrompt;
    }

    async promptAsync(): Promise<BamlPrompt> {
        return await this._callAsync('ai.FunctionSpec.prompt') as BamlPrompt;
    }

    tools(): unknown {
        return this._callSync('ai.FunctionSpec.tools');
    }

    async toolsAsync(): Promise<unknown> {
        return await this._callAsync('ai.FunctionSpec.tools');
    }

    clientId(): string {
        return this._callSync('ai.FunctionSpec.client_id') as string;
    }

    async clientIdAsync(): Promise<string> {
        return await this._callAsync('ai.FunctionSpec.client_id') as string;
    }

    buildRequest(options?: BamlFunctionSpecBuildRequestOptions): unknown {
        return this._callSync('ai.FunctionSpec.build_request', suppliedOptions(options));
    }

    async buildRequestAsync(options?: BamlFunctionSpecBuildRequestOptions): Promise<unknown> {
        return await this._callAsync(
            'ai.FunctionSpec.build_request',
            suppliedOptions(options),
        );
    }

    parse(json: string): TOut {
        return this._callSync('ai.FunctionSpec.parse', { json }) as TOut;
    }

    async parseAsync(json: string): Promise<TOut> {
        return await this._callAsync('ai.FunctionSpec.parse', { json }) as TOut;
    }

    call(options?: BamlFunctionSpecCallOptions): TOut {
        return this._callSync('ai.FunctionSpec.call', suppliedOptions(options)) as TOut;
    }

    async callAsync(options?: BamlFunctionSpecCallOptions): Promise<TOut> {
        return await this._callAsync(
            'ai.FunctionSpec.call',
            suppliedOptions(options),
        ) as TOut;
    }

    private _callSync(fqn: string, kwargs: Record<string, unknown> = {}): unknown {
        const argsProto = encodeCallArgs(
            { self: this, ...kwargs },
            { syncMode: true, callId: newFunctionCall(), functionName: fqn },
        );
        return decodeCallResult(getRuntime().callFunctionSync(argsProto, null, null));
    }

    private async _callAsync(
        fqn: string,
        kwargs: Record<string, unknown> = {},
    ): Promise<unknown> {
        const argsProto = encodeCallArgs(
            { self: this, ...kwargs },
            { callId: newFunctionCall(), functionName: fqn },
        );
        return decodeCallResult(await getRuntime().callFunction(argsProto, null, null));
    }

    toString(): string {
        return '<BamlFunctionSpec>';
    }
}
