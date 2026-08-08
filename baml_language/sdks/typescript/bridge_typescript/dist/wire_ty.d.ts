/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
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
/** Internal row consumed by the generated `reflect.class.new` binding. */
export declare class BamlTypeMetadataRow {
    readonly ty: BamlType;
    readonly alias: string | null;
    readonly description: string | null;
    readonly docstring: string | null;
    readonly other: Record<string, string>;
    constructor(ty: BamlType, alias: string | null, description: string | null, docstring: string | null, other?: Record<string, string>);
}
/**
 * Opaque, process-local handle for a reflected BAML definition. Only the
 * composing operations required by H-11 are public. Each wire occurrence
 * carries a copied definition graph; JavaScript identity is never type
 * identity.
 */
export declare class BamlType {
    #private;
    private constructor();
    /** @internal Bridge/codegen hook; not a host inspection surface. */
    static _fromWire(definition: baml_bridge.cffi.v1.IBamlTyDef): BamlType;
    /** @internal Bridge hook returning a fresh protobuf graph. */
    _wireCopy(): baml_bridge.cffi.v1.BamlTyDef;
    static from(token: BamlType | BamlTypeToken): BamlType;
    meta(options?: BamlTypeMetadata): BamlTypeMetadataRow;
    array(): BamlType;
    optional(): BamlType;
    toJSON(): never;
    toString(): string;
}
/** Runtime member installed as generated `reflect.type`. */
export declare const reflectType: Readonly<{
    of(token: BamlType | BamlTypeToken): BamlType;
}>;
/** Lower a statically-known token to a sparse wire `BamlTy`. */
export declare function lowerTypeToWireTy(token: BamlTypeToken): baml_bridge.cffi.v1.IBamlTy;
/** Decode the sparse value-level type channel on generated class instances. */
export declare function outboundTyToBamlTypeToken(ty: baml_bridge.cffi.v1.IBamlTy | null | undefined): BamlTypeToken | undefined;
//# sourceMappingURL=wire_ty.d.ts.map