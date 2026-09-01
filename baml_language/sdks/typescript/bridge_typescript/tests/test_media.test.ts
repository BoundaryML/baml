// test_media.test.ts — mirrors bridge_python/tests/test_media.py.
// Constructors round-trip through the native accessors.

import {
    BamlImage,
    BamlAudio,
    BamlVideo,
    BamlPdf,
    decodeCallResult,
    encodeCallArgs,
} from '../dist/index.js';
import { baml_bridge } from '../dist/proto/baml_cffi.js';

type MediaCtor = {
    fromUrl(url: string, mimeType?: string): { url(): string | null; file(): string | null; base64(): string; mimeType(): string | null };
    fromFile(file: string, mimeType?: string): { url(): string | null; file(): string | null; base64(): string; mimeType(): string | null };
    fromBase64(base64: string, mimeType?: string): { url(): string | null; file(): string | null; base64(): string; mimeType(): string | null };
};

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
        expect(m.file()).toBeNull();
        expect(m.mimeType()).toBeNull();
    });

    test('fromUrl with mime', () => {
        const m = Ctor.fromUrl('https://example.com/asset', 'application/octet-stream');
        expect(m.mimeType()).toBe('application/octet-stream');
    });

    test('fromFile', () => {
        const m = Ctor.fromFile('/tmp/asset');
        expect(m.file()).toBe('/tmp/asset');
        expect(m.url()).toBeNull();
    });

    test('fromBase64', () => {
        const m = Ctor.fromBase64('aGVsbG8=');
        expect(m.base64()).toBe('aGVsbG8=');
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
