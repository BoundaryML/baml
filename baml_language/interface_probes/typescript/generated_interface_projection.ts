/** Actual generated callers; add required_interfaces.baml to the probe project. */
import assert from 'node:assert/strict';
import * as b from './generated/baml_sdk/index.js';
import { BamlType, Never, BamlImage, BamlTypeMap, setTypeMap } from '@boundaryml/baml-bridge';

const text = BamlType.from('string');
const reflectedText: BamlType<string> = b.reflect.Type.of('string');
assert.equal(reflectedText._wireCopy().root?.primitive?.kind, text._wireCopy().root?.primitive?.kind);
const bottom = BamlType.from(Never);
const target = b.RequiredBaseRef.type(text, text, bottom);
const recordType: BamlType<b.ProjectionRecord> = BamlType.from(b.ProjectionRecord);
class UnregisteredRecord extends b.ProjectionRecord {}
assert.throws(() => BamlType.from(UnregisteredRecord), /class type token does not belong/);
const recordOriginal = await b.record_decoder_async();
const recordDecoder = await recordOriginal.as_interface(b.DecoderRef.type(recordType, bottom));
const record: b.ProjectionRecord = await recordDecoder.decode(BamlImage.fromBase64('aGVsbG8=', 'image/png'));
assert.ok(record instanceof b.ProjectionRecord);
assert.equal(record.text, 'aGVsbG8=');
recordOriginal.close();
recordDecoder.close();
const original = await b.required_unpinned_async();
const selected: b.RequiredBaseRef<string, string, never> = await original.as_interface(target);
const result: string = await selected.apply('Ada');
assert.equal(result, 'Ada');
assert.equal(await b.use_required_base_async(selected), 'Ada');

await assert.rejects(original.as_interface(b.RequiredBaseRef.type(text, BamlType.from('int'), bottom)));
await assert.rejects(original.as_interface(b.RequiredBaseRef.type(text, text, text)));
assert.equal(await selected.apply('still valid'), 'still valid');

class Unregistered extends b.RequiredBaseRef<string, string, never> {}
assert.throws(() => original.as_interface(Unregistered.type(text, text, bottom)), /this reference's SDK/);
// The nested token also retains its generated constructor identity.
assert.throws(() => selected.as_interface(b.DecoderRef.type(Unregistered.type(text, text, bottom).array(), bottom)), /this reference's SDK/);

setTypeMap(new BamlTypeMap());
const recordAgain = await b.record_decoder_async();
const recordView = await recordAgain.as_interface(b.DecoderRef.type(recordType, bottom));
assert.ok(await recordView.decode(BamlImage.fromBase64('aGVsbG8=', 'image/png')) instanceof b.ProjectionRecord);
recordAgain.close();
recordView.close();
const pending = original.as_interface(target);
original.close();
selected.close();
const retained = await pending;
assert.equal(await retained.apply('Grace'), 'Grace');
retained.close();

function rejectedTypes() {
    class FabricatedType extends BamlType<string> {
        constructor() {
            // @ts-expect-error Subclassing cannot mint typed evidence from a raw schema.
            super({ root: { primitive: { kind: 2 } } });
        }
    }
    // @ts-expect-error Type evidence cannot change an associated binding.
    const wrong: BamlType<number> = text;
    // @ts-expect-error An unknown reflected type is not static string evidence.
    const erased: BamlType<string> = BamlType.from({ list: 'string' });
    // @ts-expect-error Every view argument must be specified.
    b.RequiredBaseRef.type(text, text);
    // @ts-expect-error A native string is not checked BamlType evidence.
    b.RequiredBaseRef.type(text, 'string', bottom);
    // @ts-expect-error The type selector itself is not a receiver.
    const receiver: b.RequiredBaseRef<string, string, never> = target;
    // @ts-expect-error Associated Error remains exact on the projected result.
    const changed: Promise<b.RequiredBaseRef<string, string, string>> = original.as_interface(target);
    void [FabricatedType, wrong, erased, receiver, changed];
}
void rejectedTypes;
console.log('Checked associated projection, native typing, SDK identity and retained admission passed');
