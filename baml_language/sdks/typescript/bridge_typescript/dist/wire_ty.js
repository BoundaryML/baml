// Host type tokens and opaque BEP-066 reflected type definitions.
import { baml_bridge } from './proto/baml_cffi.js';
import { getTypeMap } from './typemap.js';
const TyPrimitiveKind = baml_bridge.cffi.v1.BamlTyPrimitiveKind;
const BamlTyDefMessage = baml_bridge.cffi.v1.BamlTyDef;
/** The bottom type (BAML `never`). */
export const Never = Symbol('baml.Never');
const typeEvidence = Symbol('baml.type.evidence');
const nativeTypeEvidence = Symbol('baml.type.native');
/** @internal Not exported by the public package; only SDK factories mint evidence. */
export const _typeConstruction = Symbol('baml.type.construction');
export function isBamlType(value) {
    return value instanceof BamlType;
}
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
/** @internal Combine child graphs without dropping definitions or reassigning witnesses. */
export function _composeTypeDefinition(children, root) {
    const classes = new Map();
    const enums = new Map();
    const merge = (table, values, encode) => {
        for (const value of values) {
            const name = value.name ?? '';
            const previous = table.get(name);
            if (previous) {
                const a = encode(previous), b = encode(value);
                if (a.length !== b.length || a.some((byte, i) => byte !== b[i])) {
                    throw new TypeError(`conflicting type definitions for ${name}`);
                }
            }
            table.set(name, value);
        }
    };
    const types = children.map(value => {
        if (!isBamlType(value))
            throw new TypeError('type arguments must be BamlType values');
        const definition = value._wireCopy();
        if (!definition.root)
            throw new TypeError('type definition has no root');
        if (definition.witnesses.length)
            throw new TypeError('embedding a type with root-scoped conformance witnesses is not supported yet');
        merge(classes, definition.classes, value => baml_bridge.cffi.v1.BamlClassDef.encode(value).finish());
        merge(enums, definition.enums, value => baml_bridge.cffi.v1.BamlEnumDef.encode(value).finish());
        return definition.root;
    });
    return { root: root(types), classes: [...classes.values()], enums: [...enums.values()] };
}
/**
 * Opaque, process-local handle for a reflected BAML definition. Only the
 * composing operations required by H-11 are public. Each wire occurrence
 * carries a copied definition graph; JavaScript identity is never type
 * identity.
 */
export class BamlType {
    #definition;
    #checkSdk;
    constructor(key, definition, checkSdk = () => { }) {
        if (key !== _typeConstruction)
            throw new TypeError('BAML type evidence is created by SDK factories');
        this.#definition = cloneDefinition(definition);
        this.#checkSdk = checkSdk;
    }
    /** @internal Bridge/codegen hook; not a host inspection surface. */
    static _fromWire(definition) {
        return new BamlType(_typeConstruction, definition);
    }
    /** @internal Generated SDK factories supply the native type and declaration arity. */
    static _declared(typeMap, kind, name, args) {
        // Resolve once at creation. A name alone does not identify a host codec.
        if (kind === 'class')
            typeMap.getClass(name);
        else
            typeMap.getEnum(name);
        const children = [...args];
        const definition = _composeTypeDefinition(children, typeArgs => kind === 'class'
            ? { classTy: { name, typeArgs } } : { enum: { name } });
        const check = (map) => {
            if (map !== typeMap)
                throw new TypeError('declared type token does not belong to this SDK');
            for (const child of children)
                child._checkSdk(map);
        };
        check(typeMap);
        return new BamlType(_typeConstruction, definition, check);
    }
    /** @internal Bridge hook returning a fresh protobuf graph. */
    _wireCopy() {
        return cloneDefinition(this.#definition);
    }
    /** @internal Generated declaration identity survives composition. */
    _checkSdk(typeMap) { this.#checkSdk(typeMap); }
    static from(token) {
        if (isBamlType(token))
            return token;
        const typeMap = getTypeMap();
        const root = lowerTypeToWireTy(token, typeMap);
        if (root.classTy && typeof token === 'function') {
            const name = root.classTy.name ?? '';
            const check = (map) => {
                if (map.getClass(name) !== token)
                    throw new TypeError('class type token does not belong to this SDK');
            };
            check(typeMap);
            return new BamlType(_typeConstruction, { root }, check);
        }
        return new BamlType(_typeConstruction, { root });
    }
    meta(options = {}) {
        return new BamlTypeMetadataRow(this, options.alias ?? null, options.description ?? null, options.docstring ?? null, { ...(options.other ?? {}) });
    }
    array() {
        const definition = this._wireCopy();
        if (definition.witnesses.length)
            throw new TypeError('cannot move root-scoped conformance witnesses into an array type');
        definition.root = { list: { item: definition.root } };
        return new BamlType(_typeConstruction, definition, this.#checkSdk);
    }
    optional() {
        const definition = this._wireCopy();
        if (definition.witnesses.length)
            throw new TypeError('cannot move root-scoped conformance witnesses into an optional type');
        definition.root = { optional: { inner: definition.root } };
        return new BamlType(_typeConstruction, definition, this.#checkSdk);
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
    of: BamlType.from,
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
export function lowerTypeToWireTy(token, typeMap = getTypeMap()) {
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
        return namedWireTy(token, [], typeMap);
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
            return namedWireTy(shape.class, args, typeMap);
        }
        if ('list' in shape)
            return { list: { item: lowerTypeToWireTy(shape.list, typeMap) } };
        if ('map' in shape) {
            if (!Array.isArray(shape.map) || shape.map.length !== 2)
                return unsupported(token);
            const [key, value] = shape.map;
            return { map: { key: lowerTypeToWireTy(key, typeMap), value: lowerTypeToWireTy(value, typeMap) } };
        }
        if ('optional' in shape)
            return { optional: { inner: lowerTypeToWireTy(shape.optional, typeMap) } };
        if ('union' in shape) {
            if (!Array.isArray(shape.union))
                return unsupported(token);
            return { union: { options: shape.union.map(value => lowerTypeToWireTy(value, typeMap)) } };
        }
        const fqn = typeMap.jsTypeToBamlType(token);
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
export function outboundTyToBamlTypeToken(ty, typeMap = getTypeMap()) {
    if (!ty)
        return undefined;
    if (ty.primitive)
        return TY_PRIMITIVE_TOKEN[ty.primitive.kind ?? -1];
    if (ty.list) {
        const item = outboundTyToBamlTypeToken(ty.list.item, typeMap);
        return item === undefined ? undefined : { list: item };
    }
    if (ty.map) {
        const key = outboundTyToBamlTypeToken(ty.map.key, typeMap);
        const value = outboundTyToBamlTypeToken(ty.map.value, typeMap);
        return key === undefined || value === undefined ? undefined : { map: [key, value] };
    }
    if (ty.optional) {
        const inner = outboundTyToBamlTypeToken(ty.optional.inner, typeMap);
        return inner === undefined ? undefined : { optional: inner };
    }
    if (ty.classTy) {
        const token = resolveNamedToken(ty.classTy.name ?? '', typeMap);
        if (typeof token !== 'function')
            return undefined;
        const args = (ty.classTy.typeArgs ?? []).map(value => outboundTyToBamlTypeToken(value, typeMap));
        if (args.some((arg) => arg === undefined))
            return undefined;
        return args.length ? { class: token, args: args } : token;
    }
    if (ty.enum)
        return resolveNamedToken(ty.enum.name ?? '', typeMap);
    return undefined;
}
function resolveNamedToken(fqn, typeMap) {
    try {
        return typeMap.getClass(fqn);
    }
    catch {
        try {
            return typeMap.getEnum(fqn);
        }
        catch {
            return undefined;
        }
    }
}
function namedWireTy(token, args, typeMap) {
    const fqn = typeMap.jsTypeToBamlType(token);
    if (!fqn)
        return unsupported(token);
    return { classTy: { name: fqn, typeArgs: args.map(value => lowerTypeToWireTy(value, typeMap)) } };
}
//# sourceMappingURL=wire_ty.js.map