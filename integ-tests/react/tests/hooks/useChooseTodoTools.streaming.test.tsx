import { readFileSync } from "fs";
import { join } from "path";

import { render, screen, waitFor } from "@testing-library/react";
import { useEffect, useMemo, useRef, useState } from "react";

import { useChooseTodoTools } from "../../baml_client/react/hooks";

type StreamingComponentProps = {
  query: string;
  onStreamData?: (response?: unknown) => void;
  onFinalData?: (response?: unknown) => void;
};

const StreamingComponent = ({
  query,
  onStreamData,
  onFinalData,
}: StreamingComponentProps) => {
  // Memoize options to prevent unnecessary mutate recreation
  const options = useMemo(() => ({
    stream: true as const,
    onStreamData,
    onFinalData,
  }), [onStreamData, onFinalData]);

  const { mutate, status, streamData, data, finalData, isLoading, isSuccess } =
    useChooseTodoTools(options);

  const [statusHistory, setStatusHistory] = useState<string[]>([]);
  const [streamSnapshots, setStreamSnapshots] = useState<string[]>([]);

  // Track if mutate has been called to prevent multiple calls
  const hasMutatedRef = useRef(false);

  useEffect(() => {
    setStatusHistory((prev) =>
      prev.at(-1) === status ? prev : [...prev, status],
    );
  }, [status]);

  useEffect(() => {
    if (streamData) {
      setStreamSnapshots((prev) => [...prev, JSON.stringify(streamData)]);
    }
  }, [streamData]);

  useEffect(() => {
    // Only call mutate once
    if (hasMutatedRef.current) return;
    hasMutatedRef.current = true;

    const run = async () => {
      try {
        await mutate(query);
      } catch (error) {
        // The hook surfaces errors via state; assertions read from DOM instead.
      }
    };

    void run();
  }, [mutate, query]);

  return (
    <div>
      <div data-testid="status">{status}</div>
      <div data-testid="status-history">{JSON.stringify(statusHistory)}</div>
      <div data-testid="stream-snapshots">
        {JSON.stringify(streamSnapshots)}
      </div>
      <div data-testid="final-data">{JSON.stringify(data)}</div>
      <div data-testid="final-data-length">
        {Array.isArray(finalData) ? finalData.length : 0}
      </div>
      <div data-testid="is-loading">{String(isLoading)}</div>
      <div data-testid="is-success">{String(isSuccess)}</div>
    </div>
  );
};

describe("useChooseTodoTools streaming hook", () => {
  // This test makes real API calls, so needs longer timeout
  jest.setTimeout(60000);

  it("surfaces mixed union results through streaming states", async () => {
    const onStreamData = jest.fn();
    const onFinalData = jest.fn();
    render(
      <StreamingComponent
        query="5 todo items for learning chess"
        onStreamData={onStreamData}
        onFinalData={onFinalData}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("status").textContent).toBe("success");
    }, { timeout: 30000 });

    const readJson = (testId: string) => {
      const raw = screen.getByTestId(testId).textContent ?? "null";
      try {
        return JSON.parse(raw);
      } catch {
        return null;
      }
    };

    const statusHistory = readJson("status-history") as string[];
    expect(Array.isArray(statusHistory)).toBe(true);
    expect(statusHistory[0]).toBe("idle");
    expect(statusHistory).toEqual(
      expect.arrayContaining(["pending", "success"]),
    );

    if (statusHistory.includes("streaming")) {
      expect(onStreamData).toHaveBeenCalled();
      const streamSnapshots = readJson("stream-snapshots") as unknown[];
      expect(Array.isArray(streamSnapshots)).toBe(true);
      expect(streamSnapshots.length).toBeGreaterThan(0);
    }

    const finalData = readJson("final-data") as unknown;
    expect(Array.isArray(finalData)).toBe(true);
    expect((finalData as unknown[]).length).toBeGreaterThan(0);
    expect(
      (finalData as unknown[]).every(
        (item) => typeof item === "object" && item !== null && "type" in item,
      ),
    ).toBe(true);

    expect(screen.getByTestId("is-loading").textContent).toBe("false");
    expect(screen.getByTestId("is-success").textContent).toBe("true");
    expect(onFinalData).toHaveBeenCalled();
  });
});
