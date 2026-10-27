// Roundtrip coverage for baml_sdk/media — ported from test_media.py.
// Media values can't be hand-built as plain objects, so each value is sourced
// from the matching return_* function (built engine-side via Image.fromUrl).
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { BamlAudio, BamlImage, BamlPdf, BamlVideo } from "@boundaryml/baml-bridge";
import {
  Image,
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
  round_trip_all_media,
  round_trip_media_or_image_class,
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

// A union member that is media crosses as a media value, not a class or a
// scalar: it must come back as its own kind, never as another arm.
describe("roundtrip media — under a union", () => {
  const MEDIA_KINDS = [BamlImage, BamlAudio, BamlVideo, BamlPdf] as const;

  it("media_round_trip_all_media", () => {
    const sent = [
      return_image(URL, MIME),
      return_audio(URL, MIME),
      return_video(URL, MIME),
      return_pdf(URL, MIME),
    ] as const;
    sent.forEach((value, index) => {
      const back = round_trip_all_media(value);
      for (const [kindIndex, Kind] of MEDIA_KINDS.entries()) {
        expect(back instanceof Kind).toBe(kindIndex === index);
      }
      expect(back.url()).toBe(URL);
      expect(back.mimeType()).toBe(MIME);
    });
  });

  it("media_round_trip_media_or_image_class", () => {
    // `image | Image`: the media type and a user class whose name collides
    // with it stay distinct arms.
    const media = round_trip_media_or_image_class(return_image(URL, MIME));
    expect(media).toBeInstanceOf(BamlImage);
    expect(media).not.toBeInstanceOf(Image);
    expect((media as BamlImage).url()).toBe(URL);

    const labelled = new Image({ label: "cover" });
    const cls = round_trip_media_or_image_class(labelled);
    expect(cls).toBeInstanceOf(Image);
    expect(cls).not.toBeInstanceOf(BamlImage);
    expect(cls).toEqual(labelled);
  });
});
