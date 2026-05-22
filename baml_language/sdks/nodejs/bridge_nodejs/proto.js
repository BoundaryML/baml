/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
"use strict";
// proto.ts — mirrors bridge_python/python_src/baml_py/proto.py
//
// Encodes TS objects → CallFunctionArgs protobuf bytes (for sending to Rust)
// Decodes BamlOutboundValue protobuf bytes → TS objects (for receiving from Rust)
Object.defineProperty(exports, "__esModule", { value: true });
exports.encodeCallArgs = encodeCallArgs;
exports.decodeCallResult = decodeCallResult;
const baml_cffi_1 = require("./proto/baml_cffi");
const native_1 = require("./native");
const stream_1 = require("./stream");
const typemap_1 = require("./typemap");
const errors_1 = require("./errors");
const BamlHandleType = baml_cffi_1.baml_core.cffi.v1.BamlHandleType;
const CallFunctionArgs = baml_cffi_1.baml_core.cffi.v1.CallFunctionArgs;
const BamlOutboundValue = baml_cffi_1.baml_core.cffi.v1.BamlOutboundValue;
// ─── Inbound (TS → Rust) ───
function isPlainObject(value) {
    if (value == null || typeof value !== 'object')
        return false;
    const proto = Object.getPrototypeOf(value);
    return proto === Object.prototype || proto === null;
}
function setInboundValue(iv, value, typeMap) {
    if (value === null || value === undefined) {
        return; // Leave oneof unset → null
    }
    if (typeof value === 'boolean') {
        iv.boolValue = value;
    }
    else if (typeof value === 'number') {
        if (Number.isInteger(value)) {
            iv.intValue = value;
        }
        else {
            iv.floatValue = value;
        }
    }
    else if (typeof value === 'string') {
        iv.stringValue = value;
    }
    else if (value instanceof Uint8Array) {
        iv.uint8arrayValue = value;
    }
    else if (value instanceof native_1.BamlHandle) {
        iv.handle = { key: value.key, handleType: value.handleType };
    }
    else if (value instanceof stream_1.BamlStream) {
        const h = value._toHandle();
        iv.handle = { key: h.key, handleType: h.handleType };
    }
    else if (value instanceof native_1.BamlImage ||
        value instanceof native_1.BamlAudio ||
        value instanceof native_1.BamlVideo ||
        value instanceof native_1.BamlPdf) {
        const h = value._toHandle();
        iv.handle = { key: h.key, handleType: h.handleType };
    }
    else if (Array.isArray(value)) {
        const listVal = [];
        for (const item of value) {
            const child = {};
            setInboundValue(child, item, typeMap);
            listVal.push(child);
        }
        iv.listValue = { values: listVal };
    }
    else if (isPlainObject(value)) {
        const entries = [];
        for (const [k, v] of Object.entries(value)) {
            const entry = { stringKey: k };
            const childVal = {};
            setInboundValue(childVal, v, typeMap);
            entry.value = childVal;
            entries.push(entry);
        }
        iv.mapValue = { entries };
    }
    else {
        // Reverse-lookup: is this an instance of a codegen-emitted class?
        // `jsTypeToBamlType` walks the prototype chain looking for a known
        // class identity. Returns "" if no match.
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const fqn = typeMap.jsTypeToBamlType(value.constructor);
        if (fqn) {
            const fields = [];
            for (const [k, v] of Object.entries(value)) {
                const childVal = {};
                setInboundValue(childVal, v, typeMap);
                fields.push({ stringKey: k, value: childVal });
            }
            iv.classValue = { name: fqn, fields };
        }
        else {
            throw new TypeError(`Cannot encode value of type ${Object.prototype.toString.call(value)} to protobuf`);
        }
    }
}
function encodeCallArgs(kwargs) {
    const typeMap = (0, typemap_1.getTypeMap)();
    const entries = [];
    for (const [key, value] of Object.entries(kwargs)) {
        const entry = { stringKey: key };
        const iv = {};
        setInboundValue(iv, value, typeMap);
        entry.value = iv;
        entries.push(entry);
    }
    const msg = CallFunctionArgs.create({ kwargs: entries });
    return Buffer.from(CallFunctionArgs.encode(msg).finish());
}
// ─── Outbound (Rust → TS) ───
function decodeValue(holder, typeMap) {
    if (holder.nullValue != null)
        return null;
    if (holder.stringValue != null)
        return holder.stringValue;
    if (holder.intValue != null)
        return Number(holder.intValue);
    if (holder.floatValue != null)
        return holder.floatValue;
    if (holder.boolValue != null)
        return holder.boolValue;
    if (holder.uint8arrayValue != null)
        return holder.uint8arrayValue;
    if (holder.classValue)
        return decodeClass(holder.classValue, typeMap);
    if (holder.enumValue)
        return decodeEnum(holder.enumValue, typeMap);
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
        const obj = Object.create(null);
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
    if (holder.handleValue)
        return decodeHandle(holder.handleValue, typeMap);
    return null;
}
function decodeClass(classValue, typeMap) {
    const fields = Object.create(null);
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
    let Cls;
    try {
        Cls = typeMap.getClass(fqn);
    }
    catch {
        // Unknown FQN — return the plain object rather than throwing.
        // Phase 5 default: typed decoding is best-effort.
        return fields;
    }
    // Media-class unwrap: the wire shape for `baml.media.{Image,…}` is
    // `class_value { fields: { _data: handle_value(ADT_MEDIA_*) } }`. The
    // _data field already decoded to the typed media instance via
    // `decodeHandle`; return it directly so callers see `BamlImage`
    // rather than `{ _data: BamlImage }`.
    if ((Cls === native_1.BamlImage || Cls === native_1.BamlAudio || Cls === native_1.BamlVideo || Cls === native_1.BamlPdf) &&
        '_data' in fields) {
        return fields._data;
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const Ctor = Cls;
    try {
        return new Ctor(fields);
    }
    catch {
        // Constructor failed (e.g. user-emitted class with no compatible
        // signature) — fall back to a prototype-attached field bag.
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        return Object.assign(Object.create(Cls.prototype), fields);
    }
}
function decodeEnum(enumValue, typeMap) {
    const variant = enumValue.value ?? '';
    const fqn = enumValue.name?.name ?? '';
    if (!fqn)
        return variant;
    let Cls;
    try {
        Cls = typeMap.getEnum(fqn);
    }
    catch {
        return variant;
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const obj = Cls;
    if (variant in obj)
        return obj[variant];
    throw new errors_1.BamlError(`Unknown enum variant ${variant} on ${fqn}`);
}
function decodeHandle(handle, typeMap) {
    const bh = new native_1.BamlHandle(handle.key, handle.handleType ?? 0);
    const ht = handle.handleType ?? 0;
    if (ht === BamlHandleType.ADT_MEDIA_IMAGE)
        return native_1.BamlImage._fromHandle(bh);
    if (ht === BamlHandleType.ADT_MEDIA_AUDIO)
        return native_1.BamlAudio._fromHandle(bh);
    if (ht === BamlHandleType.ADT_MEDIA_VIDEO)
        return native_1.BamlVideo._fromHandle(bh);
    if (ht === BamlHandleType.ADT_MEDIA_PDF)
        return native_1.BamlPdf._fromHandle(bh);
    if (ht === BamlHandleType.HANDLE_UNSPECIFIED) {
        throw new errors_1.BamlError('BEX emitted HANDLE_UNSPECIFIED (Rust-side bug)');
    }
    // ADT_TAGGED_HEAP_HANDLE → typed wrapper via typemap (FQN on the
    // wire's BamlHandle.name). Phase 5 best-effort: if the FQN isn't
    // present or the typemap doesn't know it, fall through to a bare
    // BamlHandle. BamlStream (FQN baml.llm.Stream) is the canonical case.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const named = handle;
    const fqn = named?.name?.name ?? '';
    if (ht === BamlHandleType.ADT_TAGGED_HEAP_HANDLE && fqn === 'baml.llm.Stream') {
        return stream_1.BamlStream._fromHandle(bh);
    }
    if (ht === BamlHandleType.ADT_TAGGED_HEAP_HANDLE && fqn) {
        try {
            const Cls = typeMap.getClass(fqn);
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            const FromHandle = Cls?._fromHandle;
            if (typeof FromHandle === 'function')
                return FromHandle.call(Cls, bh);
        }
        catch {
            // Fall through to bare BamlHandle.
        }
    }
    return bh;
}
function decodeCallResult(data) {
    const typeMap = (0, typemap_1.getTypeMap)();
    const msg = BamlOutboundValue.decode(data instanceof Buffer ? data : Buffer.from(data));
    return decodeValue(msg, typeMap);
}
//# sourceMappingURL=proto.js.map