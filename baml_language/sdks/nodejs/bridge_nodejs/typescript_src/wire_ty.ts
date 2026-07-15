// wire_ty.ts — lower a host-supplied type token to a wire `Ty` (baml_type.proto).
//
// The Node analog of `python_type_to_wire_ty` / `_fill_wire_ty` in
// sdks/python/src/baml_bridge/proto.py. Python recovers a generic instance's
// concrete type args from Pydantic's runtime generic metadata and lowers
// Python `type` objects (`int`, `str`, `Box[int]`, …) to a wire `Ty`. TypeScript
// erases generics at runtime, so there is no metadata to recover and no `type`
// object to inspect — the host must spell the binding explicitly. A generic
// class instance carries its bindings in a `$types` field, and a generic
// function/method call carries them in a `$types` call option; both hold
// {@link BamlType} tokens that this module lowers.
//
// `undefined` / `null` (an absent binding) lowers to the unknown/top type —
// the engine treats it as a wildcard, matching Python's `_fill_wire_ty(None)`.

import { baml_bridge } from './proto/baml_cffi.js';
import { BamlAudio, BamlImage, BamlPdf, BamlVideo } from './native.js';
import { getTypeMap } from './typemap.js';

const TyPrimitiveKind = baml_bridge.cffi.v1.BamlTyPrimitiveKind;
const TyMediaKind = baml_bridge.cffi.v1.BamlTyMediaKind;

/**
 * The bottom type (BAML `never`). Pass as a `$types` binding to bind a TypeVar
 * to `never`, mirroring Python's `_types={"T": Never}`.
 */
export const Never: unique symbol = Symbol('baml.Never');

/** Unambiguous primitive spellings. JS has a single `number`, so `int` and
 * `float` are distinguished by an explicit token rather than a constructor. */
export type BamlPrimitiveToken =
    | 'int'
    | 'float'
    | 'string'
    | 'bool'
    | 'null'
    | 'bytes'
    | 'bigint';

/** A constructor for a codegen-emitted BAML class (`Box`, `Resume`, …). */
export type BamlClassCtor = new (...args: never[]) => unknown;

/** A generated TypeScript enum object (`typeof Status`). */
export type BamlEnumObject = object;

/** Runtime constructors re-exported by generated SDKs as Image/Audio/Video/Pdf. */
export type BamlMediaCtor =
    | typeof BamlImage
    | typeof BamlAudio
    | typeof BamlVideo
    | typeof BamlPdf;

/** An exact BAML literal type. The tag keeps `int`/`float` and primitive type
 * tokens unambiguous despite JavaScript's erased scalar types. The host keeps
 * this exact metadata; the engine intentionally widens a literal when it is
 * reused as a TypeVar binding. */
export type BamlLiteralToken =
    | { kind: 'string'; value: string }
    | { kind: 'int'; value: number }
    | { kind: 'bool'; value: boolean }
    | { kind: 'bigint'; value: bigint }
    | { kind: 'float'; value: string };

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
export type BamlType =
    | BamlPrimitiveToken
    | typeof Never
    | BamlClassCtor
    | BamlMediaCtor
    | { class: BamlClassCtor; args?: BamlType[] }
    | { list: BamlType }
    | { map: [BamlType, BamlType] }
    | { optional: BamlType }
    | { union: BamlType[] }
    | { enum: BamlEnumObject }
    | { literal: BamlLiteralToken }
    | BamlTypeAliasToken
    | null
    | undefined;

const PRIMITIVE_KIND: Record<BamlPrimitiveToken, number> = {
    int: TyPrimitiveKind.BAML_TY_PRIMITIVE_INT,
    float: TyPrimitiveKind.BAML_TY_PRIMITIVE_FLOAT,
    string: TyPrimitiveKind.BAML_TY_PRIMITIVE_STRING,
    bool: TyPrimitiveKind.BAML_TY_PRIMITIVE_BOOL,
    null: TyPrimitiveKind.BAML_TY_PRIMITIVE_NULL,
    bytes: TyPrimitiveKind.BAML_TY_PRIMITIVE_BYTES,
    bigint: TyPrimitiveKind.BAML_TY_PRIMITIVE_BIGINT,
};

const MEDIA_CTOR_KIND = new Map<BamlMediaCtor, number>([
    [BamlImage, TyMediaKind.BAML_TY_MEDIA_KIND_IMAGE],
    [BamlAudio, TyMediaKind.BAML_TY_MEDIA_KIND_AUDIO],
    [BamlVideo, TyMediaKind.BAML_TY_MEDIA_KIND_VIDEO],
    [BamlPdf, TyMediaKind.BAML_TY_MEDIA_KIND_PDF],
]);

const TY_MEDIA_CTOR: Record<number, BamlMediaCtor> = {
    [TyMediaKind.BAML_TY_MEDIA_KIND_IMAGE]: BamlImage,
    [TyMediaKind.BAML_TY_MEDIA_KIND_AUDIO]: BamlAudio,
    [TyMediaKind.BAML_TY_MEDIA_KIND_VIDEO]: BamlVideo,
    [TyMediaKind.BAML_TY_MEDIA_KIND_PDF]: BamlPdf,
};

/**
 * Lower a {@link BamlType} token to a wire `BamlTy` (an `IBamlTy` plain object the
 * protobufjs `fromObject` path accepts). Mirrors `_fill_wire_ty`: an
 * unrecognized or absent token leaves the unknown/top type, which binds
 * nothing.
 */
export function lowerTypeToWireTy(token: BamlType): baml_bridge.cffi.v1.IBamlTy {
    // Absent binding → unknown/top (matches Python's `_fill_wire_ty(None)`).
    if (token === null || token === undefined) {
        return { unknown: {} };
    }
    // Bottom type.
    if (token === Never) {
        return { never: {} };
    }
    // Primitive spelling.
    if (typeof token === 'string') {
        const kind = PRIMITIVE_KIND[token as BamlPrimitiveToken];
        if (kind !== undefined) {
            return { primitive: { kind } };
        }
        return { unknown: {} };
    }
    // Media constructors are functions too, so dispatch before the generic
    // codegen-class path.
    if (typeof token === 'function') {
        const mediaKind = MEDIA_CTOR_KIND.get(token as BamlMediaCtor);
        if (mediaKind !== undefined) {
            return { media: { kind: mediaKind } };
        }
        // A bare class constructor → that class, no concrete args.
        return classWireTy(token as BamlClassCtor, []);
    }
    if (typeof token === 'object') {
        if ('class' in token) {
            return classWireTy(token.class, token.args ?? []);
        }
        if ('list' in token) {
            return { list: { item: lowerTypeToWireTy(token.list) } };
        }
        if ('map' in token) {
            const [k, v] = token.map;
            return { map: { key: lowerTypeToWireTy(k), value: lowerTypeToWireTy(v) } };
        }
        if ('optional' in token) {
            return { optional: { inner: lowerTypeToWireTy(token.optional) } };
        }
        if ('union' in token) {
            return { union: { options: token.union.map(lowerTypeToWireTy) } };
        }
        if ('enum' in token) {
            const fqn = getTypeMap().jsTypeToBamlType(token.enum);
            return fqn ? { enum: { name: fqn } } : { unknown: {} };
        }
        if ('literal' in token) {
            return literalWireTy(token.literal);
        }
        if ('typeAlias' in token) {
            if (!token.typeAlias) return { unknown: {} };
            try {
                getTypeMap().getTypeAlias(token.typeAlias);
            } catch {
                return { unknown: {} };
            }
            return {
                typeAlias: {
                    name: token.typeAlias,
                    typeArgs: (token.args ?? []).map(lowerTypeToWireTy),
                },
            };
        }
    }
    // Unrecognized: leave as unknown/top (binds nothing).
    return { unknown: {} };
}

const TY_PRIMITIVE_TOKEN: Record<number, BamlPrimitiveToken> = {
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_INT]: 'int',
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_FLOAT]: 'float',
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_STRING]: 'string',
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_BOOL]: 'bool',
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_NULL]: 'null',
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_BYTES]: 'bytes',
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_BIGINT]: 'bigint',
};

function literalWireTy(token: BamlLiteralToken): baml_bridge.cffi.v1.IBamlTy {
    switch (token.kind) {
        case 'string':
            return { literal: { stringValue: token.value } };
        case 'int':
            return Number.isSafeInteger(token.value)
                ? { literal: { intValue: token.value } }
                : { unknown: {} };
        case 'bool':
            return { literal: { boolValue: token.value } };
        case 'bigint':
            return { literal: { bigintValue: token.value.toString(10) } };
        case 'float':
            return { literal: { floatValue: token.value } };
    }
}

function outboundLiteral(
    literal: baml_bridge.cffi.v1.IBamlTyLiteral,
): BamlLiteralToken | undefined {
    const oneof = (literal as baml_bridge.cffi.v1.BamlTyLiteral).literal;
    if (oneof === 'stringValue' || (oneof === undefined && literal.stringValue != null)) {
        return { kind: 'string', value: literal.stringValue ?? '' };
    }
    if (oneof === 'intValue' || (oneof === undefined && literal.intValue != null)) {
        const value = Number(literal.intValue);
        return Number.isSafeInteger(value) ? { kind: 'int', value } : undefined;
    }
    if (oneof === 'boolValue' || (oneof === undefined && literal.boolValue != null)) {
        return { kind: 'bool', value: literal.boolValue ?? false };
    }
    if (oneof === 'bigintValue' || (oneof === undefined && literal.bigintValue != null)) {
        try {
            return { kind: 'bigint', value: BigInt(literal.bigintValue ?? '') };
        } catch {
            return undefined;
        }
    }
    if (oneof === 'floatValue' || (oneof === undefined && literal.floatValue != null)) {
        return { kind: 'float', value: literal.floatValue ?? '' };
    }
    return undefined;
}

function isTypeAliasToken(value: unknown): value is BamlTypeAliasToken {
    return typeof value === 'object'
        && value !== null
        && 'typeAlias' in value
        && typeof value.typeAlias === 'string';
}

/**
 * Decode a wire `Ty` (baml_type.proto) back to a {@link BamlType} token — the
 * exact inverse of {@link lowerTypeToWireTy}, used to repopulate a generic
 * instance's `$types` field on decode. Mirrors the engine's
 * `ty_encode::runtime_ty_to_proto_ty` and Python's `_ty_to_python_type`.
 * Positions with no concrete JS binding (a type variable or opaque/runtime-only
 * type) decode to `undefined`, i.e. an unbound wildcard.
 */
export function outboundTyToBamlType(
    ty: baml_bridge.cffi.v1.IBamlTy | null | undefined,
): BamlType {
    if (!ty) return undefined;
    if (ty.primitive) {
        return TY_PRIMITIVE_TOKEN[ty.primitive.kind ?? -1] ?? undefined;
    }
    if (ty.list) return { list: outboundTyToBamlType(ty.list.item) };
    if (ty.map) {
        return { map: [outboundTyToBamlType(ty.map.key), outboundTyToBamlType(ty.map.value)] };
    }
    if (ty.optional) return { optional: outboundTyToBamlType(ty.optional.inner) };
    if (ty.union) {
        return { union: (ty.union.options ?? []).map(outboundTyToBamlType) };
    }
    if (ty.classTy) {
        const fqn = ty.classTy.name ?? '';
        const args = (ty.classTy.typeArgs ?? []).map((a) => outboundTyToBamlType(a));
        let ctor: BamlClassCtor | undefined;
        try {
            ctor = getTypeMap().getClass(fqn) as BamlClassCtor;
        } catch {
            ctor = undefined; // unmapped FQN — leave unbound
        }
        if (!ctor) return undefined;
        return args.length ? { class: ctor, args } : ctor;
    }
    if (ty.enum) {
        const fqn = ty.enum.name ?? '';
        try {
            const enumObject = getTypeMap().getEnum(fqn);
            return typeof enumObject === 'object' && enumObject !== null
                ? { enum: enumObject }
                : undefined;
        } catch {
            return undefined;
        }
    }
    if (ty.literal) {
        const literal = outboundLiteral(ty.literal);
        return literal === undefined ? undefined : { literal };
    }
    if (ty.typeAlias) {
        const fqn = ty.typeAlias.name ?? '';
        const args = (ty.typeAlias.typeArgs ?? []).map(outboundTyToBamlType);
        try {
            const alias = getTypeMap().getTypeAlias(fqn);
            if (!isTypeAliasToken(alias)) return undefined;
            return args.length ? { ...alias, args } : alias;
        } catch {
            return undefined;
        }
    }
    if (ty.media) {
        return TY_MEDIA_CTOR[ty.media.kind ?? -1] ?? undefined;
    }
    if (ty.never) return Never;
    // type_var / interface / function / opaque/runtime-only / unknown / any
    // other unrepresentable type → unbound wildcard.
    return undefined;
}

/** Build a `class_ty` wire `Ty` for a codegen class constructor and its
 * concrete generic args. The FQN comes from the typemap reverse map; an
 * unmapped constructor lowers to unknown so it can't manufacture a bogus
 * class reference. */
function classWireTy(ctor: BamlClassCtor, args: BamlType[]): baml_bridge.cffi.v1.IBamlTy {
    const fqn = getTypeMap().jsTypeToBamlType(ctor);
    if (!fqn) {
        return { unknown: {} };
    }
    return { classTy: { name: fqn, typeArgs: args.map(lowerTypeToWireTy) } };
}
