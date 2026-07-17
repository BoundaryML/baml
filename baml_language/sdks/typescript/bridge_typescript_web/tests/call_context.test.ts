import {
  BamlCallContext,
  _setCallCancellationObserverForTest,
} from "#bridge-web-native";
import { afterEach, describe, expect, it } from "vitest";

afterEach(() => {
  _setCallCancellationObserverForTest(undefined);
});

describe("Web BamlCallContext", () => {
  it("parses the native decimal uint64 grammar without number coercion", () => {
    const ctx = new BamlCallContext();
    for (const callId of ["0", "+1", "01", "9007199254740992", "18446744073709551615"]) {
      expect(() => ctx._attachCallId(callId)).not.toThrow();
    }
    expect(ctx._activeCallIdsForTest()).toEqual([
      0n,
      1n,
      9007199254740992n,
      18446744073709551615n,
    ]);
  });

  it("rejects non-native and overflowing call ID spellings", () => {
    const ctx = new BamlCallContext();
    for (const callId of ["", " ", " 1", "1 ", "-0", "1.0", "0x10", "1_000", "+", "18446744073709551616"]) {
      expect(() => ctx._attachCallId(callId)).toThrow("callId must be a decimal uint64 string");
      expect(() => ctx._detachCallId(callId)).toThrow("callId must be a decimal uint64 string");
    }
  });

  it("suppresses duplicate attach, tolerates absent detach, and cancels all active IDs once", () => {
    const attempts: bigint[] = [];
    _setCallCancellationObserverForTest((callId) => attempts.push(callId));
    const ctx = new BamlCallContext();
    ctx._attachCallId("9007199254740992");
    ctx._attachCallId("9007199254740992");
    ctx._attachCallId("18446744073709551615");
    ctx._detachCallId("7");

    ctx.abort();
    expect(ctx.aborted).toBe(true);
    expect(attempts).toEqual([9007199254740992n, 18446744073709551615n]);
    ctx.abort();
    expect(attempts).toHaveLength(2);

    ctx._attachCallId("18446744073709551615");
    expect(attempts).toHaveLength(2);
    ctx._attachCallId("0");
    expect(attempts).toEqual([9007199254740992n, 18446744073709551615n, 0n]);
    ctx._detachCallId("18446744073709551615");
    ctx._attachCallId("18446744073709551615");
    expect(attempts.at(-1)).toBe(18446744073709551615n);
  });
});
