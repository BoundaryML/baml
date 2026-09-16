export type TestRuntime = "node" | "web" | "workers";

declare global {
  interface ImportMeta {
    readonly env: { BAML_TEST_RUNTIME?: string };
  }
}

const configuredRuntime =
  import.meta.env.BAML_TEST_RUNTIME ?? process.env.BAML_TEST_RUNTIME;

if (
  configuredRuntime !== "node" &&
  configuredRuntime !== "web" &&
  configuredRuntime !== "workers"
) {
  throw new Error(
    `unknown BAML_TEST_RUNTIME: ${configuredRuntime ?? "<unset>"}`,
  );
}

export const testRuntime: TestRuntime = configuredRuntime;

export function isTestRuntime(...runtimes: TestRuntime[]): boolean {
  return runtimes.includes(testRuntime);
}
