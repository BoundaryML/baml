import { atom, useAtomValue } from "jotai";
import { atomFamily, useAtomCallback } from "jotai/utils";
import { runtimeCtx, selectedFunctionAtom, selectedRuntimeAtom } from "../EventListener";
import React from "react";

const isRunningAtom = atom(false);
export const showTestsAtom = atom(true);

export type TestStatusType = 'queued' | 'running' | 'done' | 'error';
export type TestState = {
  status: 'queued' | 'running'
} | {
  status: 'done'
  response: any
} | {
  status: 'error'
  message: string
};


const statusAtom = atom<TestState>({ status: 'queued' });

const testStatusAtom = atomFamily((testName: string) => statusAtom);
const runningTestsAtom = atom<string[]>([]);

export const useRunHooks = () => {
  const isRunning = useAtomValue(isRunningAtom);

  const runTest = useAtomCallback(async (get, set, testNames: string[]) => {
    let runtime = get(selectedRuntimeAtom);
    let func = await get(selectedFunctionAtom);
    if (!runtime || !func) {
      // Refuse to run a test if no runtime is selected
      return;
    }
    const isRunning = get(isRunningAtom);
    if (isRunning) {
      // Refuse to run another test if one is already running
      return;
    }
    set(isRunningAtom, true);

    let ctx = await get(runtimeCtx);
    // First clear any previous test results
    testStatusAtom.setShouldRemove(() => true);
    // Remove the shouldRemove function so we don't remove future test results
    testStatusAtom.setShouldRemove(null);

    set(runningTestsAtom, testNames);
    // Batch into groups of 5
    let batches = [];
    for (let i = 0; i < testNames.length; i += 5) {
      batches.push(testNames.slice(i, i + 5));
    }
    for (let batch of batches) {
      let promises = await Promise.allSettled(batch.map((testName) => {
        set(testStatusAtom(testName), { status: 'running' });
        return func.run_test(runtime, ctx, testName);
      }));
      for (let i = 0; i < promises.length; i++) {
        let result = promises[i];
        if (result.status === 'fulfilled') {
          let res = result.value;
          set(testStatusAtom(batch[i]), { status: 'done', response: res });
        } else {
          set(testStatusAtom(batch[i]), { status: 'error', message: `${result.reason}` });
        }
      }
    }

    set(isRunningAtom, false);
  },);


  return { isRunning, run: runTest };
};
