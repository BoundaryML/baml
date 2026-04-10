import { atom } from 'jotai';
import { type TestState } from '../../atoms';

export interface TestHistoryEntry {
  timestamp: number;
  functionName: string;
  testName: string;
  response: TestState;
}

export const testHistoryAtom = atom<TestHistoryEntry[]>([]);
export const selectedHistoryIndexAtom = atom<number>(0);
