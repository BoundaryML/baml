/** Execute with the generated SDK for the shared fixture plus sdk_context.baml. */
import assert from 'node:assert/strict';
import * as b from './generated/baml_sdk/index.js';
import { BamlRuntime, BamlTypeMap, setTypeMap } from '@boundaryml/baml-bridge';

async function check(): Promise<void> {
    // These calls also exercise ESM setup through the real generated typemap,
    // which imports both root and nested namespace modules.
    const original: b.CodecPayload = await b.make_codec_payload_async();
    assert.equal(original.constructor, b.CodecPayload);
    setTypeMap(new BamlTypeMap());
    const copied: b.CodecPayload = await b.copy_codec_payload_async(original);
    assert.equal(copied.constructor, b.CodecPayload);
    assert.equal(copied.text, 'Ada');
    const nested: b.CodecPayload = await b.baml.identity_async(original, { $types: { T: b.CodecPayload } });
    assert.equal(nested.constructor, b.CodecPayload);
    const closure = await b.codec_closure_async();
    assert.equal(closure(original).constructor, b.CodecPayload);
    const passed: b.CodecPayload = b.apply_codec_closure(closure, original);
    assert.equal(passed.constructor, b.CodecPayload);
    const visited: b.CodecPayload = await b.visit_codec_async((value: b.CodecPayload) => {
        assert.equal(value.constructor, b.CodecPayload);
        return new b.CodecPayload({ text: 'Grace' });
    });
    assert.equal(visited.constructor, b.CodecPayload);
    assert.equal(visited.text, 'Grace');
    BamlRuntime.initializeRuntime('.', { 'main.baml': 'function make_codec_payload() -> int throws never { 99 }' });
    await assert.rejects(b.make_codec_payload_async(), /closed or replaced/);
    await assert.rejects(b.baml.identity_async(original, { $types: { T: b.CodecPayload } }), /closed or replaced/);
    assert.throws(() => closure(original), /closed or replaced/);
}

await check();
console.log('Generated SDK retains its runtime, codecs and callable carriers');
