/** Run against the full shared fixture, through actual generated SDK code. */
import assert from 'node:assert/strict';
import * as b from './generated/baml_sdk/index.js';
import { BamlImage, BamlType, BamlTypeMap, setTypeMap } from '@boundaryml/baml-bridge';

const greeter: b.GreeterRef = await b.make_greeter_async('Hello');
setTypeMap(new BamlTypeMap());
assert.equal(await greeter.greet('Ada'), 'Hello, Ada!');
assert.equal(await greeter.label(), 'greeter');
const returned = await b.pass_greeter_async(greeter);
greeter.close();
assert.equal(await returned.greet('Grace'), 'Hello, Grace!');
returned.close();

const counter = await b.make_extended_counter_async(10);
const inherited: b.CounterRef = counter;
assert.equal(await inherited.current(), 10);
const parent: b.CounterRef = await b.pass_counter_async(counter);
assert.equal(await counter.add(2), 12);
assert.equal(await parent.current(), 12);
const record = await b.counter_record_async(counter);
assert.ok(record.counter instanceof b.CounterRef);
assert.equal(await record.counter.add(3), 15);
const copy = counter.clone();
counter.close();
const pending = copy.add(4);
copy.close();
assert.equal(await pending, 19);
assert.equal(await parent.current(), 19);
record.counter.close();
parent.close();

const decoder: b.DecoderRef<string, never> = await b.make_text_decoder_async();
const image = BamlImage.fromBase64('aGVsbG8=', 'image/png');
assert.equal(await decoder.decode(image), 'aGVsbG8=');
decoder.close();

const echo = await b.make_echo_async();
const text: string = await echo.echo('Ada', { $types: { T: BamlType.from('string') } });
assert.equal(text, 'Ada');
const echoInput = await b.make_counter_async(1);
const echoed = await echo.echo(echoInput, { $types: { T: b.CounterRef.type() } });
assert.ok(echoed instanceof b.CounterRef);
assert.equal(await echoed.add(2), 3);
echoInput.close();
echoed.close();
echo.close();

const iterable = await b.as_string_iterable_async(['Ada', 'Grace']);
const iterator: b.baml.iter.IteratorRef<string, never> = await iterable.iter();
iterable.close();
assert.equal(await iterator.next(), 'Ada');
assert.equal(await iterator.next(), 'Grace');
assert.ok(await iterator.next() instanceof b.baml.iter.Done);
iterator.close();

let retained: b.CounterRef | undefined;
assert.equal(await b.visit_counter_async(10, async value => {
    assert.ok(value instanceof b.CounterRef);
    retained = value;
    return await value.add(2);
}), 12);
assert.equal(await retained!.add(3), 15);
retained!.close();

const runner = await b.make_counter_runner_async(10);
assert.equal(await runner.visit(async (value, options = {}) => {
    try { return await value.add(options.amount ?? 1); }
    finally { value.close(); }
}), 12);
assert.equal(await runner.choose(3, async (value, options = {}) => value + (options.other ?? 0), {
    $types: { T: BamlType.from('int') },
}), 6);
runner.close();

const marked = await b.marker_record_async('initial');
const marker = await b.pass_marker_async(marked);
marked.text = 'local';
assert.equal(await b.marker_text_async(marker), 'initial');
assert.equal(await b.marker_text_async(marked), 'local');
marker.close();

console.log('Generated interface callers, associated types, inherited inputs, callbacks and non-class receivers passed');
