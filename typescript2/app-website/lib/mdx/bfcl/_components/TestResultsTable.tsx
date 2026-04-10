'use client';

// pages/index.tsx
import Ansi from 'ansi-to-react';
import clsx from 'clsx';
import {
  BookA,
  CheckIcon,
  CircleAlertIcon,
  ShareIcon,
  SquareFunction,
  Text,
  XIcon,
} from 'lucide-react';
// import BamlLogo from "./baml-lamb-white.png";
import Image from 'next/image';
import { useEffect, useMemo, useState } from 'react';
import { RWebShare } from 'react-web-share';
import JsonView from 'react18-json-view';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import type { BCFLFunction, Question, TestResult } from '../_loader/types';
import { calculateContent, calculateCost } from '../_loader/utils';
// Import JSONL data using ES modules
// @ts-expect-error - JSONL imports
import mergedData from '../data/merged.jsonl';
// @ts-expect-error - JSONL imports
import multipleFunction from '../data/questions/gorilla_openfunctions_v1_test_multiple_function.jsonl';
// @ts-expect-error - JSONL imports
import parallelFunction from '../data/questions/gorilla_openfunctions_v1_test_parallel_function.jsonl';
// @ts-expect-error - JSONL imports
import parallelMultipleFunction from '../data/questions/gorilla_openfunctions_v1_test_parallel_multiple_function.jsonl';
// @ts-expect-error - JSONL imports
import relevance from '../data/questions/gorilla_openfunctions_v1_test_relevance.jsonl';
// @ts-expect-error - JSONL imports
import simple from '../data/questions/gorilla_openfunctions_v1_test_simple.jsonl';
import 'react18-json-view/src/style.css';
import { usePathname, useRouter, useSearchParams } from 'next/navigation';

function useURLState<T extends string | number>(
  key: string,
  initialValue: T,
): [T, React.Dispatch<React.SetStateAction<T>>] {
  const [state, setState] = useState<T>(initialValue);
  const [initialized, setInitialized] = useState<boolean>(false);
  const searchParams = useSearchParams();
  const pathname = usePathname();
  const router = useRouter();

  useEffect(() => {
    const value = searchParams?.get(key);
    if (value !== null) {
      if (typeof initialValue === 'number') {
        setState(Number.parseInt(value ?? '0', 10) as T);
      } else {
        setState(value as T);
      }
    }
    setInitialized(true);
  }, [key, initialValue, searchParams]);

  useEffect(() => {
    if (initialized) {
      const params = new URLSearchParams(searchParams ?? '');
      params.set(key, state.toString());

      // Update the URL without using a router
      const url = `${pathname}?${params.toString()}`;
      console.log('Setting URL to', url);

      if (url) {
        window.history.replaceState(
          null,
          '',
          `${pathname}?${params.toString()}`,
        );
        // router.push(url).catch((e) => {
        //   console.error(e);
        // });
      }
    }
  }, [key, state, initialized, pathname, searchParams, router]);

  return [state, setState];
}

type CompareMode = 'cost' | 'accuracy' | 'latency' | 'tps';

const QualifierIcon: React.FC<{ qualifier: string }> = ({ qualifier }) => {
  if (qualifier === 'bfcl') {
    return <Text className="h-4 w-4" />;
  }
  if (qualifier === 'FC') {
    return <SquareFunction className="h-4 w-4" />;
  }
  if (qualifier.toLocaleLowerCase() === 'baml') {
    return (
      <div>
        <div className="h-full rounded-md border border-[#A763FF]">
          <Image
            alt="baml"
            className="mb-0 mt-0"
            // fill
            height={9}
            src={'/bamllogopurple.svg'}
            width={18}
          />
        </div>
      </div>
    );
  }
  if (qualifier === 'FC-strict') {
    return <SquareFunction className="h-4 w-4 text-orange-500" />;
  }

  return <span>{qualifier}</span>;
};

const QualifierText = ({ qualifier }: { qualifier: string }) => {
  return (
    {
      baml: 'BAML',
      bfcl: 'Berkley Function Calling Prompting Technique',
      FC: 'Function Calling',
      'FC-strict': 'Function Calling (Strict)',
    }[qualifier] ?? qualifier
  );
};

const QualifierHeader: React.FC<{ qualifier: string }> = ({ qualifier }) => {
  return (
    <div className="flex items-center space-x-2">
      <QualifierIcon qualifier={qualifier} />
      <span className="font-bold">{QualifierText({ qualifier })}</span>
    </div>
  );
};

const StatBadge: React.FC<{
  stats: ReturnType<typeof calculateContent>;
  compareMode: CompareMode;
}> = ({ stats, compareMode }) => {
  if (compareMode === 'cost') {
    return (
      <div className="flex flex-wrap space-x-2">
        {Object.entries(stats.data).map(([qualifier, metrics]) => (
          <Badge
            className={clsx(
              'flex gap-2',
              metrics.cost === stats.best.cost ? 'bg-green-300' : '',
            )}
            key={qualifier}
            variant="outline"
          >
            <QualifierIcon qualifier={qualifier} /> ${metrics.cost.toFixed(4)}
          </Badge>
        ))}
      </div>
    );
  }
  if (compareMode === 'latency') {
    return (
      <div className="flex flex-wrap space-x-2">
        {Object.entries(stats.data).map(([qualifier, metrics]) => (
          <Badge
            className={clsx(
              'flex gap-2',
              metrics.latency.avg === stats.best.latency.avg
                ? 'bg-green-300'
                : '',
            )}
            key={qualifier}
            variant="outline"
          >
            <QualifierIcon qualifier={qualifier} />{' '}
            {metrics.latency.avg.toFixed(2)}s
          </Badge>
        ))}
      </div>
    );
  }
  if (compareMode === 'tps') {
    return (
      <div className="flex flex-wrap gap-2">
        {Object.entries(stats.data).map(([qualifier, metrics]) => (
          <Badge
            className={clsx(
              'flex gap-2',
              metrics.tps.avg === stats.best.tps.avg ? 'bg-green-300' : '',
            )}
            key={qualifier}
            variant="outline"
          >
            <QualifierIcon qualifier={qualifier} /> {metrics.tps.avg.toFixed(2)}
          </Badge>
        ))}
      </div>
    );
  }
  return (
    <div className="flex flex-wrap gap-2">
      {Object.entries(stats.data).map(([qualifier, metrics]) => (
        <Badge
          className={clsx(
            'flex h-fit gap-2',
            metrics.accuracy === stats.best.accuracy ? 'bg-green-300' : '',
          )}
          key={qualifier}
          variant="outline"
        >
          <QualifierIcon qualifier={qualifier} /> {metrics.accuracy.toFixed(2)}%
        </Badge>
      ))}
    </div>
  );
};

const ModelSelector: React.FC<{
  results: TestResult[];
  models: string[];
  selectedModel: string;
  setSelectedModel: (model: string) => void;
  compareMode: CompareMode;
}> = ({ results, models, selectedModel, setSelectedModel, compareMode }) => {
  const renderModel = (model: string) => {
    const accuracySummary = calculateContent(results, model, null);
    const count =
      results.filter((r) => r.model === model).length /
      Object.keys(accuracySummary.data).length;
    return (
      <div className="mb-2 flex items-center space-x-2" key={model}>
        <RadioGroupItem id={`model-${model}`} value={model} />
        <Label htmlFor={`model-${model}`}>
          {model} <span className="text-sm text-gray-500">(n={count})</span>
          <StatBadge compareMode={compareMode} stats={accuracySummary} />
        </Label>
      </div>
    );
  };
  return (
    <div>
      <div className="mb-2 text-lg font-semibold">Select Model</div>
      <RadioGroup onValueChange={setSelectedModel} value={selectedModel}>
        {Array.from(new Set(results.map((r) => r.model))).map((model) =>
          renderModel(model),
        )}
      </RadioGroup>

      <div className="mt-4 rounded-lg border border-gray-300 bg-white p-4 shadow-sm">
        <h3 className="text-md mb-2 font-semibold">Legend</h3>
        <ul className="list-inside list-disc space-y-2">
          <li className="flex items-center gap-2">
            <QualifierIcon qualifier="bfcl" />
            <span>Berkley Function Calling Prompting Technique</span>
          </li>
          <li className="flex items-center gap-2">
            <QualifierIcon qualifier="baml" />
            <span>BAML </span>
          </li>
          <li className="flex items-center gap-2">
            <QualifierIcon qualifier="FC" />
            <span>Function Calling</span>
          </li>
          <li className="flex items-center gap-2">
            <QualifierIcon qualifier="FC-strict" />
            <span>Function Calling (strict)</span>
          </li>
        </ul>
      </div>
    </div>
  );
};

const TestTypeSelector: React.FC<{
  results: TestResult[];
  selectedModel: string;
  testTypes: string[];
  selectedTestType: string;
  setSelectedTestType: (testType: string) => void;
  compareMode: CompareMode;
}> = ({
  results,
  selectedModel,
  testTypes,
  selectedTestType,
  setSelectedTestType,
  compareMode,
}) => {
  const renderTestType = (testType: string, label: string) => {
    const accuracySummary = calculateContent(results, selectedModel, testType);
    const count =
      results.filter(
        (r) => r.id.startsWith(testType) && r.model === selectedModel,
      ).length / Object.keys(accuracySummary.data).length;
    return (
      <div className="mb-2 flex items-start space-x-2" key={testType}>
        <div className="mt-1">
          <RadioGroupItem id={`testType-${testType}`} value={testType} />
        </div>
        <Label className="flex flex-col gap-1" htmlFor={`testType-${testType}`}>
          <span>
            {label} <span className="text-sm text-gray-500">(n={count})</span>
          </span>
          <div className="flex space-x-2 text-muted-foreground">
            {
              {
                multiple_function: 'Choose 1 from many',
                parallel_function: 'Choose many from 1',
                parallel_multiple_function: 'Choose many from many',
                relevance: 'Choose (1 or null) from 1',
                simple: 'Choose 1 from 1',
              }[label]
            }
          </div>
          <StatBadge compareMode={compareMode} stats={accuracySummary} />
        </Label>
      </div>
    );
  };

  const astTests = testTypes.filter((t) => !t.includes('executable'));
  const executableTests = testTypes.filter((t) => t.includes('executable'));

  return (
    <div>
      <h2 className="mb-2 text-lg font-semibold">Select Test Type</h2>
      <RadioGroup onValueChange={setSelectedTestType} value={selectedTestType}>
        <div className="flex flex-col space-y-2">
          <div>
            <h3 className="text-md font-semibold">AST Tests</h3>
            <p className="mb-2 text-sm text-gray-500">
              Tests that check for correctness of the function parameters
            </p>
            {astTests.map((testType) => renderTestType(testType, testType))}
          </div>
          {/* <div>
            <h3 className="text-md font-semibold">Executable Tests</h3>
            <p className="text-sm text-gray-500 mb-2">
              Tests call some actual python code and check for correctness of
              the response
              <span className="flex flex-row gap-2 text-yellow-500">
                <TriangleAlertIcon className="w-4 h-4" /> We&apos;ve found many
                evaluations that are out of date.
              </span>
            </p>
            {executableTests.map((testType) =>
              renderTestType(testType, testType.substring("executable_".length))
            )}
          </div> */}
        </div>
      </RadioGroup>
    </div>
  );
};

const TestResultIcon: React.FC<{ result?: TestResult }> = ({ result }) => {
  if (!result) {
    return null;
  }
  return (
    <>
      {result.valid ? (
        <CheckIcon className="h-6 w-6 text-green-500" />
      ) : result.baml_error ? (
        <CircleAlertIcon className="h-6 w-6 text-red-800" />
      ) : (
        <XIcon className="h-6 w-6 text-red-500" />
      )}
    </>
  );
};

const ShortFn = (fn: BCFLFunction) => {
  let str = `\n${fn.name}`;
  if (fn.parameters?.properties) {
    str += `(${Object.keys(fn.parameters.properties)
      .map((p) => `${p}: ${fn.parameters?.properties[p]?.type ?? 'any'}`)
      .join(', ')})`;
  }
  return (
    <>
      <span className="text-green-600">
        {'// '}
        {fn.description}
      </span>
      {str}
    </>
  );
};

const LongFn = (fn: BCFLFunction) => {
  return (
    <JsonView
      className="bg-muted p-2"
      collapsed={({ depth }) => depth === 2}
      src={fn}
    />
  );
};

const results = mergedData as TestResult[];

const questionFiles = {
  multiple_function: multipleFunction,
  parallel_function: parallelFunction,
  parallel_multiple_function: parallelMultipleFunction,
  relevance: relevance,
  simple: simple,
} as const;

const questions = Object.entries(questionFiles).flatMap(([testType, data]) => {
  return (data as Omit<Question, 'idx' | 'test_type'>[]).map(
    (question, idx) => ({
      idx,
      test_type: testType,
      ...question,
    }),
  );
});

const TestResultsTable: React.FC<{
  // results: TestResult[];
  // questions: Question[];
}> = () => {
  const testTypes = useMemo(
    () => Array.from(new Set(results.map((r) => r.test_type))).sort(),
    [results],
  );
  const modelTypes = useMemo(
    () => Array.from(new Set(results.map((r) => r.model))).sort(),
    [results],
  );

  const [selectedModel, setSelectedModel] = useURLState(
    'model',
    modelTypes.at(-1)!,
  );
  const [selectedTestType, setSelectedTestType] = useURLState(
    'test',
    testTypes[0]!,
  );
  const accuracyMetrics = useMemo(
    () => calculateContent(results, selectedModel, selectedTestType),
    [selectedTestType, selectedModel, results],
  );

  const filteredResults = useMemo(
    () =>
      results.filter(
        (result) =>
          result.model === selectedModel &&
          result.test_type === selectedTestType,
      ),
    [results, selectedModel, selectedTestType],
  );
  const [questionFilter, setQuestionFilter] = useState({
    baml: 'all',
    bfcl: 'all',
    FC: 'all',
    'FC-strict': 'all',
  });
  const [compareMode, setCompareMode] = useURLState<CompareMode>(
    'cmp',
    'accuracy',
  );

  const filteredQuestions = useMemo(() => {
    const qs = questions
      .filter((q) => q.test_type === selectedTestType)
      .map((q) => {
        const result = filteredResults.filter(
          (r) => r.id === `${selectedTestType}_${q.idx}`,
        );
        return {
          ...q,
          status: result.map((r): [keyof typeof questionFilter, TestResult] => [
            r.qualifier,
            r,
          ]),
        };
      })
      .filter((q) => {
        if (Object.values(questionFilter).every((v) => v === 'all')) {
          return true;
        }
        return q.status.every(([qualifier, r]) => {
          if (questionFilter[qualifier] === 'all') {
            return true;
          }
          return r.valid === (questionFilter[qualifier] === 'passed');
        });
      });
    return qs;
  }, [questions, selectedTestType, filteredResults, questionFilter]);

  const [selectedQuestionIdx, setSelectedQuestionIdx] = useURLState<number>(
    'q',
    0,
  );

  const selectedQuestion = useMemo(
    () => filteredQuestions.find((q) => q.idx === selectedQuestionIdx),
    [filteredQuestions, selectedQuestionIdx],
  );

  // use arrow keys to navigate between questions
  useEffect(() => {
    const validIds = filteredQuestions.map((q) => q.idx);
    const handleKeyDown = (e: KeyboardEvent) => {
      // use left and right arrow keys to navigate between questions
      if (e.key === 'ArrowLeft') {
        setSelectedQuestionIdx((prev) => {
          // Find the largest index that is less than the current index
          if (prev === 0) {
            return validIds.at(-1) ?? prev;
          }
          const nextIdx = validIds.filter((id) => id < prev).at(-1);
          return nextIdx ?? prev;
        });
      } else if (e.key === 'ArrowRight') {
        setSelectedQuestionIdx((prev) => {
          // Find the smallest index that is greater than the current index
          if (prev === validIds.at(-1)) {
            return validIds.at(0) ?? prev;
          }
          const nextIdx = validIds.filter((id) => id > prev)[0];
          return nextIdx ?? prev;
        });
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [filteredQuestions]);

  // anytime filteredQuestsoins changes, reset the selected question unless it's already selected
  useEffect(() => {
    if (!filteredQuestions.find((q) => q.idx === selectedQuestionIdx)) {
      setSelectedQuestionIdx(filteredQuestions[0]?.idx ?? 0);
    }
  }, [filteredQuestions]);

  return (
    <div className="not-prose table-container flex w-[100vw] flex-col items-center justify-center overflow-x-scroll p-1 lg:px-8">
      <div className="flex w-full flex-col">
        <h1 className="mb-4 text-2xl font-bold">
          BFCL Model/Technique Comparison
        </h1>

        <div className="flex flex-row items-center gap-2">
          <span>Compare by:</span>

          <RadioGroup
            className="flex flex-row items-center gap-2"
            onValueChange={(prev) => setCompareMode(prev as CompareMode)}
            value={compareMode}
          >
            {(['accuracy', 'cost', 'latency', 'tps'] as CompareMode[]).map(
              (mode) => (
                <div className="mb-2 flex items-center space-x-2" key={mode}>
                  <RadioGroupItem id={`stat-type-${mode}`} value={mode} />
                  <Label htmlFor={`stat-type-${mode}`}>
                    {mode.charAt(0).toUpperCase() + mode.slice(1)}
                  </Label>
                </div>
              ),
            )}
          </RadioGroup>
        </div>

        <div className="mb-4 grid grid-cols-1 gap-4 md:grid-cols-3">
          <ModelSelector
            compareMode={compareMode}
            models={modelTypes}
            results={results}
            selectedModel={selectedModel}
            setSelectedModel={setSelectedModel}
          />
          <TestTypeSelector
            compareMode={compareMode}
            results={results}
            selectedModel={selectedModel}
            selectedTestType={selectedTestType}
            setSelectedTestType={setSelectedTestType}
            testTypes={testTypes}
          />

          <div className="mb-4">
            <h2 className="mb-2 text-xl font-semibold">Metrics</h2>
            <div className="grid grid-rows-3 gap-4">
              {Object.entries(accuracyMetrics.data).map(
                ([qualifier, metrics]) => (
                  <Card className={clsx('rounded p-2')} key={qualifier}>
                    <CardHeader className="p-1">
                      <QualifierHeader qualifier={qualifier} />
                    </CardHeader>
                    <CardContent className="text-xs">
                      <RadioGroup
                        className="mb-2 flex flex-row items-center space-x-5"
                        onValueChange={(val) =>
                          setQuestionFilter((prev) => ({
                            ...prev,
                            [qualifier]: val,
                          }))
                        }
                        value={
                          questionFilter[
                            qualifier as keyof typeof questionFilter
                          ]
                        }
                      >
                        {['all', 'passed', 'failed'].map((status) => (
                          <div
                            className="mb-2 flex items-center space-x-2"
                            key={status}
                          >
                            <RadioGroupItem
                              id={`${qualifier}-${status}`}
                              value={status}
                            />
                            <Label htmlFor={`${qualifier}-${status}`}>
                              {status.charAt(0).toUpperCase() + status.slice(1)}
                            </Label>
                          </div>
                        ))}
                      </RadioGroup>
                      <p>
                        Accuracy:{' '}
                        <span
                          className={clsx(
                            accuracyMetrics.best.accuracy === metrics.accuracy
                              ? 'font-bold text-green-600'
                              : '',
                          )}
                        >
                          {metrics.accuracy.toFixed(2)}%{' '}
                        </span>
                        {accuracyMetrics.best.accuracy > metrics.accuracy && (
                          <span className="text-sm font-bold text-red-500">
                            (
                            {(
                              metrics.accuracy - accuracyMetrics.best.accuracy
                            ).toFixed(2)}
                            %)
                          </span>
                        )}
                      </p>
                      <p>
                        Mean Latency:{' '}
                        <span className={clsx('font-bold')}>
                          {metrics.latency.avg.toFixed(2)} ±{' '}
                          {metrics.latency.std.toFixed(2)}s
                        </span>
                      </p>
                      <p>
                        Mean TPS:{' '}
                        <span className={clsx('font-bold')}>
                          {metrics.tps.avg.toFixed(2)} ±{' '}
                          {metrics.tps.std.toFixed(2)}
                        </span>
                      </p>
                      <p>
                        Total Cost:{' '}
                        <span className={clsx('font-bold')}>
                          ${metrics.cost.toFixed(4)}
                        </span>
                      </p>
                      <p>
                        Total Input Tokens:{' '}
                        <span className={clsx('font-bold')}>
                          {metrics.input_tokens}
                        </span>
                      </p>
                      <p>
                        Total Output Tokens:{' '}
                        <span className={clsx('font-bold')}>
                          {metrics.output_tokens}
                        </span>
                      </p>
                    </CardContent>
                  </Card>
                ),
              )}
            </div>
          </div>
        </div>

        <div className="flex flex-col md:flex-row">
          <div className="w-full border-r border-gray-200 md:w-1/4">
            <h2 className="mb-2 text-xl font-semibold">Questions</h2>
            <div className="flex flex-col gap-2 text-xs">
              <div>
                Showing {filteredQuestions.length} out of{' '}
                {
                  questions.filter((q) => q.test_type === selectedTestType)
                    .length
                }{' '}
                questions
              </div>
              <div className="flex flex-col gap-2 text-xs">
                Use Left and Right arrow keys to navigate between questions
              </div>
            </div>

            <div className="block md:hidden">
              <Select
                onValueChange={(value) => setSelectedQuestionIdx(Number(value))}
              >
                <SelectTrigger className="w-full">
                  <SelectValue placeholder="Select a question" />
                </SelectTrigger>
                <SelectContent>
                  {filteredQuestions.map((result) => (
                    <SelectItem key={result.idx} value={result.idx.toString()}>
                      <div className="flex flex-col items-start gap-0.5">
                        <div className="flex items-start justify-between gap-2">
                          <div>Q:{result.idx}</div>
                          <div className="flex flex-row items-end gap-1">
                            {result.status.map(([qualifier, status]) => (
                              <div
                                className="flex flex-row items-center gap-1"
                                key={qualifier}
                              >
                                <TestResultIcon result={status} />
                                <QualifierIcon qualifier={qualifier} />
                              </div>
                            ))}
                          </div>
                        </div>
                        <div className="w-96 truncate text-muted-foreground">
                          {result.question.length > 100
                            ? `${result.question.slice(0, 100)}...`
                            : result.question}
                        </div>
                      </div>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="hidden md:block">
              <ScrollArea className="h-[75vh]">
                {filteredQuestions.map((result) => (
                  <div
                    className={clsx(
                      'flex cursor-pointer flex-col items-start gap-2 border-b border-gray-200 p-4 hover:bg-gray-100',
                      selectedQuestion?.idx === result.idx ? 'bg-gray-100' : '',
                    )}
                    key={result.idx}
                    onClick={() => setSelectedQuestionIdx(result.idx)}
                  >
                    <div className="flex flex-row items-center gap-2">
                      <div>Q:{result.idx}</div>
                      <div className="flex flex-row gap-2">
                        {result.status.map(([qualifier, status]) => (
                          <div
                            className="flex flex-row items-center gap-0.5"
                            key={qualifier}
                          >
                            <TestResultIcon result={status} />
                            <QualifierIcon qualifier={qualifier} />
                          </div>
                        ))}
                      </div>
                    </div>
                    {/* <div>
                    <span className="text-muted-foreground text-sm">
                      {result.question.length > 100
                        ? `${result.question.slice(0, 100)}...`
                        : result.question}
                    </span>
                  </div> */}
                  </div>
                ))}
              </ScrollArea>
            </div>
          </div>
          <div className="w-full p-4 md:w-3/4">
            {selectedQuestion ? (
              <InspectQuestionDialog question={selectedQuestion} />
            ) : (
              <div className="text-center text-gray-500">
                Select a question to inspect
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

const ShareButton: React.FC<{ idx: number }> = ({ idx }) => {
  // Get the base URL
  const baseUrl =
    typeof window !== 'undefined'
      ? `${window.location.href}#question-preview`
      : '';

  return (
    <RWebShare
      data={{
        text: 'Share this question',
        title: `Question ${idx}`,
        url: baseUrl,
      }}
    >
      <ShareIcon className="h-6 w-6 cursor-pointer" />
    </RWebShare>
  );
};

const InspectQuestionDialog: React.FC<{
  question: Question & {
    status: ['baml' | 'FC' | 'bfcl' | 'FC-strict', TestResult][];
  };
}> = ({ question }) => {
  const [selectedTab, setSelectedTab] = useURLState<number>('r', 0);

  return (
    <div className="flex max-h-[75%] flex-col gap-4 p-4" id="question-preview">
      <div className="flex flex-row items-center gap-2">
        <h2 className="text-xl font-semibold">Question {question.idx}</h2>
        <ShareButton idx={question.idx} />
      </div>
      <div className="flex flex-col gap-2">
        <pre className="whitespace-pre-wrap rounded-md bg-muted p-2">
          {question.question}
        </pre>
        <div className="flex flex-col gap-2">
          Expected
          <pre className="flex flex-col whitespace-pre-wrap rounded-md bg-muted p-2">
            {question.status[0]?.[1].ground_truth ? (
              Object.entries(question.status[0]?.[1].ground_truth)
                .map(([k, v]) => {
                  const params = Object.entries(v)
                    .map(([name, values]) => {
                      const options = values
                        .map((v) => {
                          if (v === '') {
                            return 'null';
                          }
                          if (typeof v === 'number') {
                            return `${v}`;
                          }
                          return JSON.stringify(v);
                        })
                        .join(' | ');
                      return `${name} = ${options}`;
                    })
                    .join(',\n  ');
                  return (
                    <>
                      <span className="text-blue-600">{k}</span>({'\n  '}
                      {params}
                      {'\n'})
                    </>
                  );
                })
                .map((s, idx) => <span key={`${idx}`}>{s}</span>)
            ) : (
              <span className="text-red-600">nothing</span>
            )}
          </pre>
        </div>
        <div className="flex flex-row items-center space-x-2 text-sm">
          {['Function Details', 'baml', 'FC', 'FC-strict', 'bfcl'].map(
            (label, index) => (
              <div className="flex items-center" key={index}>
                <div
                  className={`flex cursor-pointer items-center gap-1 text-lg font-semibold ${
                    selectedTab === index
                      ? 'text-blue-500 hover:text-blue-500'
                      : ''
                  } hover:text-gray-500`}
                  onClick={() => setSelectedTab(index)}
                >
                  {label === 'Function Details' ? (
                    <Badge
                      className={clsx(
                        'p-2',
                        selectedTab === index
                          ? 'border-blue-500 bg-blue-100'
                          : '',
                      )}
                      variant={'outline'}
                    >
                      <BookA className="h-4 w-4" /> Spec
                    </Badge>
                  ) : (
                    <Badge
                      className={clsx(
                        selectedTab === index
                          ? 'border-blue-500 bg-blue-100'
                          : '',
                        'flex flex-row gap-0.5 ',
                      )}
                      variant={'outline'}
                    >
                      <TestResultIcon
                        result={
                          question.status.find(([qualifier, _]) => {
                            return qualifier === label;
                          })?.[1]
                        }
                      />
                      <QualifierIcon qualifier={label} />
                    </Badge>
                  )}
                </div>
                {index < 3 && (
                  <Separator
                    className="hidden md:block"
                    orientation="vertical"
                  />
                )}
              </div>
            ),
          )}
        </div>
      </div>
      {selectedTab === 0 && (
        <div className="flex flex-col gap-2">
          <div className="font-semibold">
            Function definitions used for this test:
          </div>

          {Array.isArray(question.function) ? (
            question.function.map((f) => (
              <div className="flex flex-col gap-2" key={f.name}>
                <pre className="whitespace-pre-wrap">
                  <ShortFn {...f} />
                </pre>

                <div className="flex flex-col gap-2 text-xs">
                  <div>Schema:</div>
                  <LongFn {...f} />
                </div>
              </div>
            ))
          ) : (
            <div className="flex flex-col gap-2">
              <pre className="whitespace-pre-wrap">
                <ShortFn {...question.function} />
              </pre>
              <div className="flex flex-col gap-2 text-xs">
                <div>Schema:</div>

                <LongFn {...question.function} />
              </div>
            </div>
          )}
        </div>
      )}
      {selectedTab === 1 && (
        <div className="flex w-full flex-col gap-2 p-1">
          {question.status
            .filter(([qualifier, _]) => qualifier === 'baml')
            .map(([_, result]) => (
              <RenderTestResult key={result.id} result={result} />
            ))}
        </div>
      )}
      {selectedTab === 2 && (
        <div className="flex flex-col gap-2 p-1">
          {question.status
            .filter(([qualifier, _]) => qualifier === 'FC')
            .map(([_, result]) => (
              <RenderTestResult key={result.id} result={result} />
            ))}
        </div>
      )}
      {selectedTab === 3 && (
        <div className="flex flex-col gap-2 p-1">
          {question.status
            .filter(([qualifier, _]) => qualifier === 'FC-strict')
            .map(([_, result]) => (
              <RenderTestResult key={result.id} result={result} />
            ))}
        </div>
      )}
      {selectedTab === 4 && (
        <div className="flex flex-col gap-2 p-1">
          {question.status
            .filter(([qualifier, _]) => qualifier === 'bfcl')
            .map(([_, result]) => (
              <RenderTestResult key={result.id} result={result} />
            ))}
        </div>
      )}
    </div>
  );
};

const RenderTestResult: React.FC<{ result: TestResult }> = ({ result }) => {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-col gap-2 md:flex-row">
        <Badge variant="secondary">
          IO Tokens: {result.input_token_count} -&gt;{' '}
          {result.output_token_count}
        </Badge>
        <Badge variant="secondary">
          $
          {(
            calculateCost(
              result.input_token_count,
              result.output_token_count,
              result.model,
            ) * 1000
          ).toFixed(4)}{' '}
          / 1000
        </Badge>
        <Badge variant="secondary">Latency: {result.latency.toFixed(2)}s</Badge>
        <Badge variant="secondary">
          {result.latency === 0
            ? 0
            : (result.output_token_count / result.latency).toFixed(0)}{' '}
          tokens / sec
        </Badge>
      </div>
      {result.baml_error ? (
        <RenderError error={[result.baml_error]} error_type="crash" />
      ) : result.error ? (
        <RenderError error={result.error} error_type={result.error_type} />
      ) : null}
      <RenderAnswer
        answer={result.model_result_decoded ?? result.result}
        requireTool={result.qualifier.includes('FC')}
      />
      {result.prompt.parsed && <RenderRawLLM response={result.prompt.parsed} />}
      <RenderPrompt prompt={result.prompt} />

      <div className="font-semibold uppercase">Raw Data</div>
      <div className="flex flex-col gap-2 rounded-md border-2 border-muted bg-muted p-2">
        <pre className="whitespace-pre-wrap text-muted-foreground">
          <JsonView collapsed src={result} />
        </pre>
      </div>
    </div>
  );
};

const RenderError = ({
  error,
  error_type,
}: {
  error: NonNullable<TestResult['error']>;
  error_type: TestResult['error_type'];
}) => {
  return (
    <div className="flex flex-col gap-2 rounded-md bg-red-50 p-2 text-xs">
      <span className="text-sm font-semibold uppercase text-red-700">
        {error_type}
      </span>
      <pre className="whitespace-pre-wrap text-muted-foreground">
        {error_type === 'crash' ? (
          <Ansi>{error.join('\n\n')}</Ansi>
        ) : (
          error
            .map((e) => {
              if (typeof e === 'string') {
                return e;
              }
              return Object.values(e)
                .flatMap((v) => v.sub_error)
                .join('\n\n');
            })
            .join('\n\n')
        )}
      </pre>
    </div>
  );
};

const RenderAnswer = ({
  answer,
  requireTool,
}: {
  answer: TestResult['result'];
  requireTool: boolean;
}) => {
  return (
    <div className="flex flex-col gap-2 rounded-md bg-muted p-2">
      <span className="text-sm font-semibold uppercase text-blue-500">
        Answer{' '}
        {requireTool && typeof answer === 'string' && (
          <span className="text-red-400">(No Tools Called)</span>
        )}
      </span>
      <pre className="whitespace-pre-wrap bg-muted text-muted-foreground">
        {answer === null ||
        (typeof answer === 'string' &&
          answer.startsWith('Error: Error code:')) ? (
          <span className="text-red-400">(nothing)</span>
        ) : typeof answer === 'string' ? (
          answer
        ) : (
          answer
            .map((funcs) =>
              Object.entries(funcs).map(
                ([name, fn]) =>
                  `${name}(${Object.entries(
                    // eslint-disable-next-line @typescript-eslint/no-unsafe-argument
                    typeof fn === 'string' ? JSON.parse(fn) : fn,
                  )
                    .filter(([k]) => k !== 'api_name')
                    .map(([k, v]) => `${k}=${JSON.stringify(v)}`)
                    .join(', ')})`,
              ),
            )
            .join('\n')
        )}
      </pre>
    </div>
  );
};

const RenderRawLLM = ({ response }: { response: string }) => {
  return (
    <div className="flex flex-col gap-2 rounded-md bg-muted p-2 text-xs">
      <span className="text-sm font-semibold uppercase text-muted-foreground">
        Raw LLM Response
      </span>
      <pre className="whitespace-pre-wrap">{response}</pre>
    </div>
  );
};

const RenderPrompt: React.FC<{ prompt: TestResult['prompt'] }> = ({
  prompt,
}) => {
  return (
    <div className="flex flex-col gap-2 text-sm">
      <span className="font-semibold">Prompt</span>
      {prompt.messages.map((p, idx) => (
        <div
          className="flex flex-col gap-2 rounded-md border-2 border-muted bg-muted p-2"
          key={idx}
        >
          <span className="font-semibold uppercase text-muted-foreground">
            {p.role.toUpperCase()}
          </span>
          {'content' in p ? (
            <pre className="whitespace-pre-wrap text-xs">{p.content}</pre>
          ) : (
            <pre className="whitespace-pre-wrap text-xs">
              {p.parts.map((part, idx) => part.Text).join('\n')}
            </pre>
          )}
        </div>
      ))}

      {prompt.tools?.map((tool, idx) => {
        return (
          <div
            className="flex flex-col gap-2 rounded-md border-2 border-muted bg-muted p-2"
            key={idx}
          >
            <span className="font-semibold uppercase text-muted-foreground">
              TOOL - {(tool as { function: { name: string } }).function.name}
            </span>
            <pre className="whitespace-pre-wrap text-muted-foreground">
              <JsonView collapsed={({ depth }) => depth === 2} src={tool} />
            </pre>
          </div>
        );
      })}
    </div>
  );
};

export default TestResultsTable;
