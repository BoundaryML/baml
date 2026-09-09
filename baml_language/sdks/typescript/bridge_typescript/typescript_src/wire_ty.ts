// Host type tokens and opaque BEP-066 reflected type definitions.

import { baml_bridge } from './proto/baml_cffi.js';
import { BamlTypeMap, getTypeMap } from './typemap.js';

const TyPrimitiveKind = baml_bridge.cffi.v1.BamlTyPrimitiveKind;
const BamlTyDefMessage = baml_bridge.cffi.v1.BamlTyDef;

/** The bottom type (BAML `never`). */
export const Never: unique symbol = Symbol('baml.Never');

export type BamlPrimitiveToken =
    | 'int'
    | 'float'
    | 'string'
    | 'bool'
    | 'null'
    | 'bytes'
    | 'bigint';

/** A generated class value, including live classes with protected constructors.
 * Membership is checked through the issuing SDK's constructor typemap. */
export type BamlClassCtor = Function & { readonly prototype: object };

/** A codegen-emitted erased interface token. */
export type BamlInterfaceToken = {
    readonly __baml_interface_fqn__: string;
};

/**
 * A runtime spelling of a statically-known BAML type. TypeScript erases its
 * type grammar, so recursive containers use small data constructors and
 * generated classes/enums are passed as their emitted runtime values.
 */
export type BamlTypeToken =
    | BamlPrimitiveToken
    | StringConstructor
    | BooleanConstructor
    | BigIntConstructor
    | Uint8ArrayConstructor
    | typeof Never
    | BamlClassCtor
    | BamlInterfaceToken
    | Record<string, string>
    | { class: BamlClassCtor; args?: BamlTypeToken[] }
    | { list: BamlTypeToken }
    | { map: [BamlTypeToken, BamlTypeToken] }
    | { optional: BamlTypeToken }
    | { union: BamlTypeToken[] };

const typeEvidence: unique symbol = Symbol('baml.type.evidence');
const nativeTypeEvidence: unique symbol = Symbol('baml.type.native');
/** @internal Not exported by the public package; only SDK factories mint evidence. */
export const _typeConstruction: unique symbol = Symbol('baml.type.construction');

/** An opaque type when its native result type is not statically known. */
export interface BamlTypeValue {
    readonly [typeEvidence]: true;
    _wireCopy(): baml_bridge.cffi.v1.BamlTyDef;
    _checkSdk(typeMap: BamlTypeMap): void;
}

export function isBamlType(value: unknown): value is BamlTypeValue {
    return value instanceof BamlType;
}

/** Native projection of the primitive tokens whose identity is unambiguous. */
export type BamlPrimitiveValue<T> =
    T extends 'int' | 'float' ? number :
    T extends 'string' | StringConstructor ? string :
    T extends 'bool' | BooleanConstructor ? boolean :
    T extends 'bigint' | BigIntConstructor ? bigint :
    T extends 'bytes' | Uint8ArrayConstructor ? Uint8Array :
    T extends 'null' ? null :
    T extends typeof Never ? never : unknown;

/** Keep inference tied to explicit type evidence, even in generated namespaces. */
export type BamlNoInfer<T> = NoInfer<T>;

type ExactPrimitiveToken = BamlPrimitiveToken | StringConstructor | BooleanConstructor
    | BigIntConstructor | Uint8ArrayConstructor | typeof Never;

export interface BamlTypeMetadata {
    alias?: string;
    description?: string;
    docstring?: string;
    other?: Record<string, string>;
}

/** Internal row consumed by the generated `reflect.class.new` binding. */
export class BamlTypeMetadataRow {
    constructor(
        readonly ty: BamlTypeValue,
        readonly alias: string | null,
        readonly description: string | null,
        readonly docstring: string | null,
        readonly other: Record<string, string> = {},
    ) {}
}

function cloneDefinition(
    definition: baml_bridge.cffi.v1.IBamlTyDef,
): baml_bridge.cffi.v1.BamlTyDef {
    const message = BamlTyDefMessage.fromObject(definition as Record<string, unknown>);
    return BamlTyDefMessage.decode(BamlTyDefMessage.encode(message).finish());
}

/** @internal Combine child graphs without dropping definitions or reassigning witnesses. */
export function _composeTypeDefinition(
    children: readonly BamlTypeValue[],
    root: (types: baml_bridge.cffi.v1.IBamlTy[]) => baml_bridge.cffi.v1.IBamlTy,
): baml_bridge.cffi.v1.IBamlTyDef {
    const classes = new Map<string, baml_bridge.cffi.v1.IBamlClassDef>();
    const enums = new Map<string, baml_bridge.cffi.v1.IBamlEnumDef>();
    const merge = <T extends { name?: string | null }>(table: Map<string, T>, values: readonly T[], encode: (value: T) => Uint8Array) => {
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
        if (!isBamlType(value)) throw new TypeError('type arguments must be BamlType values');
        const definition = value._wireCopy();
        if (!definition.root) throw new TypeError('type definition has no root');
        if (definition.witnesses.length) throw new TypeError('embedding a type with root-scoped conformance witnesses is not supported yet');
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
export class BamlType<T = unknown> implements BamlTypeValue {
    declare readonly [typeEvidence]: true;
    // Invariant: an annotation must not silently change an associated binding.
    declare readonly [nativeTypeEvidence]: (value: T) => T;
    readonly #definition: baml_bridge.cffi.v1.BamlTyDef;
    readonly #checkSdk: (typeMap: BamlTypeMap) => void;

    protected constructor(key: typeof _typeConstruction, definition: baml_bridge.cffi.v1.IBamlTyDef, checkSdk: (typeMap: BamlTypeMap) => void = () => {}) {
        if (key !== _typeConstruction) throw new TypeError('BAML type evidence is created by SDK factories');
        this.#definition = cloneDefinition(definition);
        this.#checkSdk = checkSdk;
    }

    /** @internal Bridge/codegen hook; not a host inspection surface. */
    static _fromWire(definition: baml_bridge.cffi.v1.IBamlTyDef): BamlType {
        return new BamlType(_typeConstruction, definition);
    }

    /** @internal Generated SDK factories supply the native type and declaration arity. */
    static _declared<T>(typeMap: BamlTypeMap, kind: 'class' | 'enum', name: string, args: readonly BamlTypeValue[]): BamlType<T> {
        // Resolve once at creation. A name alone does not identify a host codec.
        if (kind === 'class') typeMap.getClass(name); else typeMap.getEnum(name);
        const children = [...args];
        const definition = _composeTypeDefinition(children, typeArgs => kind === 'class'
            ? { classTy: { name, typeArgs } } : { enum: { name } });
        const check = (map: BamlTypeMap) => {
            if (map !== typeMap) throw new TypeError('declared type token does not belong to this SDK');
            for (const child of children) child._checkSdk(map);
        };
        check(typeMap);
        return new BamlType<T>(_typeConstruction, definition, check);
    }

    /** @internal Bridge hook returning a fresh protobuf graph. */
    _wireCopy(): baml_bridge.cffi.v1.BamlTyDef {
        return cloneDefinition(this.#definition);
    }

    /** @internal Generated declaration identity survives composition. */
    _checkSdk(typeMap: BamlTypeMap): void { this.#checkSdk(typeMap); }

    static from<T>(token: BamlType<T>): BamlType<T>;
    static from<const T extends ExactPrimitiveToken>(token: T): BamlType<BamlPrimitiveValue<T>>;
    static from<C extends abstract new (...args: never[]) => object>(token: C): BamlType<InstanceType<C>>;
    static from(token: BamlTypeValue | BamlTypeToken): BamlType<unknown>;
    static from(token: BamlTypeValue | BamlTypeToken): BamlTypeValue {
        if (isBamlType(token)) return token;
        const typeMap = getTypeMap();
        const root = lowerTypeToWireTy(token, typeMap);
        if (root.classTy && typeof token === 'function') {
            const name = root.classTy.name ?? '';
            const check = (map: BamlTypeMap) => {
                if (map.getClass(name) !== token) throw new TypeError('class type token does not belong to this SDK');
            };
            check(typeMap);
            return new BamlType(_typeConstruction, { root }, check);
        }
        return new BamlType(_typeConstruction, { root });
    }

    meta(options: BamlTypeMetadata = {}): BamlTypeMetadataRow {
        return new BamlTypeMetadataRow(
            this,
            options.alias ?? null,
            options.description ?? null,
            options.docstring ?? null,
            { ...(options.other ?? {}) },
        );
    }

    array(): BamlType<T[]> {
        const definition = this._wireCopy();
        if (definition.witnesses.length) throw new TypeError('cannot move root-scoped conformance witnesses into an array type');
        definition.root = { list: { item: definition.root } };
        return new BamlType(_typeConstruction, definition, this.#checkSdk);
    }

    optional(): BamlType<T | null> {
        const definition = this._wireCopy();
        if (definition.witnesses.length) throw new TypeError('cannot move root-scoped conformance witnesses into an optional type');
        definition.root = { optional: { inner: definition.root } };
        return new BamlType(_typeConstruction, definition, this.#checkSdk);
    }

    toJSON(): never {
        throw new TypeError('BamlType values are runtime handles and cannot be serialized');
    }

    toString(): string {
        return 'BamlType(<opaque>)';
    }
}

/** Runtime member installed as generated `reflect.Type`. */
export const reflectType = Object.freeze({
    of: BamlType.from,
});

const PRIMITIVE_KIND: Record<BamlPrimitiveToken, number> = {
    int: TyPrimitiveKind.BAML_TY_PRIMITIVE_INT,
    float: TyPrimitiveKind.BAML_TY_PRIMITIVE_FLOAT,
    string: TyPrimitiveKind.BAML_TY_PRIMITIVE_STRING,
    bool: TyPrimitiveKind.BAML_TY_PRIMITIVE_BOOL,
    null: TyPrimitiveKind.BAML_TY_PRIMITIVE_NULL,
    bytes: TyPrimitiveKind.BAML_TY_PRIMITIVE_BYTES,
    bigint: TyPrimitiveKind.BAML_TY_PRIMITIVE_BIGINT,
};

function unsupported(token: unknown): never {
    const rendered = typeof token === 'function'
        ? token.name || '<anonymous constructor>'
        : Object.prototype.toString.call(token);
    throw new TypeError(`unsupported TypeScript type token: ${rendered}`);
}

/** Lower a statically-known token to a sparse wire `BamlTy`. */
export function lowerTypeToWireTy(token: BamlTypeToken, typeMap: BamlTypeMap = getTypeMap()): baml_bridge.cffi.v1.IBamlTy {
    if (token === Never) return { never: {} };
    if (token === String) return { primitive: { kind: PRIMITIVE_KIND.string } };
    if (token === Boolean) return { primitive: { kind: PRIMITIVE_KIND.bool } };
    if (token === BigInt) return { primitive: { kind: PRIMITIVE_KIND.bigint } };
    if (token === Uint8Array) return { primitive: { kind: PRIMITIVE_KIND.bytes } };
    if (typeof token === 'string') {
        const kind = PRIMITIVE_KIND[token as BamlPrimitiveToken];
        if (kind !== undefined) return { primitive: { kind } };
        return unsupported(token);
    }
    if (typeof token === 'function') {
        return namedWireTy(token, [], typeMap);
    }
    if (token !== null && typeof token === 'object') {
        const shape = token as Record<string, unknown>;
        if ('__baml_interface_fqn__' in shape) {
            const name = shape.__baml_interface_fqn__;
            if (typeof name !== 'string' || !name) return unsupported(token);
            return { interface: { name } };
        }
        if ('class' in shape) {
            const args = shape.args === undefined ? [] : shape.args;
            if (!Array.isArray(args)) return unsupported(token);
            return namedWireTy(shape.class, args as BamlTypeToken[], typeMap);
        }
        if ('list' in shape) return { list: { item: lowerTypeToWireTy(shape.list as BamlTypeToken, typeMap) } };
        if ('map' in shape) {
            if (!Array.isArray(shape.map) || shape.map.length !== 2) return unsupported(token);
            const [key, value] = shape.map as BamlTypeToken[];
            return { map: { key: lowerTypeToWireTy(key, typeMap), value: lowerTypeToWireTy(value, typeMap) } };
        }
        if ('optional' in shape) return { optional: { inner: lowerTypeToWireTy(shape.optional as BamlTypeToken, typeMap) } };
        if ('union' in shape) {
            if (!Array.isArray(shape.union)) return unsupported(token);
            return { union: { options: (shape.union as BamlTypeToken[]).map(value => lowerTypeToWireTy(value, typeMap)) } };
        }
        const fqn = typeMap.jsTypeToBamlType(token);
        if (fqn) return { enum: { name: fqn } };
    }
    return unsupported(token);
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

/** Decode the sparse value-level type channel on generated class instances. */
export function outboundTyToBamlTypeToken(
    ty: baml_bridge.cffi.v1.IBamlTy | null | undefined,
    typeMap: BamlTypeMap = getTypeMap(),
): BamlTypeToken | undefined {
    if (!ty) return undefined;
    if (ty.primitive) return TY_PRIMITIVE_TOKEN[ty.primitive.kind ?? -1];
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
        if (typeof token !== 'function') return undefined;
        const args = (ty.classTy.typeArgs ?? []).map(value => outboundTyToBamlTypeToken(value, typeMap));
        if (args.some((arg) => arg === undefined)) return undefined;
        return args.length ? { class: token as BamlClassCtor, args: args as BamlTypeToken[] } : token as BamlClassCtor;
    }
    if (ty.enum) return resolveNamedToken(ty.enum.name ?? '', typeMap) as BamlTypeToken | undefined;
    return undefined;
}

function resolveNamedToken(fqn: string, typeMap: BamlTypeMap): unknown {
    try {
        return typeMap.getClass(fqn);
    } catch {
        try {
            return typeMap.getEnum(fqn);
        } catch {
            return undefined;
        }
    }
}

function namedWireTy(token: unknown, args: BamlTypeToken[], typeMap: BamlTypeMap): baml_bridge.cffi.v1.IBamlTy {
    const fqn = typeMap.jsTypeToBamlType(token);
    if (!fqn) return unsupported(token);
    return { classTy: { name: fqn, typeArgs: args.map(value => lowerTypeToWireTy(value, typeMap)) } };
}
