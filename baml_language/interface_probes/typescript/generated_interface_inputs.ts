/** Add interface_inputs.baml and typescript_interface_names.baml to the fixture. */
import assert from 'node:assert/strict';
import { BamlType } from '@boundaryml/baml-bridge';
import * as b from './generated/baml_sdk/index.js';

const dual = await b.dual_mapper_async();
assert.equal(await b.use_int_mapper_async(dual), 10);
assert.equal(await b.use_text_mapper_async(dual), 'Ada!');
const intInput: b.MapperInput_<number, string> = dual;
const textInput: b.MapperInput_<string, never> = dual;
void [intInput, textInput];

// These declarations are type-only; do not execute rejected calls.
function rejected() {
    // @ts-expect-error Separate implementations do not grant a union view.
    const union: b.MapperInput_<number | string, string> = dual;
    // @ts-expect-error Int implementation has Error=string, not never.
    const neverError: b.MapperInput_<number, never> = dual;
    // @ts-expect-error Input companion must not hide the authored class.
    const collision: b.MapperInput = dual;
    return [union, neverError, collision];
}
void rejected;

const thenLike = await b.make_then_like_async();
assert.equal(await thenLike.then_(), 'then method');
assert.equal(await thenLike.close__(), 'close method');
assert.equal(await thenLike.close_(), 'authored underscore');
thenLike.close();
console.log('Multiple checked input views and Promise/lifecycle method collisions passed');

const names = await b.make_frame_names_async();
const receiverResult = await names.receiver_result();
const methodResult = await names.method_result({ $types: { U: BamlType.from('int') } });
assert.ok(receiverResult instanceof b._P0);
assert.ok(methodResult instanceof b._M0);
assert.equal(receiverResult.value, 'receiver');
assert.equal(methodResult.value, 'method');
names.close();
