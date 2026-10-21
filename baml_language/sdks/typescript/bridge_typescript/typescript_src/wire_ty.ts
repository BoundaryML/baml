// Host type tokens and opaque BEP-066 reflected type definitions.

import { BamlHandle, type HandleKey } from './native.js';
import { baml_bridge } from './proto/baml_cffi.js';
import { getTypeMap } from './typemap.js';

const TyPrimitiveKind = baml_bridge.cffi.v1.BamlTyPrimitiveKind;
const BamlTyMessage = baml_bridge.cffi.v1.BamlTy;

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

/** A constructor for a codegen-emitted BAML class. */
export type BamlClassCtor = new (...args: never[]) => unknown;

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

export interface BamlTypeMetadata {
    alias?: string;
    description?: string;
    docstring?: string;
    other?: Record<string, string>;
}

/** Internal row consumed by the generated `reflect.Class.new` binding. */
export class BamlTypeMetadataRow {
    constructor(
        readonly ty: BamlType,
        readonly alias: string | null,
        readonly description: string | null,
        readonly docstring: string | null,
        readonly other: Record<string, string> = {},
    ) {}
}

function cloneTy(ty: baml_bridge.cffi.v1.IBamlTy): baml_bridge.cffi.v1.BamlTy {
    const message = BamlTyMessage.fromObject(ty as Record<string, unknown>);
    return BamlTyMessage.decode(BamlTyMessage.encode(message).finish());
}

type Ty = baml_bridge.cffi.v1.IBamlTy;

/**
 * Every `live` head under a wire type, once per occurrence: each is a row of
 * the engine's handle table naming a runtime declaration.
 */
export function liveKeys(ty: Ty | null | undefined): HandleKey[] {
    const keys: HandleKey[] = [];
    const visit = (node: Ty | null | undefined): void => {
        if (!node) return;
        const head = (nominal: { live?: unknown } | null | undefined) => {
            if (nominal?.live != null) keys.push(nominal.live as HandleKey);
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
    readonly handles: readonly BamlHandle[];

    constructor(keys: readonly HandleKey[]) {
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
    readonly #ty: baml_bridge.cffi.v1.BamlTy;
    readonly #rows: LiveRows | undefined;

    private constructor(ty: Ty, rows: LiveRows | undefined) {
        this.#ty = cloneTy(ty);
        this.#rows = rows;
    }

    /** @internal A type the engine handed over: its `live` heads are rows this value now holds. */
    static _fromWire(ty: Ty): BamlType {
        return new BamlType(ty, new LiveRows(liveKeys(ty)));
    }

    /** @internal Bridge hook returning a fresh protobuf tree. */
    _wireCopy(): baml_bridge.cffi.v1.BamlTy {
        return cloneTy(this.#ty);
    }

    static from(token: BamlType | BamlTypeToken): BamlType {
        return token instanceof BamlType
            ? token
            : new BamlType(lowerTypeToWireTy(token), undefined);
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

    array(): BamlType {
        return new BamlType({ list: { item: this.#ty } }, this.#rows);
    }

    optional(): BamlType {
        return new BamlType({ optional: { inner: this.#ty } }, this.#rows);
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
    of(token: BamlType | BamlTypeToken): BamlType {
        return BamlType.from(token);
    },
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
export function lowerTypeToWireTy(token: BamlTypeToken): baml_bridge.cffi.v1.IBamlTy {
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
        return namedWireTy(token, []);
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
            return namedWireTy(shape.class, args as BamlTypeToken[]);
        }
        if ('list' in shape) return { list: { item: lowerTypeToWireTy(shape.list as BamlTypeToken) } };
        if ('map' in shape) {
            if (!Array.isArray(shape.map) || shape.map.length !== 2) return unsupported(token);
            const [key, value] = shape.map as BamlTypeToken[];
            return { map: { key: lowerTypeToWireTy(key), value: lowerTypeToWireTy(value) } };
        }
        if ('optional' in shape) return { optional: { inner: lowerTypeToWireTy(shape.optional as BamlTypeToken) } };
        if ('union' in shape) {
            if (!Array.isArray(shape.union)) return unsupported(token);
            return { union: { options: (shape.union as BamlTypeToken[]).map(lowerTypeToWireTy) } };
        }
        const fqn = getTypeMap().jsTypeToBamlType(token);
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
): BamlTypeToken | undefined {
    if (!ty) return undefined;
    if (ty.primitive) return TY_PRIMITIVE_TOKEN[ty.primitive.kind ?? -1];
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
        if (typeof token !== 'function') return undefined;
        const args = (ty.classTy.typeArgs ?? []).map(outboundTyToBamlTypeToken);
        if (args.some((arg) => arg === undefined)) return undefined;
        return args.length ? { class: token as BamlClassCtor, args: args as BamlTypeToken[] } : token as BamlClassCtor;
    }
    if (ty.enum) return resolveNamedToken(ty.enum.name ?? '') as BamlTypeToken | undefined;
    return undefined;
}

function resolveNamedToken(fqn: string): unknown {
    try {
        return getTypeMap().getClass(fqn);
    } catch {
        try {
            return getTypeMap().getEnum(fqn);
        } catch {
            return undefined;
        }
    }
}

function namedWireTy(token: unknown, args: BamlTypeToken[]): baml_bridge.cffi.v1.IBamlTy {
    const fqn = getTypeMap().jsTypeToBamlType(token);
    if (!fqn) return unsupported(token);
    return { classTy: { name: fqn, typeArgs: args.map(lowerTypeToWireTy) } };
}
