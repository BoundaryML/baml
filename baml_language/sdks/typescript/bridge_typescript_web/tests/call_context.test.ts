import { _setCallCancellationObserverForTest, cancelFunctionCall } from "#bridge-web-native";
import { afterEach, describe, expect, it } from "vitest";

afterEach(() => {
  _setCallCancellationObserverForTest(undefined);
});

describe("Web call IDs", () => {
  it("parses the native decimal uint64 grammar without number coercion", () => {
    const cancelled: bigint[] = [];
    _setCallCancellationObserverForTest((callId) => cancelled.push(callId));
    for (const callId of ["0", "+1", "01", "9007199254740992", "18446744073709551615"]) {
      // No call carries these IDs, so nothing is cancelled; the ID still
      // reaches the runtime as the exact integer it spells.
      expect(cancelFunctionCall(callId)).toBe(false);
    }
    expect(cancelled).toEqual([0n, 1n, 1n, 9007199254740992n, 18446744073709551615n]);
  });

  it("rejects non-native and overflowing call ID spellings", () => {
    for (const callId of ["", " ", " 1", "1 ", "-0", "1.0", "0x10", "1_000", "+", "18446744073709551616"]) {
      expect(() => cancelFunctionCall(callId)).toThrow("callId must be a decimal uint64 string");
    }
    expect(() => cancelFunctionCall(18446744073709551616n)).toThrow("callId must be a decimal uint64 string");
    expect(() => cancelFunctionCall(-1n)).toThrow("callId must be a decimal uint64 string");
  });
});
