import { describe, expect, it } from 'vitest';
import type { BamlJsMedia } from '@b/pkg-proto';
import { findImageMedia, isBamlMedia, mediaToSrc } from '../media-values';

const imageUrl: BamlJsMedia = {
  $baml: { type: '$media' },
  media_type: 'image',
  mime_type: 'image/png',
  content_type: 'url',
  url: 'https://example.com/image.png',
};

const imageBase64: BamlJsMedia = {
  $baml: { type: '$media' },
  media_type: 'image',
  mime_type: 'image/jpeg',
  content_type: 'base64',
  base64: 'abc123',
};

const audio: BamlJsMedia = {
  $baml: { type: '$media' },
  media_type: 'audio',
  mime_type: 'audio/mpeg',
  content_type: 'url',
  url: 'https://example.com/audio.mp3',
};

describe('media-values', () => {
  it('detects $media values', () => {
    expect(isBamlMedia(imageUrl)).toBe(true);
    expect(isBamlMedia({ $baml: { type: 'Other' } })).toBe(false);
  });

  it('converts URL and base64 media to renderable src values', () => {
    expect(mediaToSrc(imageUrl)).toBe('https://example.com/image.png');
    expect(mediaToSrc(imageBase64)).toBe('data:image/jpeg;base64,abc123');
  });

  it('extracts nested image media while ignoring non-image media', () => {
    const value = ['caption', { first: imageUrl, nested: [audio, imageBase64] }];
    expect(findImageMedia(value)).toEqual([imageUrl, imageBase64]);
  });
});
