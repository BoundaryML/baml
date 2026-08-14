import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import type {
  WasmFunctionResponse,
  WasmLLMFailure,
  WasmLLMResponse,
  WasmTestResponse,
} from '@gloo-ai/baml-schema-wasm-web';
import { useAtom, useAtomValue } from 'jotai';
import { Brain, ChevronDown, Clock, Cpu } from 'lucide-react';
import { ChevronUp } from 'lucide-react';
import { AlertCircle, CheckCircle, XCircle } from 'lucide-react';
import { useState } from 'react';
import {
  type DoneTestStatusType,
  type TestState,
  testCaseResponseAtom,
} from '../../atoms';
import { FunctionTestName } from '../../function-test-name';
import RenderText from '../LongText';
import { Loader } from '../components';
import { selectedHistoryIndexAtom, testHistoryAtom } from './atoms';

interface TestId {
  functionName: string;
  testName: string;
}

const TestPanel = () => {
  const [selectedHistoryIndex, setSelectedHistoryIndex] = useAtom(
    selectedHistoryIndexAtom,
  );
  const testHistory = useAtomValue(testHistoryAtom);

  // forgive me for this any
  const getHistoryButtonColor = (test: any, isSelected: boolean) => {
    const status = test.response?.status as TestState['status'];
    const finalState = test.response?.response_status as DoneTestStatusType;

    const baseClasses = isSelected
      ? {
          running: 'bg-blue-300',
          error: 'bg-red-300 text-red-950',
          success: 'bg-green-300 text-green-950',
          warning: 'bg-yellow-300 text-yellow-950',
          default: 'bg-gray-300 text-gray-950',
        }
      : {
          running: 'border-blue-200 border hover:bg-blue-100',
          error: 'border-red-200 border hover:bg-red-100',
          success: 'border-green-200 border hover:bg-green-100',
          warning: 'border-yellow-200 border hover:bg-yellow-100',
          default: 'border-gray-200 border hover:bg-gray-100',
        };

    if (status === 'running') {
      return baseClasses.running;
    }

    if (status === 'error') {
      return baseClasses.error;
    }

    if (status === 'done') {
      if (!finalState) {
        return baseClasses.default;
      }

      if (finalState === 'passed') {
        return baseClasses.success;
      }

      if (finalState === 'constraints_failed') {
        return baseClasses.warning;
      }

      if (
        ['parse_failed', 'llm_failed', 'assert_failed', 'error'].includes(
          finalState,
        )
      ) {
        return baseClasses.error;
      }
    }

    return baseClasses.default;
  };

  return (
    <div className="flex w-full space-y-4 p-1 pt-3">
      {testHistory.length > 0 ? (
        <div className="flex h-fit w-full flex-col">
          {testHistory.length > 1 && (
            <div className="mb-3 flex items-center justify-between">
              <div className="flex overflow-x-auto">
                <div className="flex gap-1">
                  {testHistory.slice(-7).map((test, index) => (
                    <button
                      key={index}
                      onClick={() => setSelectedHistoryIndex(index)}
                      className={`h-6 min-w-6 rounded px-1.5 text-xs  ${getHistoryButtonColor(
                        test,
                        selectedHistoryIndex === index,
                      )}`}
                    >
                      {testHistory.length - index}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          )}

          {(() => {
            const selectedTest = testHistory[selectedHistoryIndex];
            if (!selectedTest) return null;

            return (
              <>
                <div className="mb-1 text-xs text-muted-foreground/50">
                  {selectedTest.timestamp
                    ? new Date(selectedTest.timestamp).toLocaleString()
                    : 'No timestamp available'}
                </div>
                <TestResult
                  testId={{
                    functionName: selectedTest.functionName,
                    testName: selectedTest.testName,
                  }}
                  historicalResponse={selectedTest.response}
                />
              </>
            );
          })()}
        </div>
      ) : (
        <div className="p-4 text-gray-500">No tests running</div>
      )}
    </div>
  );
};

interface TestStatusProps {
  status: TestState['status'];
  finalState?: DoneTestStatusType;
}

export const TestStatus = ({ status, finalState }: TestStatusProps) => {
  const getStatusColor = (
    status: TestState['status'],
    finalState?: DoneTestStatusType,
  ) => {
    if (status === 'running') return 'text-blue-500';
    if (status === 'done') {
      if (!finalState) return 'text-gray-500';
      return finalState === 'passed'
        ? 'text-green-500'
        : finalState === 'constraints_failed'
          ? 'text-yellow-600'
          : 'text-red-500';
    }
    if (status === 'error') return 'text-red-500';
    return 'text-gray-500';
  };

  const getStatusText = () => {
    if (status === 'running') return 'Running';
    if (status === 'done' && finalState) {
      switch (finalState) {
        case 'passed':
          return 'Passed';
        case 'llm_failed':
          return 'LLM Failed';
        case 'parse_failed':
          return 'Parse Failed';
        case 'constraints_failed':
          return 'Check Failed';
        case 'assert_failed':
          return 'Assert Failed';
        case 'error':
          return 'Error';
      }
    }
    return status;
  };

  const getStatusIcon = () => {
    if (status === 'running') return <Loader />;
    if (status === 'done') {
      if (finalState === 'passed') return <CheckCircle className="h-4 w-4" />;
      if (finalState) return <XCircle className="h-4 w-4" />;
    }
    if (status === 'error') return <AlertCircle className="h-4 w-4" />;
    return null;
  };

  const color = getStatusColor(status, finalState);

  return (
    <div className={`flex items-center gap-1.5 ${color}`}>
      {getStatusIcon()}
      <span className="text-xs md:whitespace-nowrap">{getStatusText()}</span>
    </div>
  );
};

const TestResult = ({
  testId,
  historicalResponse,
}: {
  testId: TestId;
  historicalResponse?: TestState;
}) => {
  const response = useAtomValue(testCaseResponseAtom(testId));
  const displayResponse = historicalResponse || response;

  if (!displayResponse) {
    return null;
  }

  return (
    <div className="flex flex-col gap-2 rounded-lg border p-3">
      <div className="flex items-center justify-between">
        <FunctionTestName
          functionName={testId.functionName}
          testName={testId.testName}
        />
        <TestStatus
          status={displayResponse.status}
          finalState={(displayResponse as any)?.response_status}
        />
      </div>

      {displayResponse.status === 'running' && displayResponse.response && (
        <RenderRunningResponse response={displayResponse.response} />
      )}

      {displayResponse.status === 'done' && (
        <>
          {displayResponse.status === 'done' && displayResponse.response && (
            <div className="flex flex-wrap gap-2 pr-2">
              <TooltipProvider>
                {displayResponse.response.llm_response() && (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Badge
                        variant="outline"
                        className="flex items-center space-x-1"
                      >
                        <Brain className="h-3 w-3" />
                        <span>
                          {displayResponse.response.llm_response()?.model}
                        </span>
                      </Badge>
                    </TooltipTrigger>
                    <TooltipContent>
                      <p>Model</p>
                    </TooltipContent>
                  </Tooltip>
                )}
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Badge
                      variant="outline"
                      className="flex items-center space-x-1 sm:hidden lg:flex"
                    >
                      <Clock className="h-3 w-3" />
                      <span>
                        {(
                          Number(
                            displayResponse.response.llm_response()
                              ?.latency_ms ?? 0,
                          ) / 1000
                        ).toFixed(2)}
                        s
                      </span>
                    </Badge>
                  </TooltipTrigger>
                  <TooltipContent>
                    <p>Latency</p>
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
            </div>
          )}
          <RenderDoneResponse
            response={displayResponse.response}
            status={displayResponse.response_status}
          />
        </>
      )}

      {displayResponse.status === 'error' && (
        <div className="mt-2 text-xs text-red-500">
          Error: {displayResponse.message}
        </div>
      )}
    </div>
  );
};

const RenderParsedResponse: React.FC<{
  parsedResponse: string | undefined;
}> = ({ parsedResponse }) => {
  return (
    <div className="flex flex-col gap-4">
      <span className="text-xs text-muted-foreground">Parsed Response</span>

      {parsedResponse && (
        <RenderText
          text={JSON.stringify(JSON.parse(parsedResponse), null, 2)}
        />
      )}
    </div>
  );
};

const RenderLLMResponse: React.FC<{ response: WasmLLMResponse }> = ({
  response,
}) => {
  const [isExpanded, setIsExpanded] = useState(false);

  const toggleExpand = () => setIsExpanded(!isExpanded);

  return (
    <div className="space-y-3 text-xs">
      <div className="flex items-center space-x-2">
        <span className="text-muted-foreground">LLM Response</span>
      </div>
      <RenderText text={response.content} />
    </div>
  );
};

const RenderRunningResponse: React.FC<{ response: WasmFunctionResponse }> = ({
  response,
}) => {
  const llmFailure = response.llm_failure();
  const llmResponse = response.llm_response();
  const parsedResponse = response.parsed_response();

  if (llmFailure) {
    return <RenderLLMFailure response={llmFailure} />;
  }

  return (
    <div className="flex flex-row gap-4">
      {llmResponse && <RenderLLMResponse response={llmResponse} />}
      <RenderParsedResponse parsedResponse={parsedResponse} />
    </div>
  );
};

const RenderDoneResponse: React.FC<{
  response: WasmTestResponse;
  status: DoneTestStatusType;
}> = ({ response, status }) => {
  const llmFailure = response.llm_failure();
  const failureMessage = response.failure_message();
  const parsedResponse = response.parsed_response();
  const llmResponse = response.llm_response();

  if (status === 'llm_failed') {
    return llmFailure && <RenderLLMFailure response={llmFailure} />;
  }

  if (status === 'parse_failed') {
    return (
      <div className="grid grid-cols-2 gap-4">
        {llmResponse && <RenderLLMResponse response={llmResponse} />}
        <div className="flex flex-col gap-4">
          <span className="text-xs text-muted-foreground">Failure Message</span>
          <pre className="mt-2 whitespace-pre-wrap text-xs">
            {failureMessage}
          </pre>
        </div>
      </div>
    );
  }

  if (parsedResponse) {
    return (
      <div>
        {failureMessage && (
          <div className="-mt-3 flex h-fit w-full flex-col items-end justify-end py-3">
            <div className=" text-xs text-red-500">{failureMessage}</div>
          </div>
        )}
        <div className="grid grid-cols-2 gap-4">
          {llmResponse && <RenderLLMResponse response={llmResponse} />}
          <div className="flex flex-col gap-4">
            <span className="text-xs text-muted-foreground">
              Parsed Response{' '}
              {parsedResponse.check_count > 0 &&
                `(${parsedResponse.check_count} checks)`}
            </span>
            <pre className="mt-2 whitespace-pre-wrap text-xs">
              {JSON.stringify(JSON.parse(parsedResponse.value), null, 2)}
            </pre>
            {parsedResponse.explanation && (
              <div className="flex flex-col gap-2 pt-4">
                Parsing Debugger
                <div className="text-xs text-muted-foreground/80">
                  {parsedResponse.explanation}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    );
  }

  return null;
};

// const RenderLLMResponse: React.FC<{ response: WasmLLMFailure }> = ({ response }) => {
//   return (
//     <div className="text-xs space-y-0.5">
//       <div className="text-muted-foreground">Client: <span className="text-foreground">{response.client_name()}</span></div>
//       <div className="text-muted-foreground">Model: <span className="text-foreground">{response.model || 'N/A'}</span></div>
//       <div className="text-muted-foreground">Latency: <span className="text-foreground">{response.latency_ms.toString()}ms</span></div>
//       <div className="text-muted-foreground">Error Code: <span className="text-foreground">{response.code}</span></div>
//       <div className="text-muted-foreground">Message: <span className="text-foreground">{response.message}</span></div>
//     </div>
//   );
// }

function RenderLLMFailure({ response }: { response: WasmLLMFailure }) {
  const [isExpanded, setIsExpanded] = useState(false);

  const toggleExpand = () => setIsExpanded(!isExpanded);

  return (
    <div className="space-y-3 text-xs">
      <div className="flex items-center space-x-2 text-destructive">
        <AlertCircle className="h-4 w-4" />
        <span className="font-semibold">{response.code}</span>
      </div>
      <div>
        <Button
          variant="ghost"
          size="sm"
          onClick={toggleExpand}
          className="h-auto p-0 font-normal"
        >
          {isExpanded ? (
            <>
              <ChevronUp className="mr-1 h-4 w-4" />
              Hide full message
            </>
          ) : (
            <>
              <ChevronDown className="mr-1 h-4 w-4" />
              Show full message
            </>
          )}
        </Button>

        {isExpanded && (
          <div className="mt-2 whitespace-pre-wrap rounded-md bg-muted p-3 font-mono text-xs">
            {response.message}
          </div>
        )}
      </div>
      <div className="flex flex-wrap gap-2">
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <Badge variant="outline" className="flex items-center space-x-1">
                <Brain className="h-3 w-3" />
                <span>{response.client_name()}</span>
              </Badge>
            </TooltipTrigger>
            <TooltipContent>
              <p>Client Name</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>

        {response.model && (
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <Badge
                  variant="outline"
                  className="flex items-center space-x-1"
                >
                  <Cpu className="h-3 w-3" />
                  <span>{response.model}</span>
                </Badge>
              </TooltipTrigger>
              <TooltipContent>
                <p>Model</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        )}

        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <Badge variant="outline" className="flex items-center space-x-1">
                <Clock className="h-3 w-3" />
                <span>{response.latency_ms.toString()}ms</span>
              </Badge>
            </TooltipTrigger>
            <TooltipContent>
              <p>Latency</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      </div>
    </div>
  );
}
export default TestPanel;
