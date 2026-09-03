import { describe, expect, it } from "vitest"

import { extractAnchor, parseTrackMetadata } from "./markers"

describe("extractAnchor", () => {
  it("extracts source while preserving a replacement seam", () => {
    const source = "// TRACK: nightly\n// ANCHOR: add\nfunction Add() -> int { 2 + 2 }\n// ANCHOR_END: add\n"
    const result = extractAnchor(source, "add")
    expect(result.code).toBe("function Add() -> int { 2 + 2 }")
    expect(`${result.sourceBefore}${result.code}${result.sourceAfter}`).toBe(source)
  })

  it.each([
    ["missing", "// ANCHOR: other\nx\n// ANCHOR_END: other", "Missing anchor"],
    ["empty", "// ANCHOR: empty\n\n// ANCHOR_END: empty", "is empty"],
    ["nested", "// ANCHOR: a\n// ANCHOR: b\nx\n// ANCHOR_END: b\n// ANCHOR_END: a", "Nested anchor"],
    ["mismatched", "// ANCHOR: a\nx\n// ANCHOR_END: b", "is closed by"],
    ["duplicate", "// ANCHOR: a\nx\n// ANCHOR_END: a\n// ANCHOR: a\ny\n// ANCHOR_END: a", "Duplicate anchor"],
  ])("rejects %s anchors", (_name, source, message) => {
    expect(() => extractAnchor(source, _name === "missing" ? "wanted" : _name === "empty" ? "empty" : "a")).toThrow(message)
  })
})

describe("parseTrackMetadata", () => {
  it("defaults to canary", () => {
    expect(parseTrackMetadata("function A() -> int { 1 }")).toEqual({ declaredTrack: "canary" })
  })

  it("normalizes leading track controls", () => {
    expect(parseTrackMetadata("// TRACK: NIGHTLY\n// NO_TRACK: HEAD\n// NO_TRACK_REASON: Not ready yet.\nfunction A() -> int { 1 }")).toEqual({
      declaredTrack: "nightly",
      excludedTrack: "head",
      exclusionReason: "Not ready yet.",
    })
  })

  it("ignores controls after the first declaration", () => {
    expect(parseTrackMetadata("function A() -> int { 1 }\n// TRACK: head")).toEqual({ declaredTrack: "canary" })
  })

  it.each([
    ["duplicate keys", "// TRACK: canary\n// TRACK: head\nfunction A() -> int { 1 }", "Duplicate TRACK"],
    ["unknown tracks", "// TRACK: stable\nfunction A() -> int { 1 }", "must be one of"],
    ["orphan reasons", "// NO_TRACK_REASON: soon\nfunction A() -> int { 1 }", "requires NO_TRACK"],
    ["missing reasons", "// NO_TRACK: head\nfunction A() -> int { 1 }", "requires a non-empty"],
    ["own-track exclusions", "// TRACK: head\n// NO_TRACK: head\n// NO_TRACK_REASON: no\nfunction A() -> int { 1 }", "cannot exclude"],
  ])("rejects %s", (_name, source, message) => {
    expect(() => parseTrackMetadata(source)).toThrow(message)
  })
})
