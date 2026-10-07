// test_media.test.ts — mirrors bridge_python/tests/test_media.py.
// Constructors round-trip through the native accessors.

import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import {
    BamlRuntime,
    BamlImage,
    BamlAudio,
    BamlVideo,
    BamlPdf,
    decodeCallResult,
    encodeCallArgs,
} from '../dist/index.js';
import { baml_bridge } from '../dist/proto/baml_cffi.js';

type Media = { url(): string | null; name(): string | null; base64(): string; mimeType(): string | null };
type MediaCtor = {
    fromUrl(url: string, mimeType?: string): Media;
    fromFile(file: string, mimeType?: string): Media;
    fromFileContent(file: string, base64: string, mimeType?: string): Media;
    fromBase64(base64: string, mimeType?: string): Media;
};

// `fromFile` is BAML's `from_file`: it needs a runtime, with any program.
beforeAll(() => {
    BamlRuntime.initializeRuntime('.', { 'main.baml': 'function media_test_program() -> int { 1 }' });
});

const KINDS: Array<[string, MediaCtor]> = [
    ['BamlImage', BamlImage as unknown as MediaCtor],
    ['BamlAudio', BamlAudio as unknown as MediaCtor],
    ['BamlVideo', BamlVideo as unknown as MediaCtor],
    ['BamlPdf', BamlPdf as unknown as MediaCtor],
];

describe.each(KINDS)('%s', (_name, Ctor) => {
    test('fromUrl', () => {
        const m = Ctor.fromUrl('https://example.com/asset');
        expect(m.url()).toBe('https://example.com/asset');
        expect(m.name()).toBeNull();
        expect(m.mimeType()).toBeNull();
    });

    test('fromUrl with mime', () => {
        const m = Ctor.fromUrl('https://example.com/asset', 'application/octet-stream');
        expect(m.mimeType()).toBe('application/octet-stream');
    });

    test('fromFile reads the file now and keeps its name, not its path', () => {
        const directory = mkdtempSync(join(tmpdir(), 'baml-media-'));
        const path = join(directory, 'asset.bin');
        writeFileSync(path, 'hello');
        const m = Ctor.fromFile(path, 'application/octet-stream');
        // The file can go: the value holds its content.
        rmSync(directory, { recursive: true });
        expect(m.base64()).toBe('aGVsbG8=');
        expect(m.name()).toBe('asset.bin');
        expect(m.mimeType()).toBe('application/octet-stream');
        expect(m.url()).toBeNull();
    });

    test('fromFile fails when the value is built', () => {
        const missing = join(tmpdir(), 'baml-media-missing', 'nothing.bin');
        expect(() => Ctor.fromFile(missing)).toThrow(/nothing\.bin/);
    });

    test('fromFileContent is named and reads nothing', () => {
        const m = Ctor.fromFileContent('/nowhere/asset.bin', 'aGVsbG8=', 'application/octet-stream');
        expect(m.name()).toBe('asset.bin');
        expect(m.base64()).toBe('aGVsbG8=');
    });

    test('fromBase64', () => {
        const m = Ctor.fromBase64('aGVsbG8=');
        expect(m.base64()).toBe('aGVsbG8=');
        expect(m.name()).toBeNull();
    });
});

test('media crosses the call boundary as a portable payload', () => {
    const image = BamlImage.fromUrl('https://example.test/cat.png', 'image/png');
    const encoded = encodeCallArgs({ image }, {
        callId: 11n,
        functionName: 'user.AcceptImage',
    });
    const call = baml_bridge.cffi.v1.CallFunctionArgs.decode(encoded);
    expect(call.kwargs[0]?.value?.mediaValue).toMatchObject({
        media: 1,
        mimeType: 'image/png',
        url: 'https://example.test/cat.png',
    });
    expect(call.kwargs[0]?.value?.handle).toBeNull();
});

test('media read from a file crosses the call boundary with its name', () => {
    const directory = mkdtempSync(join(tmpdir(), 'baml-media-'));
    const path = join(directory, 'cat.png');
    writeFileSync(path, 'hello');
    const image = BamlImage.fromFile(path);
    rmSync(directory, { recursive: true });
    const encoded = encodeCallArgs({ image }, {
        callId: 12n,
        functionName: 'user.AcceptImage',
    });
    const call = baml_bridge.cffi.v1.CallFunctionArgs.decode(encoded);
    expect(call.kwargs[0]?.value?.mediaValue).toMatchObject({
        media: 1,
        mimeType: 'image/png',
        fileContent: { name: 'cat.png', base64: 'aGVsbG8=' },
    });
});

test('outbound named media keeps its name', () => {
    const envelope = baml_bridge.cffi.v1.BamlOutboundResult.encode({
        ok: { mediaValue: { media: 3, fileContent: { name: 'q3.pdf', base64: 'JVBERi0xLjc=' } } },
    }).finish();
    const pdf = decodeCallResult(envelope) as { name(): string | null; mimeType(): string | null };
    expect(pdf).toBeInstanceOf(BamlPdf);
    expect(pdf.name()).toBe('q3.pdf');
    expect(pdf.mimeType()).toBe('application/pdf');
});

test('outbound portable media reconstructs a fresh media wrapper', () => {
    const envelope = baml_bridge.cffi.v1.BamlOutboundResult.encode({
        ok: {
            mediaValue: {
                media: 1,
                mimeType: 'image/png',
                url: 'https://example.test/cat.png',
            },
        },
    }).finish();
    const image = decodeCallResult(envelope);
    expect(image).toBeInstanceOf(BamlImage);
    const media = image as { url(): string | null; mimeType(): string | null };
    expect(media.url()).toBe('https://example.test/cat.png');
    expect(media.mimeType()).toBe('image/png');
});
