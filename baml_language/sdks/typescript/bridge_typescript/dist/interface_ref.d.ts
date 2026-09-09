/** Owned base for generated interface callers. Native handles are the authority;
 * diagnostic names and JavaScript class shapes cannot grant implementation. */
import { BamlHandle, BamlCallContext } from './native.js';
import { baml_bridge } from './proto/baml_cffi.js';
import { BamlTypeMap } from './typemap.js';
import { BamlType, type BamlTypeValue } from './wire_ty.js';
/** A generated interface type with fixed ordinary and associated arguments. */
export declare class BamlInterfaceType<R extends BamlInterfaceRef> extends BamlType<R> {
    private constructor();
    /** @internal Generated declarations supply the exact argument layout. */
    static _create<R extends BamlInterfaceRef>(reference: Function, name: string, args: readonly BamlTypeValue[], bindings: ReadonlyArray<readonly [string, BamlTypeValue]>): BamlInterfaceType<R>;
}
export declare class BamlRef {
    #private;
    protected constructor(handle: BamlHandle, typeMap: BamlTypeMap);
    /** Decoder-only factory. Adoption belongs to the enclosing result receipt. */
    static _fromHandle(handle: BamlHandle, typeMap: BamlTypeMap): BamlRef;
    /** Encoder-only borrowing; the encoder clones a separate wire lease. */
    _toHandle(): BamlHandle;
    close(): void;
    toJSON(): never;
    clone(): this;
    /** Check an exact view on the same receiver; never rebind or copy its state. */
    as_interface<R extends BamlInterfaceRef>(target: BamlInterfaceType<R>): Promise<R>;
    protected _invokeConcrete(className: string, pattern: readonly number[] | null, member: string, arguments_: Record<string, unknown>, choices?: Record<string, BamlTypeValue>, parameters?: readonly string[], context?: BamlCallContext): Promise<unknown>;
    protected _invoke(member: string, arguments_: Record<string, unknown>, choices?: Record<string, BamlTypeValue>, parameters?: readonly string[], context?: BamlCallContext, concreteMethod?: baml_bridge.cffi.v1.IConcreteMethodTarget): Promise<unknown>;
}
export declare class BamlInterfaceRef extends BamlRef {
}
declare const concreteTypeEvidence: unique symbol;
export declare class BamlConcreteRef<Pins extends readonly unknown[] = readonly unknown[]> extends BamlRef {
    readonly [concreteTypeEvidence]: (pins: Pins) => Pins;
}
export {};
//# sourceMappingURL=interface_ref.d.ts.map