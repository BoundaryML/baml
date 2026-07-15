/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
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
export const Never = Symbol('baml.Never');
const PRIMITIVE_KIND = {
    int: TyPrimitiveKind.BAML_TY_PRIMITIVE_INT,
    float: TyPrimitiveKind.BAML_TY_PRIMITIVE_FLOAT,
    string: TyPrimitiveKind.BAML_TY_PRIMITIVE_STRING,
    bool: TyPrimitiveKind.BAML_TY_PRIMITIVE_BOOL,
    null: TyPrimitiveKind.BAML_TY_PRIMITIVE_NULL,
    bytes: TyPrimitiveKind.BAML_TY_PRIMITIVE_BYTES,
    bigint: TyPrimitiveKind.BAML_TY_PRIMITIVE_BIGINT,
};
const MEDIA_CTOR_KIND = new Map([
    [BamlImage, TyMediaKind.BAML_TY_MEDIA_KIND_IMAGE],
    [BamlAudio, TyMediaKind.BAML_TY_MEDIA_KIND_AUDIO],
    [BamlVideo, TyMediaKind.BAML_TY_MEDIA_KIND_VIDEO],
    [BamlPdf, TyMediaKind.BAML_TY_MEDIA_KIND_PDF],
]);
const TY_MEDIA_CTOR = {
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
export function lowerTypeToWireTy(token) {
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
        const kind = PRIMITIVE_KIND[token];
        if (kind !== undefined) {
            return { primitive: { kind } };
        }
        return { unknown: {} };
    }
    // Media constructors are functions too, so dispatch before the generic
    // codegen-class path.
    if (typeof token === 'function') {
        const mediaKind = MEDIA_CTOR_KIND.get(token);
        if (mediaKind !== undefined) {
            return { media: { kind: mediaKind } };
        }
        // A bare class constructor → that class, no concrete args.
        return classWireTy(token, []);
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
            if (!token.typeAlias)
                return { unknown: {} };
            try {
                getTypeMap().getTypeAlias(token.typeAlias);
            }
            catch {
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
const TY_PRIMITIVE_TOKEN = {
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_INT]: 'int',
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_FLOAT]: 'float',
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_STRING]: 'string',
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_BOOL]: 'bool',
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_NULL]: 'null',
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_BYTES]: 'bytes',
    [TyPrimitiveKind.BAML_TY_PRIMITIVE_BIGINT]: 'bigint',
};
function literalWireTy(token) {
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
function outboundLiteral(literal) {
    const oneof = literal.literal;
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
        }
        catch {
            return undefined;
        }
    }
    if (oneof === 'floatValue' || (oneof === undefined && literal.floatValue != null)) {
        return { kind: 'float', value: literal.floatValue ?? '' };
    }
    return undefined;
}
function isTypeAliasToken(value) {
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
export function outboundTyToBamlType(ty) {
    if (!ty)
        return undefined;
    if (ty.primitive) {
        return TY_PRIMITIVE_TOKEN[ty.primitive.kind ?? -1] ?? undefined;
    }
    if (ty.list)
        return { list: outboundTyToBamlType(ty.list.item) };
    if (ty.map) {
        return { map: [outboundTyToBamlType(ty.map.key), outboundTyToBamlType(ty.map.value)] };
    }
    if (ty.optional)
        return { optional: outboundTyToBamlType(ty.optional.inner) };
    if (ty.union) {
        return { union: (ty.union.options ?? []).map(outboundTyToBamlType) };
    }
    if (ty.classTy) {
        const fqn = ty.classTy.name ?? '';
        const args = (ty.classTy.typeArgs ?? []).map((a) => outboundTyToBamlType(a));
        let ctor;
        try {
            ctor = getTypeMap().getClass(fqn);
        }
        catch {
            ctor = undefined; // unmapped FQN — leave unbound
        }
        if (!ctor)
            return undefined;
        return args.length ? { class: ctor, args } : ctor;
    }
    if (ty.enum) {
        const fqn = ty.enum.name ?? '';
        try {
            const enumObject = getTypeMap().getEnum(fqn);
            return typeof enumObject === 'object' && enumObject !== null
                ? { enum: enumObject }
                : undefined;
        }
        catch {
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
            if (!isTypeAliasToken(alias))
                return undefined;
            return args.length ? { ...alias, args } : alias;
        }
        catch {
            return undefined;
        }
    }
    if (ty.media) {
        return TY_MEDIA_CTOR[ty.media.kind ?? -1] ?? undefined;
    }
    if (ty.never)
        return Never;
    // type_var / interface / function / opaque/runtime-only / unknown / any
    // other unrepresentable type → unbound wildcard.
    return undefined;
}
/** Build a `class_ty` wire `Ty` for a codegen class constructor and its
 * concrete generic args. The FQN comes from the typemap reverse map; an
 * unmapped constructor lowers to unknown so it can't manufacture a bogus
 * class reference. */
function classWireTy(ctor, args) {
    const fqn = getTypeMap().jsTypeToBamlType(ctor);
    if (!fqn) {
        return { unknown: {} };
    }
    return { classTy: { name: fqn, typeArgs: args.map(lowerTypeToWireTy) } };
}
//# sourceMappingURL=wire_ty.js.map