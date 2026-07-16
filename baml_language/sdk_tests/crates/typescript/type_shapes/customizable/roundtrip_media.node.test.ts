// Roundtrip coverage for baml_sdk/media — ported from test_media.py.
// Media values can't be hand-built as plain objects, so each value is sourced
// from the matching return_* function (built engine-side via Image.fromUrl).
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
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

describe("roundtrip media — decode path (return_*)", () => {
  it("return_image", () => expect(return_image(URL, null)).not.toBeNull());
  it("return_audio", () => expect(return_audio(URL, null)).not.toBeNull());
  it("return_video", () => expect(return_video(URL, null)).not.toBeNull());
  it("return_pdf", () => expect(return_pdf(URL, null)).not.toBeNull());
});

describe("roundtrip media — encode path (round_trip_*)", () => {
  it("round_trip_image", () => expect(round_trip_image(return_image(URL, null))).not.toBeNull());
  it("round_trip_audio", () => expect(round_trip_audio(return_audio(URL, null))).not.toBeNull());
  it("round_trip_video", () => expect(round_trip_video(return_video(URL, null))).not.toBeNull());
  it("round_trip_pdf", () => expect(round_trip_pdf(return_pdf(URL, null))).not.toBeNull());

  it("round_trip_media", () => {
    const m = new Media({
      image_field: return_image(URL, null),
      audio_field: return_audio(URL, null),
      video_field: return_video(URL, null),
      pdf_field: return_pdf(URL, null),
    });
    expect(round_trip_media(m)).not.toBeNull();
  });
});
