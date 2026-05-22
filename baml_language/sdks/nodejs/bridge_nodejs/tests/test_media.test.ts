// test_media.test.ts — mirrors bridge_python/tests/test_media.py.

import { BamlImage, BamlAudio, BamlVideo, BamlPdf, BamlHandle } from '../index';

describe('BamlImage', () => {
    test('fromUrl returns url, no file, no mime', () => {
        const img = BamlImage.fromUrl('https://example.com/cat.png');
        expect(img.url()).toBe('https://example.com/cat.png');
        expect(img.file()).toBeNull();
        expect(img.mimeType()).toBeNull();
    });

    test('fromUrl carries mimeType', () => {
        const img = BamlImage.fromUrl('https://example.com/cat.png', 'image/png');
        expect(img.mimeType()).toBe('image/png');
    });

    test('fromFile returns file, no url', () => {
        const img = BamlImage.fromFile('/tmp/cat.png');
        expect(img.file()).toBe('/tmp/cat.png');
        expect(img.url()).toBeNull();
    });

    test('fromBase64 returns base64', () => {
        const img = BamlImage.fromBase64('aGVsbG8=');
        expect(img.base64()).toBe('aGVsbG8=');
        expect(img.url()).toBeNull();
        expect(img.file()).toBeNull();
    });

    test('_toHandle / _fromHandle round-trip', () => {
        const img = BamlImage.fromUrl('https://example.com/cat.png');
        const h = img._toHandle();
        expect(h).toBeInstanceOf(BamlHandle);
        const img2 = BamlImage._fromHandle(h);
        expect(img2.url()).toBe('https://example.com/cat.png');
    });
});

describe('BamlAudio', () => {
    test('fromUrl + accessors', () => {
        const a = BamlAudio.fromUrl('https://example.com/song.mp3', 'audio/mpeg');
        expect(a.url()).toBe('https://example.com/song.mp3');
        expect(a.mimeType()).toBe('audio/mpeg');
        expect(a.file()).toBeNull();
    });
    test('fromFile + accessors', () => {
        const a = BamlAudio.fromFile('/tmp/song.mp3');
        expect(a.file()).toBe('/tmp/song.mp3');
        expect(a.url()).toBeNull();
    });
    test('fromBase64 + accessors', () => {
        const a = BamlAudio.fromBase64('Zm9v');
        expect(a.base64()).toBe('Zm9v');
    });
});

describe('BamlVideo', () => {
    test('fromUrl + accessors', () => {
        const v = BamlVideo.fromUrl('https://example.com/clip.mp4', 'video/mp4');
        expect(v.url()).toBe('https://example.com/clip.mp4');
        expect(v.mimeType()).toBe('video/mp4');
    });
    test('fromFile + accessors', () => {
        const v = BamlVideo.fromFile('/tmp/clip.mp4');
        expect(v.file()).toBe('/tmp/clip.mp4');
    });
    test('fromBase64 + accessors', () => {
        const v = BamlVideo.fromBase64('Zm9v');
        expect(v.base64()).toBe('Zm9v');
    });
});

describe('BamlPdf', () => {
    test('fromUrl + accessors', () => {
        const p = BamlPdf.fromUrl('https://example.com/doc.pdf', 'application/pdf');
        expect(p.url()).toBe('https://example.com/doc.pdf');
        expect(p.mimeType()).toBe('application/pdf');
    });
    test('fromFile + accessors', () => {
        const p = BamlPdf.fromFile('/tmp/doc.pdf');
        expect(p.file()).toBe('/tmp/doc.pdf');
    });
    test('fromBase64 + accessors', () => {
        const p = BamlPdf.fromBase64('Zm9v');
        expect(p.base64()).toBe('Zm9v');
    });
});

describe('media _fromHandle validation', () => {
    test('rejects mismatched handle type', () => {
        const img = BamlImage.fromUrl('https://example.com/cat.png');
        const audioHandle = BamlAudio.fromUrl('https://example.com/song.mp3')._toHandle();
        expect(() => BamlImage._fromHandle(audioHandle)).toThrow();
        // Touch img to avoid unused warning
        expect(img.url()).toBeTruthy();
    });
});
