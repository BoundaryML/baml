import { atom, useAtomValue } from 'jotai';
import { type Diagnostic } from '@codemirror/lint';
import { WasmProject } from '@gloo-ai/baml-schema-wasm-web';
import {
  unwrap,
  atomWithStorage,
  atomFamily,
  useAtomCallback,
} from 'jotai/utils';
import { useCallback } from 'react';

// Import the lessons.json file
import lessons from '@/lib/lessons.json';
import { WasmDiagnosticError } from '@gloo-ai/baml-schema-wasm-web';
import { WasmRuntime } from '@gloo-ai/baml-schema-wasm-web';
import {
  type WasmFunctionResponse,
  type WasmTestResponse,
} from '@gloo-ai/baml-schema-wasm-web';

export type ICodeBlock = {
  code: string;
  language: 'python' | 'typescript' | 'baml';
  id: string;
};

interface Page {
  id: string;
  title: string;
  content: string;
  defaultTab: string;
  fileContents: {
    [key: string]: string;
  };
}

interface Lesson {
  id: string;
  title: string;
  pages: Page[];
}

export const lessonsAtom = atom<Lesson[]>(lessons as unknown as Lesson[]);

// Create a derived atom for initial values
const initialValuesAtom = atom((get) => {
  const lessons = get(lessonsAtom);
  const firstLesson = lessons[0];
  const firstPage = firstLesson?.pages[0];
  return {
    firstLesson,
    firstPage,
    defaultTabId: firstPage?.defaultTab || '',
  };
});

const lessonAndPageIdAtomBase = atom({
  lessonIndex: 0,
  pageIndex: 0,
  tabId: '', // Will be initialized properly in the derived atom
});

export const lessonAndPageIdAtom = atom(
  (get) => {
    const baseValue = get(lessonAndPageIdAtomBase);
    // If tabId is empty (initial state), set it to the default
    if (!baseValue.tabId) {
      const { defaultTabId } = get(initialValuesAtom);
      return {
        ...baseValue,
        tabId: defaultTabId,
      };
    }
    return baseValue;
  },
  (
    get,
    set,
    update: { lessonIndex: number; pageIndex: number; tabId: string },
  ) => {
    const lessons = get(lessonsAtom);

    // Auto-correct indices to ensure they're always valid
    const safeLessonIndex = Math.max(
      0,
      Math.min(update.lessonIndex, lessons.length - 1),
    );
    const lesson = lessons[safeLessonIndex];
    const safePageIndex = lesson
      ? Math.max(0, Math.min(update.pageIndex, lesson.pages.length - 1))
      : 0;
    const page = lesson?.pages[safePageIndex];

    // Ensure tab ID exists in the current page, fall back to defaultTab if not
    const availableTabs = page?.fileContents
      ? Object.keys(page.fileContents)
      : [];
    const safeTabId = availableTabs.includes(update.tabId)
      ? update.tabId
      : page?.defaultTab || availableTabs[0] || '';

    set(lessonAndPageIdAtomBase, {
      lessonIndex: safeLessonIndex,
      pageIndex: safePageIndex,
      tabId: safeTabId,
    });
  },
);

// Add a safe atom that ensures valid indices
export const safeCurrentIndicesAtom = atom((get) => {
  const lessons = get(lessonsAtom);
  const { lessonIndex, pageIndex, tabId } = get(lessonAndPageIdAtom);

  // Ensure lesson index is valid
  const safeLessonIndex = Math.max(
    0,
    Math.min(lessonIndex, lessons.length - 1),
  );
  const lesson = lessons[safeLessonIndex];

  // Ensure page index is valid for the current lesson
  const safePageIndex = lesson
    ? Math.max(0, Math.min(pageIndex, lesson.pages.length - 1))
    : 0;
  const page = lesson?.pages[safePageIndex];

  // Ensure tab ID exists in the current page, fall back to defaultTab if not
  const availableTabs = page?.fileContents
    ? Object.keys(page.fileContents)
    : [];
  const safeTabId = availableTabs.includes(tabId)
    ? tabId
    : page?.defaultTab || availableTabs[0] || '';

  return {
    lessonIndex: safeLessonIndex,
    pageIndex: safePageIndex,
    tabId: safeTabId,
    lesson,
    page,
  };
});

export const visibleFileAtom = atom((get): ICodeBlock => {
  const { lessonIndex, pageIndex, tabId, lesson, page } = get(
    safeCurrentIndicesAtom,
  );
  console.log('lessonIndex', lessonIndex);
  console.log('pageIndex', pageIndex);
  console.log('tabId', tabId);
  const code =
    page?.fileContents?.[tabId as keyof typeof page.fileContents] || '';
  console.log('code', code);
  const language = getLanguageFromFilename(tabId);

  return {
    code,
    language,
    id: tabId,
  };
});

// Derived atom that contains the actual lesson and page objects
export const currentLessonAndPage = atom((get) => {
  const { lessonIndex, pageIndex, tabId, lesson, page } = get(
    safeCurrentIndicesAtom,
  );

  return {
    lesson,
    page,
    tabId,
  };
});

// Helper function to determine language from filename
function getLanguageFromFilename(
  filename: string,
): 'python' | 'typescript' | 'baml' {
  const extension = filename.split('.').pop()?.toLowerCase();

  switch (extension) {
    case 'py':
      return 'python';
    case 'ts':
    case 'tsx':
    case 'js':
    case 'jsx':
      return 'typescript';
    case 'baml':
      return 'baml';
    default:
      return 'baml'; // default to baml for unknown extensions
  }
}

// Derived atom that creates an ICodeBlock from the current tab
export const currentCodeBlock = atom((get): ICodeBlock => {
  const { page, tabId } = get(currentLessonAndPage);

  const code =
    page?.fileContents?.[tabId as keyof typeof page.fileContents] || '';
  const language = getLanguageFromFilename(tabId);

  return {
    code,
    language,
    id: tabId,
  };
});

export const projectAtom = atom((get) => {
  const wasm = get(wasmAtom);
  const lessons = get(lessonsAtom);
  if (!wasm) return null;
  const lessonAndPage = get(currentLessonAndPage);
  const bamlFiles = Object.entries(lessonAndPage.page?.fileContents || {});
  return wasm?.WasmProject.new('./', bamlFiles);
});

export const diagnosticsAtom = atom((get) => {
  const runtime = get(runtimeAtom);
  // console.log("diags:")
  // console.log(runtime.diags)
  return runtime.diags?.errors() ?? [];
});

export const CodeMirrorDiagnosticsAtom = atom((get) => {
  const diags = get(diagnosticsAtom);
  return diags.map((d): Diagnostic => {
    return {
      from: d.start_ch,
      to: d.start_ch === d.end_ch ? d.end_ch + 1 : d.end_ch,
      message: d.message,
      severity: d.type === 'warning' ? 'warning' : 'error',
      source: 'baml',
      markClass:
        d.type === 'error'
          ? 'decoration-wavy decoration-red-500 text-red-450 stroke-blue-500'
          : 'decoration-wavy decoration-yellow-500 text-yellow-450 stroke-blue-500',
    };
  });
});

const wasmAtomAsync = atom(async () => {
  const wasm = await import('@gloo-ai/baml-schema-wasm-web/baml_schema_build');
  return wasm;
});

export const wasmAtom = unwrap(wasmAtomAsync);

export const useWaitForWasm = () => {
  const wasm = useAtomValue(wasmAtom);
  return wasm !== undefined;
};

export const envVarsAtom = atomWithStorage<{
  [key: string]: string | undefined;
}>('baml-env-vars', {
  BOUNDARY_PROXY_URL: 'https://fiddle-proxy.fly.dev',
});

export const ctxAtom = atom((get) => {
  const wasm = get(wasmAtom);
  if (wasm === undefined) {
    return undefined;
  }
  return new wasm.WasmCallContext();
});

export const runtimeAtom = atom((get) => {
  const wasm = get(wasmAtom);
  const project = get(projectAtom);
  const envVars = get(envVarsAtom);
  if (wasm === undefined || project === undefined) {
    return { rt: undefined, diags: undefined };
  }
  try {
    const selectedEnvVars = Object.fromEntries(
      Object.entries(envVars).filter(([key, value]) => value !== undefined),
    );
    const rt = project?.runtime(selectedEnvVars);
    if (rt !== undefined) {
      const diags = project?.diagnostics(rt);
      return { rt, diags };
    } else {
      return { rt: undefined, diags: undefined };
    }
  } catch (e) {
    const WasmDiagnosticError = wasm.WasmDiagnosticError;
    if (e instanceof Error) {
      console.error(e.message);
    } else if (e instanceof WasmDiagnosticError) {
      return { rt: undefined, diags: e };
    } else {
      console.error(e);
    }
  }
  return { rt: undefined, diags: undefined };
});

// Test-related atoms for OutputPanel
export type TestStatusType = 'queued' | 'running' | 'done' | 'error' | 'idle';
export type DoneTestStatusType =
  | 'passed'
  | 'llm_failed'
  | 'parse_failed'
  | 'constraints_failed'
  | 'assert_failed'
  | 'error';
export type TestState =
  | {
      status: 'queued' | 'idle';
    }
  | {
      status: 'running';
      response?: WasmFunctionResponse;
    }
  | {
      status: 'done';
      response_status: DoneTestStatusType;
      response: WasmTestResponse;
      latency_ms: number;
    }
  | {
      status: 'error';
      message: string;
    };

export interface TestHistoryEntry {
  timestamp: number;
  functionName: string;
  testName: string;
  response: TestState;
}

export const testHistoryAtom = atom<TestHistoryEntry[]>([]);
export const selectedHistoryIndexAtom = atom<number>(0);
export const areTestsRunningAtom = atom(false);

export const testCaseResponseAtom = atomFamily(
  (params: { functionName: string; testName: string }) =>
    atom((get) => {
      const testHistory = get(testHistoryAtom);
      const testCaseResponse = testHistory.find(
        (t) =>
          t.functionName === params.functionName &&
          t.testName === params.testName,
      );
      return testCaseResponse?.response;
    }),
);

// Learn-specific test runner hook
export const useRunTests = (maxBatchSize = 5) => {
  const { rt } = useAtomValue(runtimeAtom);
  const ctx = useAtomValue(ctxAtom);
  const wasm = useAtomValue(wasmAtom);

  const runTests = useAtomCallback(
    useCallback(
      async (get, set, tests: { functionName: string; testName: string }[]) => {
        if (!rt || !ctx || !wasm) {
          console.error('Missing required dependencies for test running');
          return;
        }

        // Add tests to history
        tests.forEach((test) => {
          set(testHistoryAtom, (prev) => [
            {
              timestamp: Date.now(),
              functionName: test.functionName,
              testName: test.testName,
              response: { status: 'running' },
            },
            ...prev,
          ]);
        });

        set(selectedHistoryIndexAtom, 0);

        const setState = (
          test: { functionName: string; testName: string },
          update: TestState,
        ) => {
          set(testHistoryAtom, (prev) => {
            const testIndex = prev.findIndex(
              (t) =>
                t.functionName === test.functionName &&
                t.testName === test.testName,
            );
            if (testIndex === -1) return prev;

            const newHistory = [...prev];
            newHistory[testIndex] = {
              ...newHistory[testIndex],
              response: update,
              timestamp: Date.now(),
              functionName: test.functionName,
              testName: test.testName,
            };
            return newHistory;
          });
          set(selectedHistoryIndexAtom, 0);
        };

        const runTest = async (test: {
          functionName: string;
          testName: string;
        }) => {
          console.log('Running test:', test);

          // Find the function and test case
          const functions = rt.list_functions();
          const fn = functions.find((f) => f.name === test.functionName);
          const tc = fn?.test_cases.find((tc) => tc.name === test.testName);

          if (!fn || !tc) {
            console.log('Test case not found:', test);
            setState(test, { status: 'error', message: 'Test case not found' });
            return;
          }

          const startTime = performance.now();
          setState(test, { status: 'running' });

          try {
            const result = await fn.run_test(
              rt,
              tc.name,
              (partial: WasmFunctionResponse) => {
                setState(test, { status: 'running', response: partial });
              },
              // Simple media file finder for learn mode
              (path: string) => {
                console.log('Media file requested:', path);
                return undefined;
              },
              // Environment variables for learn mode
              { BOUNDARY_PROXY_URL: 'https://fiddle-proxy.fly.dev' },
            );

            const endTime = performance.now();
            const response_status = result.status();
            const responseStatusMap: Record<number, DoneTestStatusType> = {
              [wasm.TestStatus.Passed]: 'passed',
              [wasm.TestStatus.LLMFailure]: 'llm_failed',
              [wasm.TestStatus.ParseFailure]: 'parse_failed',
              [wasm.TestStatus.ConstraintsFailed]: 'constraints_failed',
              [wasm.TestStatus.AssertFailed]: 'assert_failed',
              [wasm.TestStatus.UnableToRun]: 'error',
              [wasm.TestStatus.FinishReasonFailed]: 'error',
            } as const;

            setState(test, {
              status: 'done',
              response: result,
              response_status: responseStatusMap[response_status] || 'error',
              latency_ms: endTime - startTime,
            });
          } catch (e) {
            setState(test, {
              status: 'error',
              message: e instanceof Error ? e.message : 'Unknown error',
            });
          }
        };

        const run = async () => {
          // Create batches of tests to run
          const batches: { functionName: string; testName: string }[][] = [];
          for (let i = 0; i < tests.length; i += maxBatchSize) {
            batches.push(tests.slice(i, i + maxBatchSize));
          }

          // Run each batch
          for (const batch of batches) {
            await Promise.allSettled(
              batch.map(async (test) => {
                setState(test, { status: 'queued' });
                await runTest(test);
              }),
            );
          }
        };

        set(areTestsRunningAtom, true);
        await run().finally(() => {
          set(areTestsRunningAtom, false);
        });
      },
      [maxBatchSize, rt, ctx, wasm],
    ),
  );

  return { setRunningTests: runTests };
};
