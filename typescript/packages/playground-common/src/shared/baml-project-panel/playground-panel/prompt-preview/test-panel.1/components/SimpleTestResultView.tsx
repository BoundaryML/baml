import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@baml/ui/collapsible';
import { cn } from '@baml/ui/lib/utils';
import { useAtomValue } from 'jotai';
import { ChevronDown, ChevronUp } from 'lucide-react';
import { useState } from 'react';
import { testCaseResponseAtom, TestState, DoneTestStatusType } from '../../../atoms';
import { RenderPromptPart } from '../../render-text';
import type { TestHistoryEntry, TestHistoryRun } from '../atoms';
import { TestResultStats } from './TestResultStats';

// Constants
const TRUNCATE_LENGTH = 100;

// Status colors mapping all possible states
const STATUS_COLORS = {
  idle: 'border-[var(--vscode-charts-gray)]',
  queued: 'border-[var(--vscode-charts-yellow)]',
  running: 'border-[var(--vscode-charts-yellow)]',
  error: 'border-[var(--vscode-charts-red)]',
  passed: 'border-[var(--vscode-charts-green)]',
  llm_failed: 'border-[var(--vscode-charts-red)]',
  parse_failed: 'border-[var(--vscode-charts-orange)]',
  constraints_failed: 'border-[var(--vscode-charts-orange)]',
  assert_failed: 'border-[var(--vscode-charts-red)]',
} as const;

// Status text mapping
const STATUS_TEXT = {
  idle: 'Idle',
  queued: 'Queued',
  running: 'Running',
  error: 'Error',
  passed: 'Passed',
  llm_failed: 'LLM Failed',
  parse_failed: 'Parse Failed',
  constraints_failed: 'Constraints Failed',
  assert_failed: 'Assert Failed',
} as const;

// Types
interface TestResultViewProps {
  run?: TestHistoryRun;
}

interface TestResultMessageProps {
  test?: TestState;
}

type TestStatus = keyof typeof STATUS_COLORS;

// Utility functions
const getTestStatus = (response: TestState | undefined): TestStatus => {
  if (!response) return 'idle';

  if (response.status === 'idle') return 'idle';
  if (response.status === 'queued') return 'queued';
  if (response.status === 'running') return 'running';
  if (response.status === 'error') return 'error';
  if (response.status === 'done') {
    return response.response_status;
  }

  return 'idle';
};

const getResponseContent = (response: TestState | undefined): string => {
  if (!response) return 'No response';

  if (response.status === 'idle') {
    return 'Ready to run';
  }

  if (response.status === 'queued') {
    return 'Waiting to run...';
  }

  if (response.status === 'running') {
    // Check if we have a partial response
    if (response.response?.llm_response()) {
      return response.response.llm_response()?.content || 'Running...';
    }
    return 'Running...';
  }

  if (response.status === 'error') {
    return response.message || 'Unknown error';
  }

  if (response.status === 'done') {
    const wasmResponse = response.response;

    switch (response.response_status) {
      case 'passed':
        // Try to get the parsed response first, then LLM response
        const parsedResponse = wasmResponse.parsed_response();
        if (parsedResponse) {
          return parsedResponse.value;
        }
        const llmResponse = wasmResponse.llm_response();
        if (llmResponse) {
          return llmResponse.content;
        }
        return 'Test passed';

      case 'llm_failed':
        const llmFailure = wasmResponse.llm_failure();
        if (llmFailure) {
          return llmFailure.message;
        }
        const failureMessage = wasmResponse.failure_message();
        if (failureMessage) {
          return failureMessage;
        }
        return 'LLM request failed';

      case 'parse_failed':
        const parseFailureMsg = wasmResponse.failure_message();
        if (parseFailureMsg) {
          return parseFailureMsg;
        }
        // Try to show the raw LLM response if available
        const rawLlmResponse = wasmResponse.llm_response();
        if (rawLlmResponse) {
          return `Parse failed. Raw response: ${rawLlmResponse.content}`;
        }
        return 'Failed to parse response';

      case 'constraints_failed':
        const constraintsFailureMsg = wasmResponse.failure_message();
        if (constraintsFailureMsg) {
          return constraintsFailureMsg;
        }
        return 'Constraints validation failed';

      case 'assert_failed':
        const assertFailureMsg = wasmResponse.failure_message();
        if (assertFailureMsg) {
          return assertFailureMsg;
        }
        return 'Assertion failed';

      case 'error':
        const errorMsg = wasmResponse.failure_message();
        if (errorMsg) {
          return errorMsg;
        }
        return 'Test execution error';

      default:
        return 'Unknown status';
    }
  }

  return 'No response';
};

const truncateFirstLine = (content: string): string => {
  if (!content) return '';

  const firstLine = content.split('\n')[0];
  if (!firstLine) return '';

  return firstLine.length > TRUNCATE_LENGTH
    ? firstLine.slice(0, TRUNCATE_LENGTH) + '...'
    : firstLine;
};

const getLatencyInfo = (response: TestState | undefined): string | undefined => {
  if (response?.status === 'done') {
    return `${response.latency_ms}ms`;
  }
  return undefined;
};

// Custom hook for test response logic
const useTestResponse = (test?: TestHistoryEntry) => {
  const currentResponse = useAtomValue(testCaseResponseAtom({ functionName: test?.functionName, testName: test?.testName }));
  const displayResponse = test?.response || currentResponse;

  const status = getTestStatus(displayResponse);
  const content = getResponseContent(displayResponse);
  const firstLine = truncateFirstLine(content);
  const borderColor = STATUS_COLORS[status];
  const statusText = STATUS_TEXT[status];
  const latency = getLatencyInfo(displayResponse);

  return {
    displayResponse,
    status,
    content,
    firstLine,
    borderColor,
    statusText,
    latency,
    hasError: status === 'error' || status === 'llm_failed' || status === 'assert_failed',
    isSuccess: status === 'passed',
    isWarning: status === 'parse_failed' || status === 'constraints_failed',
  };
};

// Sub-components
const TestStatusHeader: React.FC<{
  test: TestHistoryEntry;
  statusText: string;
  firstLine: string;
  latency?: string;
  isOpen: boolean;
}> = ({ test, statusText, firstLine, latency, isOpen }) => (
  <div className="flex flex-col items-start gap-1 flex-1 overflow-hidden min-w-0">
    <div className="flex items-center w-full justify-between">
      <div className="flex items-center gap-2">
        <div className="text-xs text-muted-foreground">
          {statusText}
        </div>
        <div className="text-xs font-mono text-muted-foreground/70">
          {test.functionName}.{test.testName}
        </div>
        {latency && (
          <div className="text-xs text-muted-foreground/50">
            ({latency})
          </div>
        )}
      </div>
      {isOpen ? (
        <ChevronUp className="size-4 ml-4 flex-shrink-0" />
      ) : (
        <ChevronDown className="size-4 ml-4 flex-shrink-0" />
      )}
    </div>
    {!isOpen && firstLine && (
      <div className="text-sm truncate whitespace-nowrap w-full text-left">
        {firstLine}
      </div>
    )}
  </div>
);

const TestContent: React.FC<{
  content: string;
  hasError: boolean;
  isWarning: boolean;
}> = ({ content, hasError, isWarning }) => (
  <div className="p-0">
    {hasError ? (
      <div className="p-3 text-red-500 bg-red-50 dark:bg-red-950/20 rounded">
        <pre className="whitespace-pre-wrap text-xs">
          {content}
        </pre>
      </div>
    ) : isWarning ? (
      <div className="p-3 text-orange-600 bg-orange-50 dark:bg-orange-950/20 rounded">
        <pre className="whitespace-pre-wrap text-xs">
          {content}
        </pre>
      </div>
    ) : (
      <RenderPromptPart text={content} />
    )}
  </div>
);

// Main components
const TestResultMessage: React.FC<{
  test: TestHistoryEntry;
}> = ({
  test,
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const {
    displayResponse,
    content,
    firstLine,
    borderColor,
    statusText,
    latency,
    hasError,
    isWarning,
  } = useTestResponse(test);

  if (!displayResponse) {
    return null;
  }

  return (
    <div className={cn('border-l-4 pl-2 rounded mb-4', borderColor)}>
      <Collapsible open={isOpen} onOpenChange={setIsOpen}>
        <CollapsibleTrigger
          className={cn(
            'flex w-full items-center justify-between p-3 transition-colors',
            'data-[state=closed]:bg-card rounded-t data-[state=closed]:hover:bg-card/80 cursor-pointer data-[state=open]:hover:bg-card/80',
          )}
        >
          <TestStatusHeader
            test={test}
            statusText={statusText}
            firstLine={firstLine}
            isOpen={isOpen}
          />
        </CollapsibleTrigger>
        <CollapsibleContent className="space-y-3">
          <TestContent content={content} hasError={hasError} isWarning={isWarning} />
        </CollapsibleContent>
      </Collapsible>
      <TestResultStats response={displayResponse} />
    </div>
  );
};

export const TestResultView = ({ run }: { run: TestHistoryRun }) => {
  if (!run?.tests.length) {
    return (
      <div className="p-4 text-center text-muted-foreground">
        No test results to display
      </div>
    );
  }

  return (
    <div className="space-y-0">
      {run.tests.map((test, index) => (
        <TestResultMessage
          key={`${test.functionName}-${test.testName}-${index}`}
          test={test}
        />
      ))}
    </div>
  );
};

