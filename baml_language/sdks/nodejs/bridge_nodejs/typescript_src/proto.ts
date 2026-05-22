// proto.ts — mirrors bridge_python/python_src/baml_py/proto.py
//
// Encodes TS objects → CallFunctionArgs protobuf bytes (for sending to Rust)
// Decodes BamlOutboundValue protobuf bytes → TS objects (for receiving from Rust)

import { baml_core } from './proto/baml_cffi';
import { BamlHandle, BamlImage, BamlAudio, BamlVideo, BamlPdf } from './native';
import { BamlStream } from './stream';
import { BamlTypeMap, getTypeMap } from './typemap';
import { BamlError } from './errors';

const BamlHandleType = baml_core.cffi.v1.BamlHandleType;

const CallFunctionArgs = baml_core.cffi.v1.CallFunctionArgs;
const BamlOutboundValue = baml_core.cffi.v1.BamlOutboundValue;

// ─── Inbound (TS → Rust) ───

function isPlainObject(value: unknown): value is Record<string, unknown> {
    if (value == null || typeof value !== 'object') return false;
    const proto = Object.getPrototypeOf(value);
    return proto === Object.prototype || proto === null;
}

function setInboundValue(
    iv: baml_core.cffi.v1.IInboundValue,
    value: unknown,
    typeMap: BamlTypeMap,
): void {
    if (value === null || value === undefined) {
        return; // Leave oneof unset → null
    }
    if (typeof value === 'boolean') {
        iv.boolValue = value;
    } else if (typeof value === 'number') {
        if (Number.isInteger(value)) {
            iv.intValue = value;
        } else {
            iv.floatValue = value;
        }
    } else if (typeof value === 'string') {
        iv.stringValue = value;
    } else if (value instanceof Uint8Array) {
        iv.uint8arrayValue = value;
    } else if (value instanceof BamlHandle) {
        iv.handle = { key: value.key, handleType: value.handleType };
    } else if (value instanceof BamlStream) {
        const h = value._toHandle();
        iv.handle = { key: h.key, handleType: h.handleType };
    } else if (
        value instanceof BamlImage ||
        value instanceof BamlAudio ||
        value instanceof BamlVideo ||
        value instanceof BamlPdf
    ) {
        const h = value._toHandle();
        iv.handle = { key: h.key, handleType: h.handleType };
    } else if (Array.isArray(value)) {
        const listVal: baml_core.cffi.v1.IInboundValue[] = [];
        for (const item of value) {
            const child: baml_core.cffi.v1.IInboundValue = {};
            setInboundValue(child, item, typeMap);
            listVal.push(child);
        }
        iv.listValue = { values: listVal };
    } else if (isPlainObject(value)) {
        const entries: baml_core.cffi.v1.IInboundMapEntry[] = [];
        for (const [k, v] of Object.entries(value)) {
            const entry: baml_core.cffi.v1.IInboundMapEntry = { stringKey: k };
            const childVal: baml_core.cffi.v1.IInboundValue = {};
            setInboundValue(childVal, v, typeMap);
            entry.value = childVal;
            entries.push(entry);
        }
        iv.mapValue = { entries };
    } else {
        // Reverse-lookup: is this an instance of a codegen-emitted class?
        // `jsTypeToBamlType` walks the prototype chain looking for a known
        // class identity. Returns "" if no match.
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const fqn = typeMap.jsTypeToBamlType((value as any).constructor);
        if (fqn) {
            const fields: baml_core.cffi.v1.IInboundMapEntry[] = [];
            for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
                const childVal: baml_core.cffi.v1.IInboundValue = {};
                setInboundValue(childVal, v, typeMap);
                fields.push({ stringKey: k, value: childVal });
            }
            iv.classValue = { name: fqn, fields };
        } else {
            throw new TypeError(
                `Cannot encode value of type ${Object.prototype.toString.call(value)} to protobuf`,
            );
        }
    }
}

export function encodeCallArgs(kwargs: Record<string, unknown>): Buffer {
    const typeMap = getTypeMap();
    const entries: baml_core.cffi.v1.IInboundMapEntry[] = [];
    for (const [key, value] of Object.entries(kwargs)) {
        const entry: baml_core.cffi.v1.IInboundMapEntry = { stringKey: key };
        const iv: baml_core.cffi.v1.IInboundValue = {};
        setInboundValue(iv, value, typeMap);
        entry.value = iv;
        entries.push(entry);
    }
    const msg = CallFunctionArgs.create({ kwargs: entries });
    return Buffer.from(CallFunctionArgs.encode(msg).finish());
}

// ─── Outbound (Rust → TS) ───

function decodeValue(
    holder: baml_core.cffi.v1.IBamlOutboundValue,
    typeMap: BamlTypeMap,
): unknown {
    if (holder.nullValue != null) return null;
    if (holder.stringValue != null) return holder.stringValue;
    if (holder.intValue != null) return Number(holder.intValue);
    if (holder.floatValue != null) return holder.floatValue;
    if (holder.boolValue != null) return holder.boolValue;
    if (holder.uint8arrayValue != null) return holder.uint8arrayValue;
    if (holder.classValue) return decodeClass(holder.classValue, typeMap);
    if (holder.enumValue) return decodeEnum(holder.enumValue, typeMap);
    if (holder.literalValue) {
        if (holder.literalValue.stringLiteral != null) {
            return holder.literalValue.stringLiteral.value;
        }
        if (holder.literalValue.intLiteral != null) {
            return Number(holder.literalValue.intLiteral.value);
        }
        if (holder.literalValue.boolLiteral != null) {
            return holder.literalValue.boolLiteral.value;
        }
    }
    if (holder.listValue) {
        return (holder.listValue.items || []).map(item => decodeValue(item, typeMap));
    }
    if (holder.mapValue) {
        const obj: Record<string, unknown> = Object.create(null);
        for (const entry of holder.mapValue.entries || []) {
            if (entry.key != null && entry.value) {
                obj[entry.key] = decodeValue(entry.value, typeMap);
            }
        }
        return obj;
    }
    if (holder.unionVariantValue && holder.unionVariantValue.value) {
        // Union metadata is discarded — TS is duck-typed and the inner
        // value carries enough shape to dispatch on at the use site.
        return decodeValue(holder.unionVariantValue.value, typeMap);
    }
    if (holder.handleValue) return decodeHandle(holder.handleValue, typeMap);
    return null;
}

function decodeClass(
    classValue: baml_core.cffi.v1.IBamlValueClass,
    typeMap: BamlTypeMap,
): unknown {
    const fields: Record<string, unknown> = Object.create(null);
    for (const entry of classValue.fields || []) {
        if (entry.key != null && entry.value) {
            fields[entry.key] = decodeValue(entry.value, typeMap);
        }
    }
    const fqn = classValue.name?.name ?? '';
    if (!fqn) {
        // No FQN — fall back to a plain object. Phase 5 typed decoding
        // depends on codegen emitting class_value with `name`.
        return fields;
    }
    let Cls: unknown;
    try {
        Cls = typeMap.getClass(fqn);
    } catch {
        // Unknown FQN — return the plain object rather than throwing.
        // Phase 5 default: typed decoding is best-effort.
        return fields;
    }
    // Media-class unwrap: the wire shape for `baml.media.{Image,…}` is
    // `class_value { fields: { _data: handle_value(ADT_MEDIA_*) } }`. The
    // _data field already decoded to the typed media instance via
    // `decodeHandle`; return it directly so callers see `BamlImage`
    // rather than `{ _data: BamlImage }`.
    if (
        (Cls === BamlImage || Cls === BamlAudio || Cls === BamlVideo || Cls === BamlPdf) &&
        '_data' in fields
    ) {
        return fields._data;
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const Ctor = Cls as new (init: Record<string, unknown>) => any;
    try {
        return new Ctor(fields);
    } catch {
        // Constructor failed (e.g. user-emitted class with no compatible
        // signature) — fall back to a prototype-attached field bag.
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        return Object.assign(Object.create((Cls as any).prototype), fields);
    }
}

function decodeEnum(
    enumValue: baml_core.cffi.v1.IBamlValueEnum,
    typeMap: BamlTypeMap,
): unknown {
    const variant = enumValue.value ?? '';
    const fqn = enumValue.name?.name ?? '';
    if (!fqn) return variant;
    let Cls: unknown;
    try {
        Cls = typeMap.getEnum(fqn);
    } catch {
        return variant;
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const obj = Cls as Record<string, any>;
    if (variant in obj) return obj[variant];
    throw new BamlError(`Unknown enum variant ${variant} on ${fqn}`);
}

function decodeHandle(
    handle: baml_core.cffi.v1.IBamlHandle,
    typeMap: BamlTypeMap,
): unknown {
    const bh = new BamlHandle(
        handle.key as unknown as { low: number; high: number },
        handle.handleType ?? 0,
    );
    const ht = handle.handleType ?? 0;
    if (ht === BamlHandleType.ADT_MEDIA_IMAGE) return BamlImage._fromHandle(bh);
    if (ht === BamlHandleType.ADT_MEDIA_AUDIO) return BamlAudio._fromHandle(bh);
    if (ht === BamlHandleType.ADT_MEDIA_VIDEO) return BamlVideo._fromHandle(bh);
    if (ht === BamlHandleType.ADT_MEDIA_PDF) return BamlPdf._fromHandle(bh);
    if (ht === BamlHandleType.HANDLE_UNSPECIFIED) {
        throw new BamlError('BEX emitted HANDLE_UNSPECIFIED (Rust-side bug)');
    }
    // ADT_TAGGED_HEAP_HANDLE → typed wrapper via typemap (FQN on the
    // wire's BamlHandle.name). Phase 5 best-effort: if the FQN isn't
    // present or the typemap doesn't know it, fall through to a bare
    // BamlHandle. BamlStream (FQN baml.llm.Stream) is the canonical case.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const named = handle as any;
    const fqn: string = named?.name?.name ?? '';
    if (ht === BamlHandleType.ADT_TAGGED_HEAP_HANDLE && fqn === 'baml.llm.Stream') {
        return BamlStream._fromHandle(bh);
    }
    if (ht === BamlHandleType.ADT_TAGGED_HEAP_HANDLE && fqn) {
        try {
            const Cls = typeMap.getClass(fqn);
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            const FromHandle = (Cls as any)?._fromHandle;
            if (typeof FromHandle === 'function') return FromHandle.call(Cls, bh);
        } catch {
            // Fall through to bare BamlHandle.
        }
    }
    return bh;
}

export function decodeCallResult(data: Buffer | Uint8Array): unknown {
    const typeMap = getTypeMap();
    const msg = BamlOutboundValue.decode(data instanceof Buffer ? data : Buffer.from(data));
    return decodeValue(msg, typeMap);
}
