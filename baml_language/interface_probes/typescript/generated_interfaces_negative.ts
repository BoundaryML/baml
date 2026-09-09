/** Type-check only. Every marked invalid use must produce a diagnostic. */
import { BamlType } from '@boundaryml/baml-bridge';
import * as b from './generated/baml_sdk/index.js';

async function invalid(
    decoder: b.DecoderRef<string, never>,
    echo: b.EchoRef,
    unpinned: b.UnspecifiedCounterRef,
    greeter: b.GreeterRef,
    concrete: b.StoredCounter,
    generic: b.FacadeBox<string>,
    runner: b.CounterCallbackRunnerRef,
) {
    // @ts-expect-error Output is fixed for this checked view.
    const wrongOutput: b.DecoderRef<number, never> = decoder;
    // @ts-expect-error Error stays invariant even though TS exceptions are untyped.
    const wrongError: b.DecoderInput<string, string> = decoder;
    // @ts-expect-error A matching method shape is not implementation evidence.
    await b.welcome_async({ greet: async () => 'Hi', label: async () => 'greeter' }, 'Ada');
    // @ts-expect-error Missing method type evidence.
    await echo.echo('Ada');
    // @ts-expect-error Wrong method type argument name.
    await echo.echo('Ada', { $types: { Output: BamlType.from('string') } });
    // @ts-expect-error Required associated pins are unspecified.
    await unpinned.update(2);
    // @ts-expect-error Interface identity is not interchangeable.
    await b.add_in_baml_async(greeter, 1);
    // @ts-expect-error Method parameter retains its declared native type.
    await greeter.greet(1);
    // @ts-expect-error Live BAML state is not a writable host field.
    concrete.count = 9;
    // @ts-expect-error A live class is created by a BAML factory.
    new b.StoredCounter({ count: 9 });
    // @ts-expect-error Concrete class arguments stay invariant in emitted .d.ts too.
    const widened: b.FacadeBox<unknown> = generic;
    // @ts-expect-error Explicit wire type determines the native argument type.
    await echo.echo(42, { $types: { T: BamlType.from('string') } });
    // @ts-expect-error Primitive tokens describe string, not a particular string literal.
    const literal: 'Ada' = await echo.echo('Ada', { $types: { T: BamlType.from('string') } });
    // @ts-expect-error An explicit native type cannot contradict wire evidence.
    await echo.echo<number>(42, { $types: { T: BamlType.from('string') } });
    // @ts-expect-error Raw method tokens are no longer unchecked evidence.
    await echo.echo('Ada', { $types: { T: 'string' } });
    // @ts-expect-error Concrete generic methods have the same token/argument relationship.
    await generic.echo(42, { $types: { U: BamlType.from('string') } });
    // @ts-expect-error Callback results must use the token-selected native type too.
    await runner.choose(3, async () => 'wrong', { $types: { T: BamlType.from('int') } });
    // @ts-expect-error Generic nominal type constructors require checked arguments.
    b.FacadeBoxType('string');
    // @ts-expect-error Missing generic class type argument.
    b.FacadeBoxType();
    // @ts-expect-error Nominal type evidence cannot change its generic arguments.
    const wrongBox: BamlType<b.FacadeBox<number>> = b.FacadeBoxType(BamlType.from('string'));
    // @ts-expect-error Enum evidence is not string evidence.
    const wrongEnum: BamlType<string> = b.FacadeFlavorType();
    return [wrongOutput, wrongError, widened];
}
void invalid;
