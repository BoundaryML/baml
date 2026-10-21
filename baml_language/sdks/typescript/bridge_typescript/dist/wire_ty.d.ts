/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { type HandleKey } from './native.js';
import { baml_bridge } from './proto/baml_cffi.js';
/** The bottom type (BAML `never`). */
export declare const Never: unique symbol;
export type BamlPrimitiveToken = 'int' | 'float' | 'string' | 'bool' | 'null' | 'bytes' | 'bigint';
/** A constructor for a codegen-emitted BAML class. */
export type BamlClassCtor = new (...args: never[]) => unknown;
/** A codegen-emitted erased interface token. */
export type BamlInterfaceToken = {
    readonly __baml_interface_fqn__: string;
};
/**
 * A runtime spelling of a statically-known BAML type. TypeScript erases its
 * type grammar, so recursive containers use small data constructors and
 * generated classes/enums are passed as their emitted runtime values.
 */
export type BamlTypeToken = BamlPrimitiveToken | StringConstructor | BooleanConstructor | BigIntConstructor | Uint8ArrayConstructor | typeof Never | BamlClassCtor | BamlInterfaceToken | Record<string, string> | {
    class: BamlClassCtor;
    args?: BamlTypeToken[];
} | {
    list: BamlTypeToken;
} | {
    map: [BamlTypeToken, BamlTypeToken];
} | {
    optional: BamlTypeToken;
} | {
    union: BamlTypeToken[];
};
export interface BamlTypeMetadata {
    alias?: string;
    description?: string;
    docstring?: string;
    other?: Record<string, string>;
}
/** Internal row consumed by the generated `reflect.Class.new` binding. */
export declare class BamlTypeMetadataRow {
    readonly ty: BamlType;
    readonly alias: string | null;
    readonly description: string | null;
    readonly docstring: string | null;
    readonly other: Record<string, string>;
    constructor(ty: BamlType, alias: string | null, description: string | null, docstring: string | null, other?: Record<string, string>);
}
type Ty = baml_bridge.cffi.v1.IBamlTy;
/**
 * Every `live` head under a wire type, once per occurrence: each is a row of
 * the engine's handle table naming a runtime declaration.
 */
export declare function liveKeys(ty: Ty | null | undefined): HandleKey[];
/**
 * Opaque, process-local handle for a reflected BAML type. Only the composing
 * operations required by H-11 are public. A runtime declaration the type
 * names is held by the engine row it arrived under; a type sent back states
 * it with a reference of the call's own. JavaScript identity is never type
 * identity.
 */
export declare class BamlType {
    #private;
    private constructor();
    /** @internal A type the engine handed over: its `live` heads are rows this value now holds. */
    static _fromWire(ty: Ty): BamlType;
    /** @internal Bridge hook returning a fresh protobuf tree. */
    _wireCopy(): baml_bridge.cffi.v1.BamlTy;
    static from(token: BamlType | BamlTypeToken): BamlType;
    meta(options?: BamlTypeMetadata): BamlTypeMetadataRow;
    array(): BamlType;
    optional(): BamlType;
    toJSON(): never;
    toString(): string;
}
/** Runtime member installed as generated `reflect.Type`. */
export declare const reflectType: Readonly<{
    of(token: BamlType | BamlTypeToken): BamlType;
}>;
/** Lower a statically-known token to a sparse wire `BamlTy`. */
export declare function lowerTypeToWireTy(token: BamlTypeToken): baml_bridge.cffi.v1.IBamlTy;
/** Decode the sparse value-level type channel on generated class instances. */
export declare function outboundTyToBamlTypeToken(ty: baml_bridge.cffi.v1.IBamlTy | null | undefined): BamlTypeToken | undefined;
export {};
//# sourceMappingURL=wire_ty.d.ts.map