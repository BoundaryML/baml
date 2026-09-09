import { baml_bridge } from './proto/baml_cffi.js';
import { BamlTypeMap } from './typemap.js';
/** The bottom type (BAML `never`). */
export declare const Never: unique symbol;
export type BamlPrimitiveToken = 'int' | 'float' | 'string' | 'bool' | 'null' | 'bytes' | 'bigint';
/** A generated class value, including live classes with protected constructors.
 * Membership is checked through the issuing SDK's constructor typemap. */
export type BamlClassCtor = Function & {
    readonly prototype: object;
};
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
declare const typeEvidence: unique symbol;
declare const nativeTypeEvidence: unique symbol;
/** @internal Not exported by the public package; only SDK factories mint evidence. */
export declare const _typeConstruction: unique symbol;
/** An opaque type when its native result type is not statically known. */
export interface BamlTypeValue {
    readonly [typeEvidence]: true;
    _wireCopy(): baml_bridge.cffi.v1.BamlTyDef;
    _checkSdk(typeMap: BamlTypeMap): void;
}
export declare function isBamlType(value: unknown): value is BamlTypeValue;
/** Native projection of the primitive tokens whose identity is unambiguous. */
export type BamlPrimitiveValue<T> = T extends 'int' | 'float' ? number : T extends 'string' | StringConstructor ? string : T extends 'bool' | BooleanConstructor ? boolean : T extends 'bigint' | BigIntConstructor ? bigint : T extends 'bytes' | Uint8ArrayConstructor ? Uint8Array : T extends 'null' ? null : T extends typeof Never ? never : unknown;
/** Keep inference tied to explicit type evidence, even in generated namespaces. */
export type BamlNoInfer<T> = NoInfer<T>;
type ExactPrimitiveToken = BamlPrimitiveToken | StringConstructor | BooleanConstructor | BigIntConstructor | Uint8ArrayConstructor | typeof Never;
export interface BamlTypeMetadata {
    alias?: string;
    description?: string;
    docstring?: string;
    other?: Record<string, string>;
}
/** Internal row consumed by the generated `reflect.class.new` binding. */
export declare class BamlTypeMetadataRow {
    readonly ty: BamlTypeValue;
    readonly alias: string | null;
    readonly description: string | null;
    readonly docstring: string | null;
    readonly other: Record<string, string>;
    constructor(ty: BamlTypeValue, alias: string | null, description: string | null, docstring: string | null, other?: Record<string, string>);
}
/** @internal Combine child graphs without dropping definitions or reassigning witnesses. */
export declare function _composeTypeDefinition(children: readonly BamlTypeValue[], root: (types: baml_bridge.cffi.v1.IBamlTy[]) => baml_bridge.cffi.v1.IBamlTy): baml_bridge.cffi.v1.IBamlTyDef;
/**
 * Opaque, process-local handle for a reflected BAML definition. Only the
 * composing operations required by H-11 are public. Each wire occurrence
 * carries a copied definition graph; JavaScript identity is never type
 * identity.
 */
export declare class BamlType<T = unknown> implements BamlTypeValue {
    #private;
    readonly [typeEvidence]: true;
    readonly [nativeTypeEvidence]: (value: T) => T;
    protected constructor(key: typeof _typeConstruction, definition: baml_bridge.cffi.v1.IBamlTyDef, checkSdk?: (typeMap: BamlTypeMap) => void);
    /** @internal Bridge/codegen hook; not a host inspection surface. */
    static _fromWire(definition: baml_bridge.cffi.v1.IBamlTyDef): BamlType;
    /** @internal Generated SDK factories supply the native type and declaration arity. */
    static _declared<T>(typeMap: BamlTypeMap, kind: 'class' | 'enum', name: string, args: readonly BamlTypeValue[]): BamlType<T>;
    /** @internal Bridge hook returning a fresh protobuf graph. */
    _wireCopy(): baml_bridge.cffi.v1.BamlTyDef;
    /** @internal Generated declaration identity survives composition. */
    _checkSdk(typeMap: BamlTypeMap): void;
    static from<T>(token: BamlType<T>): BamlType<T>;
    static from<const T extends ExactPrimitiveToken>(token: T): BamlType<BamlPrimitiveValue<T>>;
    static from<C extends abstract new (...args: never[]) => object>(token: C): BamlType<InstanceType<C>>;
    static from(token: BamlTypeValue | BamlTypeToken): BamlType<unknown>;
    meta(options?: BamlTypeMetadata): BamlTypeMetadataRow;
    array(): BamlType<T[]>;
    optional(): BamlType<T | null>;
    toJSON(): never;
    toString(): string;
}
/** Runtime member installed as generated `reflect.Type`. */
export declare const reflectType: Readonly<{
    of: typeof BamlType.from;
}>;
/** Lower a statically-known token to a sparse wire `BamlTy`. */
export declare function lowerTypeToWireTy(token: BamlTypeToken, typeMap?: BamlTypeMap): baml_bridge.cffi.v1.IBamlTy;
/** Decode the sparse value-level type channel on generated class instances. */
export declare function outboundTyToBamlTypeToken(ty: baml_bridge.cffi.v1.IBamlTy | null | undefined, typeMap?: BamlTypeMap): BamlTypeToken | undefined;
export {};
//# sourceMappingURL=wire_ty.d.ts.map