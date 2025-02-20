import { config } from 'dotenv'
import { ClientRegistry, BamlValidationError } from '@boundaryml/baml'
import { b } from '../baml_client'
import { b as b_sync } from '../baml_client/sync_client'
import { DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME, resetBamlEnvVars } from '../baml_client/globals'
import { ReadableStream, ReadableStreamDefaultController } from 'node:stream/web';
import { TextEncoder, TextDecoder } from 'util';
import '@testing-library/jest-dom';

config()

beforeAll(() => {
  // Add any global setup here
})

afterAll(() => {
  DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
})

// Add web stream APIs to global scope for tests
Object.assign(global, {
  ReadableStream,
  ReadableStreamDefaultController,
  TextEncoder,
  TextDecoder
});

// Mock TextEncoder/TextDecoder if not available
if (typeof global.TextEncoder === 'undefined') {
  global.TextEncoder = TextEncoder;
}
if (typeof global.TextDecoder === 'undefined') {
  global.TextDecoder = TextDecoder as typeof global.TextDecoder;
}

// Mock React hooks environment
const mockDispatch = jest.fn();

type ReducerState = {
  data: unknown;
  partialData: unknown;
  isLoading: boolean;
  isError: boolean;
  isSuccess: boolean;
  error: Error | null;
  status: 'idle' | 'loading' | 'success' | 'error';
};

type ReducerAction = {
  type: 'start' | 'success' | 'error' | 'partial';
  payload?: unknown;
  error?: Error;
};

type Reducer = (state: ReducerState, action: ReducerAction) => ReducerState;

// Mock React with dynamic data based on the hook being used
jest.mock('react', () => {
  const originalModule = jest.requireActual('react');

  return {
    ...originalModule,
    useReducer: (_reducer: Reducer, initialState: ReducerState) => {
      // Return the initial state for the first render
      return [initialState, mockDispatch];
    }
  };
});

// Mock server actions
jest.mock('../baml_client/react/server', () => ({
  TestAwsAction: jest.fn(async (input: string, options?: { stream?: boolean }) => {
    if (options?.stream) {
      return new ReadableStream({
        start(controller) {
          controller.enqueue(new TextEncoder().encode(JSON.stringify({ partial: 'test response' })));
          controller.enqueue(new TextEncoder().encode(JSON.stringify({ final: 'test response' })));
          controller.close();
        }
      });
    }
    return 'test response';
  }),
  TestUniverseQuestionAction: jest.fn(async (input: { question: string }, options?: { stream?: boolean }) => {
    const response = { answer: 'test answer', confidence: 0.9 };
    if (options?.stream) {
      return new ReadableStream({
        start(controller) {
          controller.enqueue(new TextEncoder().encode(JSON.stringify({ partial: response })));
          controller.enqueue(new TextEncoder().encode(JSON.stringify({ final: response })));
          controller.close();
        }
      });
    }
    return response;
  })
}));

export {
  b,
  b_sync,
  ClientRegistry,
  BamlValidationError,
  resetBamlEnvVars,
  DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME,
}
