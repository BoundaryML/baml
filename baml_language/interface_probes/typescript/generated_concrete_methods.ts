/** Add concrete_facades.baml alongside the shared fixture; no network calls. */
import assert from 'node:assert/strict';
import * as b from './generated/baml_sdk/index.js';
import { BamlType, BamlTypeMap, setTypeMap } from '@boundaryml/baml-bridge';

const client = await b.vendor.openai.ResponsesClient.new_async({ model: 'facade-probe', api_key: 'unused' });
assert.ok(client instanceof b.vendor.openai.ResponsesClient);
assert.equal(await client.id(), 'openai/facade-probe');
const agent = await b.ai.Agent.new_async({ client });
assert.ok(agent instanceof b.ai.Agent);
agent.close();
client.close();

const box: b.FacadeBox<string> = await b.facade_box_async();
assert.ok(box instanceof b.FacadeBox);
assert.equal('value' in box, false);
assert.equal(await box.read(), 'Ada');
assert.equal(await box.replace('Grace'), 'Grace');
const echoed: number = await box.echo(9, { $types: { U: BamlType.from('int') } });
assert.equal(echoed, 9);
const same: b.FacadeBox<string> = await box.same();
assert.ok(same instanceof b.FacadeBox);
assert.equal(await same.replace('Lin', { marker: 1 }), 'Lin');
assert.equal(await box.read(), 'Lin');
const record = await b.facade_record_async(box);
assert.ok(record.value instanceof b.FacadeBox);

setTypeMap(new BamlTypeMap());
const copied = await box.echo(record, { $types: {
  U: b.FacadeRecordType(b.FacadeBoxType(BamlType.from('string'))),
} });
assert.ok(copied instanceof b.FacadeRecord);
assert.ok(copied.value instanceof b.FacadeBox);
assert.equal(await copied.value.read(), 'Lin');

const flavor: b.FacadeFlavor = await box.echo(b.FacadeFlavor.Vanilla, { $types: { U: b.FacadeFlavorType() } });
assert.equal(flavor, b.FacadeFlavor.Vanilla);
const collision: BamlType<b.EvidenceItem> = b.EvidenceItemType_();
void collision;
const clone = box.clone();
box.close();
const pending = clone.replace('Ada');
clone.close();
assert.equal(await pending, 'Ada');
assert.equal(await same.read(), 'Ada');
same.close();
record.value.close();
copied.value.close();
console.log('Generated generic concrete methods, checked Self results, clients and Agent factories passed');
