import type { WasmFunctionResponse } from '@gloo-ai/baml-schema-wasm-web';
import { useAtomValue } from 'jotai';
import { useAtomCallback } from 'jotai/utils';
import { useCallback } from 'react';
import { ctxAtom, runtimeAtom, wasmAtom } from '../../../atoms';
import {
  type DoneTestStatusType,
  type TestState,
  areTestsRunningAtom,
  testCaseAtom,
} from '../../atoms';
import { findMediaFile } from '../media-utils';
import { selectedHistoryIndexAtom, testHistoryAtom } from './atoms';

export const useRunTests = (maxBatchSize = 5) => {
  const { rt } = useAtomValue(runtimeAtom);
  const ctx = useAtomValue(ctxAtom);
  const wasm = useAtomValue(wasmAtom);

  const runTests = useAtomCallback(
    useCallback(
      async (get, set, tests: { functionName: string; testName: string }[]) => {
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
          const testCase = get(testCaseAtom(test));
          if (!rt || !ctx || !testCase || !wasm) {
            setState(test, {
              status: 'error',
              message: 'Missing required dependencies',
            });
            return;
          }

          const startTime = performance.now();
          setState(test, { status: 'running' });

          try {
            const result = await testCase.fn.run_test(
              rt,
              testCase.tc.name,
              (partial: WasmFunctionResponse) => {
                setState(test, { status: 'running', response: partial });
              },
              findMediaFile,
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
