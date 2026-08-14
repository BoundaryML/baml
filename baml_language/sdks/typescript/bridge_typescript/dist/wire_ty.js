/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
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
import { getTypeMap } from './typemap.js';
const TyPrimitiveKind = baml_bridge.cffi.v1.BamlTyPrimitiveKind;
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
    // A bare class constructor → that class, no concrete args.
    if (typeof token === 'function') {
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
/**
 * Decode a wire `Ty` (baml_type.proto) back to a {@link BamlType} token — the
 * exact inverse of {@link lowerTypeToWireTy}, used to repopulate a generic
 * instance's `$types` field on decode. Mirrors the engine's
 * `ty_encode::runtime_ty_to_proto_ty` and Python's `_ty_to_python_type`.
 * Positions with no concrete JS binding (a structural union, an enum, a type
 * variable, an opaque/runtime-only type) decode to `undefined`, i.e. an unbound
 * wildcard.
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
    // union / enum / literal / media / type_var / unknown / any other →
    // unbound wildcard (a structural union is unbound for `class<args>`).
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