/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
import { baml_bridge } from './proto/baml_cffi.js';
import { BamlAudio, BamlImage, BamlPdf, BamlVideo } from './native.js';
/**
 * The bottom type (BAML `never`). Pass as a `$types` binding to bind a TypeVar
 * to `never`, mirroring Python's `_types={"T": Never}`.
 */
export declare const Never: unique symbol;
/** Unambiguous primitive spellings. JS has a single `number`, so `int` and
 * `float` are distinguished by an explicit token rather than a constructor. */
export type BamlPrimitiveToken = 'int' | 'float' | 'string' | 'bool' | 'null' | 'bytes' | 'bigint';
/** A constructor for a codegen-emitted BAML class (`Box`, `Resume`, …). */
export type BamlClassCtor = new (...args: never[]) => unknown;
/** A generated TypeScript enum object (`typeof Status`). */
export type BamlEnumObject = object;
/** Runtime constructors re-exported by generated SDKs as Image/Audio/Video/Pdf. */
export type BamlMediaCtor = typeof BamlImage | typeof BamlAudio | typeof BamlVideo | typeof BamlPdf;
/** An exact BAML literal type. The tag keeps `int`/`float` and primitive type
 * tokens unambiguous despite JavaScript's erased scalar types. The host keeps
 * this exact metadata; the engine intentionally widens a literal when it is
 * reused as a TypeVar binding. */
export type BamlLiteralToken = {
    kind: 'string';
    value: string;
} | {
    kind: 'int';
    value: number;
} | {
    kind: 'bool';
    value: boolean;
} | {
    kind: 'bigint';
    value: bigint;
} | {
    kind: 'float';
    value: string;
};
/** Runtime descriptor emitted next to a TypeScript type alias. TypeScript
 * aliases are erased, so codegen emits a same-named value carrying the BAML
 * FQN; recursive aliases are therefore still usable as `$types` bindings. */
export type BamlTypeAliasToken = {
    readonly typeAlias: string;
    readonly args?: BamlType[];
};
/**
 * A runtime spelling of a BAML type, used as a `$types` binding for a TypeVar.
 *
 * - `undefined` / `null` → the unknown/top type (an unbound wildcard).
 * - {@link Never} → the bottom type.
 * - a primitive token (`'int'`, `'string'`, …) → that primitive.
 * - a codegen class constructor (`Box`) → that class with no concrete args.
 * - `{ class, args }` → a parameterized generic class (`Box<int>`).
 * - `{ list }` / `{ map }` / `{ optional }` / `{ union }` → the container shape.
 * - `{ enum }` → a generated enum type (`{ enum: Status }`).
 * - `{ literal }` → an exact literal type.
 * - a generated type-alias descriptor → that named alias.
 * - a generated media constructor (`Image`, `Audio`, `Video`, `Pdf`) → media.
 */
export type BamlType = BamlPrimitiveToken | typeof Never | BamlClassCtor | BamlMediaCtor | {
    class: BamlClassCtor;
    args?: BamlType[];
} | {
    list: BamlType;
} | {
    map: [BamlType, BamlType];
} | {
    optional: BamlType;
} | {
    union: BamlType[];
} | {
    enum: BamlEnumObject;
} | {
    literal: BamlLiteralToken;
} | BamlTypeAliasToken | null | undefined;
/**
 * Lower a {@link BamlType} token to a wire `BamlTy` (an `IBamlTy` plain object the
 * protobufjs `fromObject` path accepts). Mirrors `_fill_wire_ty`: an
 * unrecognized or absent token leaves the unknown/top type, which binds
 * nothing.
 */
export declare function lowerTypeToWireTy(token: BamlType): baml_bridge.cffi.v1.IBamlTy;
/**
 * Decode a wire `Ty` (baml_type.proto) back to a {@link BamlType} token — the
 * exact inverse of {@link lowerTypeToWireTy}, used to repopulate a generic
 * instance's `$types` field on decode. Mirrors the engine's
 * `ty_encode::runtime_ty_to_proto_ty` and Python's `_ty_to_python_type`.
 * Positions with no concrete JS binding (a type variable or opaque/runtime-only
 * type) decode to `undefined`, i.e. an unbound wildcard.
 */
export declare function outboundTyToBamlType(ty: baml_bridge.cffi.v1.IBamlTy | null | undefined): BamlType;
//# sourceMappingURL=wire_ty.d.ts.map