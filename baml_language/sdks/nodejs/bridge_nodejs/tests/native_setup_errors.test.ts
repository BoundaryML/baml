import { spawnSync } from 'node:child_process';
import {
    BamlPanic,
    BamlTypeMap,
    initializeRuntime,
    initializeRuntimeFromBytecode,
    makeSdkPanic,
    setTypeMap,
} from '../dist/index.js';

const emptyTypeMap = (): BamlTypeMap => BamlTypeMap.fromLazyEntries({
    classes: {},
    enums: {},
    typeAliases: {},
});

function capture(run: () => unknown): unknown {
    try {
        run();
    } catch (error) {
        return error;
    }
    throw new Error('expected call to throw');
}

afterEach(() => {
    setTypeMap(emptyTypeMap());
});

describe('native setup failures', () => {
    test('makeSdkPanic falls back to the message before a typemap is installed', () => {
        setTypeMap(emptyTypeMap());
        const panic = makeSdkPanic('setup failed');

        expect(panic).toBeInstanceOf(BamlPanic);
        expect(panic.className).toBe('baml.panics.SdkPanic');
        expect(panic.value).toBe('setup failed');
    });

    test('makeSdkPanic constructs the generated SdkPanic value when mapped', () => {
        class GeneratedSdkPanic {
            message!: string;
            constructor(init: { message: string }) {
                Object.assign(this, init);
            }
        }
        setTypeMap(BamlTypeMap.fromLazyEntries({
            classes: { 'baml.panics.SdkPanic': () => GeneratedSdkPanic },
            enums: {},
            typeAliases: {},
        }));

        const panic = makeSdkPanic('typed setup failure');

        expect(panic.value).toBeInstanceOf(GeneratedSdkPanic);
        expect((panic.value as GeneratedSdkPanic).message).toBe('typed setup failure');
    });

    test('invalid source initialization surfaces BamlPanic(SdkPanic)', () => {
        setTypeMap(emptyTypeMap());
        const error = capture(() => initializeRuntime('.', {
            'main.baml': 'this is not valid baml',
        }));

        expect(error).toBeInstanceOf(BamlPanic);
        expect((error as BamlPanic).className).toBe('baml.panics.SdkPanic');
        expect(typeof (error as BamlPanic).value).toBe('string');
        expect((error as BamlPanic).value).not.toMatch(/^Baml(?:InvalidArgument|Client)Error:/);
    });

    test('invalid bytecode initialization surfaces BamlPanic(SdkPanic)', () => {
        setTypeMap(emptyTypeMap());
        const error = capture(() => initializeRuntimeFromBytecode(new Uint8Array([1, 2, 3])));

        expect(error).toBeInstanceOf(BamlPanic);
        expect((error as BamlPanic).className).toBe('baml.panics.SdkPanic');
        expect(typeof (error as BamlPanic).value).toBe('string');
        expect((error as BamlPanic).value).not.toMatch(/^Baml(?:InvalidArgument|Client)Error:/);
    });

    test('getRuntime before initialization surfaces BamlPanic(SdkPanic)', () => {
        // A child process guarantees a pristine process-global runtime even if
        // another Vitest file initialized the singleton in this worker.
        const bridgeUrl = new URL('../dist/index.js', import.meta.url).href;
        const script = `
            import { BamlPanic, getRuntime } from ${JSON.stringify(bridgeUrl)};
            let result;
            try {
                getRuntime();
                result = { threw: false };
            } catch (error) {
                result = {
                    threw: true,
                    isPanic: error instanceof BamlPanic,
                    className: error.className,
                    valueType: typeof error.value,
                };
            }
            console.log(JSON.stringify(result));
        `;
        const child = spawnSync(process.execPath, ['--input-type=module', '--eval', script], {
            encoding: 'utf8',
        });

        expect(child.status, child.stderr).toBe(0);
        expect(JSON.parse(child.stdout.trim())).toEqual({
            threw: true,
            isPanic: true,
            className: 'baml.panics.SdkPanic',
            valueType: 'string',
        });
    });
});
