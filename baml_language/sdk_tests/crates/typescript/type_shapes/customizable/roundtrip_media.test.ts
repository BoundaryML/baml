// Roundtrip coverage for baml_sdk/media — ported from test_media.py.
// Media values can't be hand-built as plain objects, so each value is sourced
// from the matching return_* function (built engine-side via Image.fromUrl).
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { BamlAudio, BamlImage, BamlPdf, BamlVideo } from "@boundaryml/baml-bridge";
import {
  Media,
  return_image,
  return_audio,
  return_video,
  return_pdf,
  round_trip_image,
  round_trip_audio,
  round_trip_video,
  round_trip_pdf,
  round_trip_media,
} from "./baml_sdk/media/index.js";

const URL = "https://example.com/asset";
const MIME = "application/test";

describe("roundtrip media — decode path (return_*)", () => {
  it("media_uses_the_runtime_owned_wrapper_constructors", () => {
    expect(return_image(URL, MIME)).toBeInstanceOf(BamlImage);
    expect(return_audio(URL, MIME)).toBeInstanceOf(BamlAudio);
    expect(return_video(URL, MIME)).toBeInstanceOf(BamlVideo);
    expect(return_pdf(URL, MIME)).toBeInstanceOf(BamlPdf);
  });

  it("media_return_image_preserves_url_and_mime", () => {
    expect(return_image(URL, MIME).url()).toBe(URL);
    expect(return_image(URL, MIME).mimeType()).toBe(MIME);
  });
  it("media_return_audio_preserves_url_and_mime", () => {
    expect(return_audio(URL, MIME).url()).toBe(URL);
    expect(return_audio(URL, MIME).mimeType()).toBe(MIME);
  });
  it("media_return_video_preserves_url_and_mime", () => {
    expect(return_video(URL, MIME).url()).toBe(URL);
    expect(return_video(URL, MIME).mimeType()).toBe(MIME);
  });
  it("media_return_pdf_preserves_url_and_mime", () => {
    expect(return_pdf(URL, MIME).url()).toBe(URL);
    expect(return_pdf(URL, MIME).mimeType()).toBe(MIME);
  });
});

describe("roundtrip media — encode path (round_trip_*)", () => {
  it("media_round_trip_image_can_reuse_the_same_wrapper", () => {
    const value = return_image(URL, MIME);
    expect(round_trip_image(value).url()).toBe(URL);
    expect(round_trip_image(value).mimeType()).toBe(MIME);
  });

  it("media_round_trip_audio_preserves_url_and_mime", () => {
    const value = round_trip_audio(return_audio(URL, MIME));
    expect(value.url()).toBe(URL);
    expect(value.mimeType()).toBe(MIME);
  });
  it("media_round_trip_video_preserves_url_and_mime", () => {
    const value = round_trip_video(return_video(URL, MIME));
    expect(value.url()).toBe(URL);
    expect(value.mimeType()).toBe(MIME);
  });
  it("media_round_trip_pdf_preserves_url_and_mime", () => {
    const value = round_trip_pdf(return_pdf(URL, MIME));
    expect(value.url()).toBe(URL);
    expect(value.mimeType()).toBe(MIME);
  });

  it("media_round_trip_media_preserves_all_four_media_fields", () => {
    const value = round_trip_media(new Media({
      image_field: return_image(URL, MIME),
      audio_field: return_audio(URL, MIME),
      video_field: return_video(URL, MIME),
      pdf_field: return_pdf(URL, MIME),
    }));
    for (const field of [value.image_field, value.audio_field, value.video_field, value.pdf_field]) {
      expect(field.url()).toBe(URL);
      expect(field.mimeType()).toBe(MIME);
    }
  });
});
