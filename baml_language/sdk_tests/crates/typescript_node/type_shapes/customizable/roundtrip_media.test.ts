// Exact bridge coverage for all media kinds and source forms — ported from
// test_media.py. URL/base64 return functions exercise BAML-to-Node decoding;
// round-trip functions exercise Node-to-BAML encoding and decode the result.
import { baml } from "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  Media,
  return_audio,
  return_audio_base64,
  return_image,
  return_image_base64,
  return_pdf,
  return_pdf_base64,
  return_video,
  return_video_base64,
  round_trip_audio,
  round_trip_image,
  round_trip_pdf,
  round_trip_video,
  round_trip_media,
} from "./baml_sdk/media/index.js";

const URL = "https://example.com/asset";
const IMAGE_MIME = "image/png";
const AUDIO_MIME = "audio/mpeg";
const VIDEO_MIME = "video/mp4";
const PDF_MIME = "application/pdf";

const IMAGE_PAYLOAD = Buffer.from("image-payload");
const AUDIO_PAYLOAD = Buffer.from("audio-payload");
const VIDEO_PAYLOAD = Buffer.from("video-payload");
const PDF_PAYLOAD = Buffer.from("pdf-payload");

type MediaValue = {
  url(): string | null;
  file(): string | null;
  base64(): string;
  mimeType(): string | null;
};

function expectUrlMedia(
  value: MediaValue,
  url: string,
  mime: string,
): void {
  expect(value.url()).toBe(url);
  expect(value.file()).toBeNull();
  expect(value.base64()).toBe("");
  expect(value.mimeType()).toBe(mime);
}

function expectBase64Media(
  value: MediaValue,
  payload: Buffer,
  mime: string,
): void {
  const encoded = payload.toString("base64");
  expect(value.url()).toBeNull();
  expect(value.file()).toBeNull();
  expect(value.base64()).toBe(encoded);
  expect(Buffer.from(value.base64(), "base64")).toEqual(payload);
  expect(value.mimeType()).toBe(mime);
}

function expectFileMedia(
  value: MediaValue,
  file: string,
  mime: string,
): void {
  expect(value.url()).toBeNull();
  expect(value.file()).toBe(file);
  expect(value.base64()).toBe("");
  expect(value.mimeType()).toBe(mime);
}

describe("roundtrip media — decode path (return_*)", () => {
  it("test_return_image", () => {
    const value = return_image(URL, IMAGE_MIME);
    expect(value).toBeInstanceOf(baml.media.Image);
    expectUrlMedia(value, URL, IMAGE_MIME);
  });

  it("test_return_audio", () => {
    const value = return_audio(URL, AUDIO_MIME);
    expect(value).toBeInstanceOf(baml.media.Audio);
    expectUrlMedia(value, URL, AUDIO_MIME);
  });

  it("test_return_video", () => {
    const value = return_video(URL, VIDEO_MIME);
    expect(value).toBeInstanceOf(baml.media.Video);
    expectUrlMedia(value, URL, VIDEO_MIME);
  });

  it("test_return_pdf", () => {
    const value = return_pdf(URL, PDF_MIME);
    expect(value).toBeInstanceOf(baml.media.Pdf);
    expectUrlMedia(value, URL, PDF_MIME);
  });

  it("test_return_image_base64", () => {
    const value = return_image_base64(IMAGE_PAYLOAD.toString("base64"), IMAGE_MIME);
    expect(value).toBeInstanceOf(baml.media.Image);
    expectBase64Media(value, IMAGE_PAYLOAD, IMAGE_MIME);
  });

  it("test_return_audio_base64", () => {
    const value = return_audio_base64(AUDIO_PAYLOAD.toString("base64"), AUDIO_MIME);
    expect(value).toBeInstanceOf(baml.media.Audio);
    expectBase64Media(value, AUDIO_PAYLOAD, AUDIO_MIME);
  });

  it("test_return_video_base64", () => {
    const value = return_video_base64(VIDEO_PAYLOAD.toString("base64"), VIDEO_MIME);
    expect(value).toBeInstanceOf(baml.media.Video);
    expectBase64Media(value, VIDEO_PAYLOAD, VIDEO_MIME);
  });

  it("test_return_pdf_base64", () => {
    const value = return_pdf_base64(PDF_PAYLOAD.toString("base64"), PDF_MIME);
    expect(value).toBeInstanceOf(baml.media.Pdf);
    expectBase64Media(value, PDF_PAYLOAD, PDF_MIME);
  });
});

describe("roundtrip media — encode path (round_trip_*)", () => {
  it("test_round_trip_image", () => {
    const result = round_trip_image(baml.media.Image.fromUrl(URL, IMAGE_MIME));
    expect(result).toBeInstanceOf(baml.media.Image);
    expectUrlMedia(result, URL, IMAGE_MIME);
  });

  it("test_round_trip_audio", () => {
    const result = round_trip_audio(baml.media.Audio.fromUrl(URL, AUDIO_MIME));
    expect(result).toBeInstanceOf(baml.media.Audio);
    expectUrlMedia(result, URL, AUDIO_MIME);
  });

  it("test_round_trip_video", () => {
    const result = round_trip_video(baml.media.Video.fromUrl(URL, VIDEO_MIME));
    expect(result).toBeInstanceOf(baml.media.Video);
    expectUrlMedia(result, URL, VIDEO_MIME);
  });

  it("test_round_trip_pdf", () => {
    const result = round_trip_pdf(baml.media.Pdf.fromUrl(URL, PDF_MIME));
    expect(result).toBeInstanceOf(baml.media.Pdf);
    expectUrlMedia(result, URL, PDF_MIME);
  });

  it("test_round_trip_image_base64", () => {
    const value = baml.media.Image.fromBase64(
      IMAGE_PAYLOAD.toString("base64"),
      IMAGE_MIME,
    );
    const result = round_trip_image(value);
    expect(result).toBeInstanceOf(baml.media.Image);
    expectBase64Media(result, IMAGE_PAYLOAD, IMAGE_MIME);
  });

  it("test_round_trip_audio_base64", () => {
    const value = baml.media.Audio.fromBase64(
      AUDIO_PAYLOAD.toString("base64"),
      AUDIO_MIME,
    );
    const result = round_trip_audio(value);
    expect(result).toBeInstanceOf(baml.media.Audio);
    expectBase64Media(result, AUDIO_PAYLOAD, AUDIO_MIME);
  });

  it("test_round_trip_video_base64", () => {
    const value = baml.media.Video.fromBase64(
      VIDEO_PAYLOAD.toString("base64"),
      VIDEO_MIME,
    );
    const result = round_trip_video(value);
    expect(result).toBeInstanceOf(baml.media.Video);
    expectBase64Media(result, VIDEO_PAYLOAD, VIDEO_MIME);
  });

  it("test_round_trip_pdf_base64", () => {
    const value = baml.media.Pdf.fromBase64(
      PDF_PAYLOAD.toString("base64"),
      PDF_MIME,
    );
    const result = round_trip_pdf(value);
    expect(result).toBeInstanceOf(baml.media.Pdf);
    expectBase64Media(result, PDF_PAYLOAD, PDF_MIME);
  });

  it("test_round_trip_media", () => {
    const m = new Media({
      image_field: baml.media.Image.fromFile("/tmp/asset.png", IMAGE_MIME),
      audio_field: baml.media.Audio.fromFile("/tmp/asset.mp3", AUDIO_MIME),
      video_field: baml.media.Video.fromFile("/tmp/asset.mp4", VIDEO_MIME),
      pdf_field: baml.media.Pdf.fromFile("/tmp/asset.pdf", PDF_MIME),
    });
    const result = round_trip_media(m);

    expect(result).toBeInstanceOf(Media);
    expect(result.image_field).toBeInstanceOf(baml.media.Image);
    expectFileMedia(result.image_field, "/tmp/asset.png", IMAGE_MIME);
    expect(result.audio_field).toBeInstanceOf(baml.media.Audio);
    expectFileMedia(result.audio_field, "/tmp/asset.mp3", AUDIO_MIME);
    expect(result.video_field).toBeInstanceOf(baml.media.Video);
    expectFileMedia(result.video_field, "/tmp/asset.mp4", VIDEO_MIME);
    expect(result.pdf_field).toBeInstanceOf(baml.media.Pdf);
    expectFileMedia(result.pdf_field, "/tmp/asset.pdf", PDF_MIME);
  });
});
