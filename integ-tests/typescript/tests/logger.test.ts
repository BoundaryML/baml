import { b } from "../baml_client";
import { setLogLevel, getLogLevel } from "../baml_client/config";

describe("Logger tests", () => {
  let originalLogLevel: string;

  // Save the original log level before each test
  beforeEach(() => {
    originalLogLevel = getLogLevel();
  });

  // Restore the original log level after each test
  afterEach(() => {
    setLogLevel(originalLogLevel);
  });

  /**
   * Helper function to capture stdout.
   * It temporarily uses jest.spyOn to override process.stdout.write,
   * calls the async function, then returns both the result and any output captured.
   */
  async function captureStdout<T>(
    fn: () => Promise<T>,
  ): Promise<{ result: T; output: string }> {
    let output = "";
    const spy = jest
      .spyOn(process.stdout, "write")
      .mockImplementation((chunk: any, encoding?: any, callback?: any) => {
        if (typeof encoding === "function") {
          callback = encoding;
          encoding = undefined;
        }
        output += chunk.toString();
        if (callback) callback();
        return true;
      });

    try {
      const result = await fn();
      return { result, output };
    } finally {
      spy.mockRestore();
    }
  }

  test("logger works as expected", async () => {
    // Test with log level "INFO"
    setLogLevel("INFO");
    expect(getLogLevel()).toBe("INFO");

    let { result, output } = await captureStdout(() =>
      b.TestOllama("banks using the word 'fiscal'"),
    );
    expect(result?.toLowerCase()).toContain("fiscal");
    expect(output).toBe("");

    // Test with log level "WARN"
    setLogLevel("WARN");
    expect(getLogLevel()).toBe("WARN");
    ({ result, output } = await captureStdout(() =>
      b.TestOllama("banks using the word 'fiscal'"),
    ));
    expect(result?.toLowerCase()).toContain("fiscal");
    expect(output).toBe("");

    // Finally, reset to "INFO" and test again
    setLogLevel("INFO");
    expect(getLogLevel()).toBe("INFO");
    ({ result, output } = await captureStdout(() =>
      b.TestOllama("banks using the word 'fiscal'"),
    ));
    expect(result?.toLowerCase()).toContain("fiscal");
    expect(output).toBe("");

    // Test with log level "OFF"
    setLogLevel("OFF");
    expect(getLogLevel()).toBe("OFF");
    ({ result, output } = await captureStdout(() =>
      b.TestOllama("banks using the word 'fiscal'"),
    ));
    expect(result?.toLowerCase()).toContain("fiscal");
    expect(output).toBe("");

    // Finally, reset to "INFO" and test again
    setLogLevel("INFO");
    expect(getLogLevel()).toBe("INFO");
    ({ result, output } = await captureStdout(() =>
      b.TestOllama("banks using the word 'fiscal'"),
    ));
    expect(result?.toLowerCase()).toContain("fiscal");
    expect(output).toBe("");
  });
});
