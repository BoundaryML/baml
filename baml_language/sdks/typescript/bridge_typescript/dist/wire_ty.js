/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// Host type tokens and opaque BEP-066 reflected type definitions.
import { baml_bridge } from './proto/baml_cffi.js';
import { getTypeMap } from './typemap.js';
const TyPrimitiveKind = baml_bridge.cffi.v1.BamlTyPrimitiveKind;
const BamlTyDefMessage = baml_bridge.cffi.v1.BamlTyDef;
/** The bottom type (BAML `never`). */
export const Never = Symbol('baml.Never');
/** Internal row consumed by the generated `reflect.class.new` binding. */
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
function cloneDefinition(definition) {
    const message = BamlTyDefMessage.fromObject(definition);
    return BamlTyDefMessage.decode(BamlTyDefMessage.encode(message).finish());
}
/**
 * Opaque, process-local handle for a reflected BAML definition. Only the
 * composing operations required by H-11 are public. Each wire occurrence
 * carries a copied definition graph; JavaScript identity is never type
 * identity.
 */
export class BamlType {
    #definition;
    constructor(definition) {
        this.#definition = cloneDefinition(definition);
    }
    /** @internal Bridge/codegen hook; not a host inspection surface. */
    static _fromWire(definition) {
        return new BamlType(definition);
    }
    /** @internal Bridge hook returning a fresh protobuf graph. */
    _wireCopy() {
        return cloneDefinition(this.#definition);
    }
    static from(token) {
        return token instanceof BamlType
            ? token
            : new BamlType({ root: lowerTypeToWireTy(token) });
    }
    meta(options = {}) {
        return new BamlTypeMetadataRow(this, options.alias ?? null, options.description ?? null, options.docstring ?? null, { ...(options.other ?? {}) });
    }
    array() {
        const definition = this._wireCopy();
        definition.root = { list: { item: definition.root } };
        return new BamlType(definition);
    }
    optional() {
        const definition = this._wireCopy();
        definition.root = { optional: { inner: definition.root } };
        return new BamlType(definition);
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