// test_media.test.ts — mirrors bridge_python/tests/test_media.py.
// Constructors round-trip through the native accessors.

import { BamlImage, BamlAudio, BamlVideo, BamlPdf } from '../dist/index.js';

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
