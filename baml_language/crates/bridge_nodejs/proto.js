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
const CallFunctionArgs = baml_cffi_1.baml_core.cffi.v1.CallFunctionArgs;
const BamlOutboundValue = baml_cffi_1.baml_core.cffi.v1.BamlOutboundValue;
// ─── Inbound (TS → Rust) ───
function isPlainObject(value) {
    if (value == null || typeof value !== 'object')
        return false;
    const proto = Object.getPrototypeOf(value);
    return proto === Object.prototype || proto === null;
}
function setInboundValue(iv, value) {
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
    else if (Array.isArray(value)) {
        const listVal = [];
        for (const item of value) {
            const child = {};
            setInboundValue(child, item);
            listVal.push(child);
        }
        iv.listValue = { values: listVal };
    }
    else if (isPlainObject(value)) {
        const entries = [];
        for (const [k, v] of Object.entries(value)) {
            const entry = { stringKey: k };
            const childVal = {};
            setInboundValue(childVal, v);
            entry.value = childVal;
            entries.push(entry);
        }
        iv.mapValue = { entries };
    }
    else {
        throw new TypeError(`Cannot encode value of type ${Object.prototype.toString.call(value)} to protobuf`);
    }
}
function encodeCallArgs(kwargs) {
    const entries = [];
    for (const [key, value] of Object.entries(kwargs)) {
        const entry = { stringKey: key };
        const iv = {};
        setInboundValue(iv, value);
        entry.value = iv;
        entries.push(entry);
    }
    const msg = CallFunctionArgs.create({ kwargs: entries });
    return Buffer.from(CallFunctionArgs.encode(msg).finish());
}
// ─── Outbound (Rust → TS) ───
function decodeValueHolder(holder) {
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
    if (holder.classValue) {
        const obj = Object.create(null);
        for (const entry of holder.classValue.fields || []) {
            if (entry.key != null && entry.value) {
                obj[entry.key] = decodeValueHolder(entry.value);
            }
        }
        return obj;
    }
    if (holder.enumValue)
        return holder.enumValue.value;
    if (holder.literalValue) {
        if (holder.literalValue.stringLiteral != null)
            return holder.literalValue.stringLiteral.value;
        if (holder.literalValue.intLiteral != null)
            return Number(holder.literalValue.intLiteral.value);
        if (holder.literalValue.boolLiteral != null)
            return holder.literalValue.boolLiteral.value;
    }
    if (holder.listValue) {
        return (holder.listValue.items || []).map(item => decodeValueHolder(item));
    }
    if (holder.mapValue) {
        const obj = Object.create(null);
        for (const entry of holder.mapValue.entries || []) {
            if (entry.key != null && entry.value) {
                obj[entry.key] = decodeValueHolder(entry.value);
            }
        }
        return obj;
    }
    if (holder.unionVariantValue && holder.unionVariantValue.value) {
        return decodeValueHolder(holder.unionVariantValue.value);
    }
    // handle_value: pass the protobufjs Long directly as the key — BamlHandle's
    // constructor accepts { low, high } which is layout-compatible with Long.
    if (holder.handleValue) {
        return new native_1.BamlHandle(holder.handleValue.key, holder.handleValue.handleType ?? 0);
    }
    // FIXME: Unknown/unsupported outbound variants silently collapse to null, making them
    // indistinguishable from a legitimate BAML null result. Legacy engine/ threw via Rust
    // Err/anyhow. bridge_python has the same silent `return None` fallthrough. Leaving as-is
    // for parity with bridge_python; fix both together if this becomes a forward-compat issue.
    return null;
}
function decodeCallResult(data) {
    const msg = BamlOutboundValue.decode(data instanceof Buffer ? data : Buffer.from(data));
    return decodeValueHolder(msg);
}
//# sourceMappingURL=proto.js.map