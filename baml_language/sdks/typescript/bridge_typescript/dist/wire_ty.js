/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// Host type tokens and opaque BEP-066 reflected type definitions.
import { BamlHandle } from './native.js';
import { baml_bridge } from './proto/baml_cffi.js';
import { getTypeMap } from './typemap.js';
const TyPrimitiveKind = baml_bridge.cffi.v1.BamlTyPrimitiveKind;
const BamlTyMessage = baml_bridge.cffi.v1.BamlTy;
/** The bottom type (BAML `never`). */
export const Never = Symbol('baml.Never');
/** Internal row consumed by the generated `reflect.Class.new` binding. */
export class BamlTypeMetadataRow {
    ty;
    alias;
    description;
    docstring;
    other;
    constructor(ty, alias, description, docstring, other = {}) {
        this.ty = ty;
        this.alias = alias;
        this.description = description;
        this.docstring = docstring;
        this.other = other;
    }
}
function cloneTy(ty) {
    const message = BamlTyMessage.fromObject(ty);
    return BamlTyMessage.decode(BamlTyMessage.encode(message).finish());
}
/**
 * Every `live` head under a wire type, once per occurrence: each is a row of
 * the engine's handle table naming a runtime declaration.
 */
export function liveKeys(ty) {
    const keys = [];
    const visit = (node) => {
        if (!node)
            return;
        const head = (nominal) => {
            if (nominal?.live != null)
                keys.push(nominal.live);
        };
        head(node.classTy);
        head(node.enum);
        head(node.typeAlias);
        head(node.interface);
        head(node.enumVariant);
        node.classTy?.typeArgs?.forEach(visit);
        node.typeAlias?.typeArgs?.forEach(visit);
        node.interface?.typeArgs?.forEach(visit);
        node.interface?.bindings?.forEach((binding) => visit(binding.ty));
        visit(node.list?.item);
        visit(node.map?.key);
        visit(node.map?.value);
        visit(node.optional?.inner);
        node.union?.options?.forEach(visit);
        node.function?.params?.forEach((param) => visit(param.ty));
        visit(node.function?.ret);
        visit(node.function?.throws);
        visit(node.future?.value);
        visit(node.future?.error);
        visit(node.associatedTypeProjection?.base);
        visit(node.associatedTypeProjection?.interface);
    };
    visit(ty);
    return keys;
}
/**
 * The engine rows a type value's runtime declarations are held by, one per
 * occurrence. Shared by the types composed from one value; each row is
 * released when its handle is collected.
 */
class LiveRows {
    handles;
    constructor(keys) {
        this.handles = keys.map((key) => new BamlHandle(key, UNTAGGED_BEX_HEAP));
    }
}
const UNTAGGED_BEX_HEAP = baml_bridge.cffi.v1.BamlHandleType.UNTAGGED_BEX_HEAP;
/**
 * Opaque, process-local handle for a reflected BAML type. Only the composing
 * operations required by H-11 are public. A runtime declaration the type
 * names is held by the engine row it arrived under; a type sent back states
 * it with a reference of the call's own. JavaScript identity is never type
 * identity.
 */
export class BamlType {
    #ty;
    #rows;
    constructor(ty, rows) {
        this.#ty = cloneTy(ty);
        this.#rows = rows;
    }
    /** @internal A type the engine handed over: its `live` heads are rows this value now holds. */
    static _fromWire(ty) {
        return new BamlType(ty, new LiveRows(liveKeys(ty)));
    }
    /** @internal Bridge hook returning a fresh protobuf tree. */
    _wireCopy() {
        return cloneTy(this.#ty);
    }
    static from(token) {
        return token instanceof BamlType
            ? token
            : new BamlType(lowerTypeToWireTy(token), undefined);
    }
    meta(options = {}) {
        return new BamlTypeMetadataRow(this, options.alias ?? null, options.description ?? null, options.docstring ?? null, { ...(options.other ?? {}) });
    }
    array() {
        return new BamlType({ list: { item: this.#ty } }, this.#rows);
    }
    optional() {
        return new BamlType({ optional: { inner: this.#ty } }, this.#rows);
    }
    toJSON() {
        throw new TypeError('BamlType values are runtime handles and cannot be serialized');
    }
    toString() {
        return 'BamlType(<opaque>)';
    }
}
/** Runtime member installed as generated `reflect.Type`. */
export const reflectType = Object.freeze({
    of(token) {
        return BamlType.from(token);
    },
});
const PRIMITIVE_KIND = {
    int: TyPrimitiveKind.BAML_TY_PRIMITIVE_INT,
    float: TyPrimitiveKind.BAML_TY_PRIMITIVE_FLOAT,
    string: TyPrimitiveKind.BAML_TY_PRIMITIVE_STRING,
    bool: TyPrimitiveKind.BAML_TY_PRIMITIVE_BOOL,
    null: TyPrimitiveKind.BAML_TY_PRIMITIVE_NULL,
    bytes: TyPrimitiveKind.BAML_TY_PRIMITIVE_BYTES,
    bigint: TyPrimitiveKind.BAML_TY_PRIMITIVE_BIGINT,
};
function unsupported(token) {
    const rendered = typeof token === 'function'
        ? token.name || '<anonymous constructor>'
        : Object.prototype.toString.call(token);
    throw new TypeError(`unsupported TypeScript type token: ${rendered}`);
}
/** Lower a statically-known token to a sparse wire `BamlTy`. */
export function lowerTypeToWireTy(token) {
    if (token === Never)
        return { never: {} };
    if (token === String)
        return { primitive: { kind: PRIMITIVE_KIND.string } };
    if (token === Boolean)
        return { primitive: { kind: PRIMITIVE_KIND.bool } };
    if (token === BigInt)
        return { primitive: { kind: PRIMITIVE_KIND.bigint } };
    if (token === Uint8Array)
        return { primitive: { kind: PRIMITIVE_KIND.bytes } };
    if (typeof token === 'string') {
        const kind = PRIMITIVE_KIND[token];
        if (kind !== undefined)
            return { primitive: { kind } };
        return unsupported(token);
    }
    if (typeof token === 'function') {
        return namedWireTy(token, []);
    }
    if (token !== null && typeof token === 'object') {
        const shape = token;
        if ('__baml_interface_fqn__' in shape) {
            const name = shape.__baml_interface_fqn__;
            if (typeof name !== 'string' || !name)
                return unsupported(token);
            return { interface: { name } };
        }
        if ('class' in shape) {
            const args = shape.args === undefined ? [] : shape.args;
            if (!Array.isArray(args))
                return unsupported(token);
            return namedWireTy(shape.class, args);
        }
        if ('list' in shape)
            return { list: { item: lowerTypeToWireTy(shape.list) } };
        if ('map' in shape) {
            if (!Array.isArray(shape.map) || shape.map.length !== 2)
                return unsupported(token);
            const [key, value] = shape.map;
            return { map: { key: lowerTypeToWireTy(key), value: lowerTypeToWireTy(value) } };
        }
        if ('optional' in shape)
            return { optional: { inner: lowerTypeToWireTy(shape.optional) } };
        if ('union' in shape) {
            if (!Array.isArray(shape.union))
                return unsupported(token);
            return { union: { options: shape.union.map(lowerTypeToWireTy) } };
        }
        const fqn = getTypeMap().jsTypeToBamlType(token);
        if (fqn)
            return { enum: { name: fqn } };
    }
    return unsupported(token);
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
/** Decode the sparse value-level type channel on generated class instances. */
export function outboundTyToBamlTypeToken(ty) {
    if (!ty)
        return undefined;
    if (ty.primitive)
        return TY_PRIMITIVE_TOKEN[ty.primitive.kind ?? -1];
    if (ty.list) {
        const item = outboundTyToBamlTypeToken(ty.list.item);
        return item === undefined ? undefined : { list: item };
    }
    if (ty.map) {
        const key = outboundTyToBamlTypeToken(ty.map.key);
        const value = outboundTyToBamlTypeToken(ty.map.value);
        return key === undefined || value === undefined ? undefined : { map: [key, value] };
    }
    if (ty.optional) {
        const inner = outboundTyToBamlTypeToken(ty.optional.inner);
        return inner === undefined ? undefined : { optional: inner };
    }
    if (ty.classTy) {
        const token = resolveNamedToken(ty.classTy.name ?? '');
        if (typeof token !== 'function')
            return undefined;
        const args = (ty.classTy.typeArgs ?? []).map(outboundTyToBamlTypeToken);
        if (args.some((arg) => arg === undefined))
            return undefined;
        return args.length ? { class: token, args: args } : token;
    }
    if (ty.enum)
        return resolveNamedToken(ty.enum.name ?? '');
    return undefined;
}
function resolveNamedToken(fqn) {
    try {
        return getTypeMap().getClass(fqn);
    }
    catch {
        try {
            return getTypeMap().getEnum(fqn);
        }
        catch {
            return undefined;
        }
    }
}
function namedWireTy(token, args) {
    const fqn = getTypeMap().jsTypeToBamlType(token);
    if (!fqn)
        return unsupported(token);
    return { classTy: { name: fqn, typeArgs: args.map(lowerTypeToWireTy) } };
}
//# sourceMappingURL=wire_ty.js.map