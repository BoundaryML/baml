/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
// wire_ty.ts — lower a host-supplied type token to a wire `Ty` (baml_type.proto).
//
// The Node analog of `python_type_to_wire_ty` / `_fill_wire_ty` in
// sdks/python/src/baml_core/proto.py. Python recovers a generic instance's
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
import { baml_core } from './proto/baml_cffi.js';
import { getTypeMap } from './typemap.js';
const TyPrimitiveKind = baml_core.cffi.v1.TyPrimitiveKind;
/**
 * The bottom type (BAML `never`). Pass as a `$types` binding to bind a TypeVar
 * to `never`, mirroring Python's `_types={"T": Never}`.
 */
export const Never = Symbol('baml.Never');
const PRIMITIVE_KIND = {
    int: TyPrimitiveKind.TY_PRIMITIVE_INT,
    float: TyPrimitiveKind.TY_PRIMITIVE_FLOAT,
    string: TyPrimitiveKind.TY_PRIMITIVE_STRING,
    bool: TyPrimitiveKind.TY_PRIMITIVE_BOOL,
    null: TyPrimitiveKind.TY_PRIMITIVE_NULL,
    bytes: TyPrimitiveKind.TY_PRIMITIVE_BYTES,
    bigint: TyPrimitiveKind.TY_PRIMITIVE_BIGINT,
};
/**
 * Lower a {@link BamlType} token to a wire `Ty` (an `ITy` plain object the
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
/**
 * Decode an outbound `BamlTy` (baml_outbound.proto) back to a {@link BamlType}
 * token — the reverse of {@link lowerTypeToWireTy}, used to repopulate a generic
 * instance's `$types` field on decode. Mirrors the BAML→Python type projection
 * (`_baml_ty_to_python_type`) that drives Python's `cls[args]` reparameterize.
 * Unrecognized / dynamic positions (`any`/`unknown`/enum/union/literal/media)
 * decode to `undefined`, i.e. an unbound wildcard.
 */
export function outboundTyToBamlType(ty) {
    if (!ty)
        return undefined;
    if (ty.stringType)
        return 'string';
    if (ty.intType)
        return 'int';
    if (ty.floatType)
        return 'float';
    if (ty.boolType)
        return 'bool';
    if (ty.nullType)
        return 'null';
    if (ty.bigintType)
        return 'bigint';
    if (ty.uint8arrayType)
        return 'bytes';
    if (ty.listType)
        return { list: outboundTyToBamlType(ty.listType.itemType) };
    if (ty.mapType) {
        return { map: [outboundTyToBamlType(ty.mapType.keyType), outboundTyToBamlType(ty.mapType.valueType)] };
    }
    if (ty.optionalType)
        return { optional: outboundTyToBamlType(ty.optionalType.value) };
    if (ty.classType) {
        const fqn = ty.classType.name?.name ?? '';
        const args = (ty.classType.name?.genericArgs ?? []).map((a) => outboundTyToBamlType(a.ty));
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
    // enum / union variant / literal / media / any / unknown → unbound wildcard.
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