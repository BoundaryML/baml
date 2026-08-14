'use client';

import {
  ArrowUp,
  BarChart3,
  ChevronDown,
  Copy,
  Key,
  Play,
  RefreshCw,
  Settings,
} from 'lucide-react';
import { motion } from 'motion/react';
import { cn } from '@/lib/utils';

// Header Component
interface CodeAnalysisHeaderProps {
  projectName?: string;
  modelName?: string;
  onRunTest?: () => void;
  onCopy?: () => void;
  onShowTokens?: () => void;
  onSettings?: () => void;
  onApiKeys?: () => void;
}

export function CodeAnalysisHeader({
  projectName = 'AnalyzeCodebase',
  modelName = 'AnalyzeCodebase',
  onRunTest,
  onCopy,
  onShowTokens,
  onSettings,
  onApiKeys,
}: CodeAnalysisHeaderProps) {
  return (
    <div className="flex items-center justify-between w-full p-4 bg-background border-b border-border">
      <div className="flex items-center gap-4">
        <div className="flex items-center gap-2">
          <span className="text-sm font-mono">f</span>
          <span className="text-sm font-medium">{projectName}</span>
          <ChevronDown className="w-3 h-3" />
        </div>
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium">{modelName}</span>
          <ChevronDown className="w-3 h-3" />
        </div>
      </div>

      <div className="flex items-center gap-3">
        <button
          className="flex items-center gap-2 px-4 py-2 bg-purple-600 text-white rounded-md hover:bg-purple-700 transition-colors"
          onClick={onRunTest}
          type="button"
        >
          <Play className="w-4 h-4" />
          Run Test
        </button>

        <button
          className="p-2 text-muted-foreground hover:text-foreground transition-colors"
          onClick={onCopy}
          type="button"
        >
          <Copy className="w-4 h-4" />
        </button>

        <div className="flex items-center gap-2">
          <button
            className="flex items-center gap-2 p-2 text-muted-foreground hover:text-foreground transition-colors"
            onClick={onApiKeys}
            type="button"
          >
            <Key className="w-4 h-4" />
            API Keys
            <div className="w-2 h-2 bg-orange-500 rounded-full" />
          </button>
        </div>

        <button
          className="p-2 text-muted-foreground hover:text-foreground transition-colors"
          onClick={onSettings}
          type="button"
        >
          <Settings className="w-4 h-4" />
        </button>
      </div>

      <div className="absolute top-12 right-4">
        <button
          className="flex items-center gap-2 text-xs text-muted-foreground hover:text-foreground transition-colors"
          onClick={onShowTokens}
          type="button"
        >
          <BarChart3 className="w-3 h-3" />
          Show Tokens
        </button>
      </div>
    </div>
  );
}

// System Prompt Component
interface SystemPromptProps {
  systemPrompt?: string;
  codeExample?: string;
  jsonSchema?: string;
  onCopy?: () => void;
  onCollapse?: () => void;
}

export function SystemPrompt({
  systemPrompt = 'Analyze the codebase for:',
  codeExample = `function add(a: int, b: int) -> int {
  return a + b
}`,
  jsonSchema = `{
  summary: string,
  issues: [
    {
      description: string,
      severity: string,
    }
  ],
}`,
  onCopy,
  onCollapse,
}: SystemPromptProps) {
  return (
    <div className="w-full bg-background border border-border rounded-lg">
      <div className="flex items-center justify-between p-4 border-b border-border">
        <div className="flex gap-4">
          <button
            className="px-3 py-1 text-sm font-medium text-foreground border-b-2 border-purple-600"
            type="button"
          >
            Preview
          </button>
          <button
            className="px-3 py-1 text-sm text-muted-foreground hover:text-foreground"
            type="button"
          >
            cURL
          </button>
          <button
            className="px-3 py-1 text-sm text-muted-foreground hover:text-foreground"
            type="button"
          >
            Client Graph
          </button>
        </div>
        <div className="flex items-center gap-2">
          <button
            className="p-1 text-muted-foreground hover:text-foreground transition-colors"
            onClick={onCopy}
            type="button"
          >
            <Copy className="w-4 h-4" />
          </button>
          <button
            className="p-1 text-muted-foreground hover:text-foreground transition-colors"
            onClick={onCollapse}
            type="button"
          >
            <ArrowUp className="w-4 h-4" />
          </button>
        </div>
      </div>

      <div className="p-4 space-y-4">
        <div>
          <h3 className="text-sm font-medium mb-2">System</h3>
          <p className="text-sm text-muted-foreground mb-3">{systemPrompt}</p>

          <div className="bg-muted rounded-md p-3 mb-3">
            <pre className="text-sm font-mono">
              <span className="text-blue-500">
                {codeExample.split('\n')[0]}
              </span>
              {codeExample
                .split('\n')
                .slice(1)
                .map((line, i) => (
                  <span key={i}>{line}</span>
                ))}
            </pre>
          </div>

          <p className="text-sm text-muted-foreground mb-2">
            Answer in JSON using this schema:
          </p>

          <div className="bg-muted rounded-md p-3">
            <pre className="text-sm font-mono text-muted-foreground">
              {jsonSchema}
            </pre>
          </div>
        </div>
      </div>
    </div>
  );
}

// History Component
interface HistoryProps {
  timestamp?: string;
  successCount?: number;
  failureCount?: number;
  onPlay?: () => void;
  onRefresh?: () => void;
  onDetailedView?: () => void;
}

export function History({
  timestamp = '7/23/2025, 9:02:19 PM',
  successCount = 2,
  failureCount = 1,
  onPlay,
  onRefresh,
  onDetailedView,
}: HistoryProps) {
  return (
    <div className="flex items-center justify-between w-full p-3 bg-muted/50 border-b border-border">
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-2">
          <div className="w-4 h-4 border-2 border-muted-foreground rounded-full" />
          <span className="text-xs text-muted-foreground">History</span>
        </div>

        <div className="flex gap-2">
          <div className="w-8 h-6 bg-green-600 rounded text-xs text-white flex items-center justify-center">
            {successCount}
          </div>
          <div className="w-8 h-6 bg-red-600 rounded text-xs text-white flex items-center justify-center">
            {failureCount}
          </div>
        </div>

        <span className="text-xs text-muted-foreground">{timestamp}</span>
      </div>

      <div className="flex items-center gap-2">
        <button
          className="p-1 text-muted-foreground hover:text-foreground transition-colors"
          onClick={onPlay}
        >
          <Play className="w-4 h-4" />
        </button>
        <button
          className="p-1 text-muted-foreground hover:text-foreground transition-colors"
          onClick={onRefresh}
        >
          <RefreshCw className="w-4 h-4" />
        </button>
        <button
          className="flex items-center gap-1 px-2 py-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
          onClick={onDetailedView}
        >
          Detailed View
          <ChevronDown className="w-3 h-3" />
        </button>
      </div>
    </div>
  );
}

// Results Component
interface AnalysisResult {
  summary: string;
  issues: Array<{
    description: string;
    severity: string;
  }>;
}

interface ResultsProps {
  model?: string;
  executionTime?: string;
  inputTokens?: number;
  outputTokens?: number;
  status?: 'passed' | 'failed';
  result?: AnalysisResult;
  onFeedback?: () => void;
}

export function Results({
  model = 'gpt-4o-2024-08-06',
  executionTime = '3.06s',
  inputTokens = 71,
  outputTokens = 96,
  status = 'passed',
  result,
  onFeedback,
}: ResultsProps) {
  const defaultResult: AnalysisResult = {
    issues: [
      {
        description:
          "The function lacks input validation to ensure that the arguments 'a' and 'b'",
        severity: 'low',
      },
    ],
    summary:
      "The function 'add' takes two integer inputs and returns their sum.",
  };

  const analysisResult = result || defaultResult;

  return (
    <div className="w-full bg-background border border-border rounded-lg">
      <div className="flex items-center justify-between p-4 border-b border-border">
        <div className="flex items-center gap-3">
          <Play className="w-4 h-4 text-muted-foreground" />
          <span className="text-sm font-mono">f AnalyzeCodebase</span>
          <ChevronDown className="w-3 h-3" />
          <div className="flex items-center gap-1">
            <div className="w-4 h-4 bg-blue-500 rounded" />
            <span className="text-sm font-mono">AnalyzeCodebase</span>
            <ChevronDown className="w-3 h-3" />
          </div>
        </div>

        <div className="flex items-center gap-2">
          <div
            className={cn(
              'flex items-center gap-1 px-2 py-1 rounded text-xs font-medium',
              status === 'passed'
                ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200'
                : 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200',
            )}
          >
            <div className="w-2 h-2 bg-current rounded-full" />
            {status === 'passed' ? 'Passed' : 'Failed'}
          </div>
        </div>
      </div>

      <div className="p-4">
        <div className="flex items-center gap-3 mb-4">
          <span className="text-xs text-muted-foreground">{model}</span>
          <span className="text-xs text-muted-foreground">{executionTime}</span>
          <span className="text-xs text-muted-foreground">
            {inputTokens} in | {outputTokens} out
          </span>
        </div>

        <div>
          <h3 className="text-sm font-medium mb-3">Parsed Response</h3>
          <div className="bg-muted rounded-md p-3 space-y-2">
            <div className="flex items-center gap-2">
              <div className="w-4 h-4 bg-green-500 rounded-full" />
              <span className="text-sm font-mono">{'{ 2 items'}</span>
            </div>

            <div className="ml-4 space-y-1">
              <div className="text-sm">
                <span className="text-muted-foreground">"summary": </span>
                <span className="text-foreground">
                  "{analysisResult.summary}"
                </span>
              </div>

              <div className="flex items-center gap-2">
                <div className="w-4 h-4 bg-green-500 rounded-full" />
                <span className="text-sm font-mono">"issues": [ 1 item</span>
              </div>

              <div className="ml-4 space-y-1">
                <div className="flex items-center gap-2">
                  <div className="w-4 h-4 bg-green-500 rounded-full" />
                  <span className="text-sm font-mono">{'{ 2 items'}</span>
                </div>

                <div className="ml-4 space-y-1">
                  <div className="text-sm">
                    <span className="text-muted-foreground">
                      "description":{' '}
                    </span>
                    <span className="text-foreground">
                      "{analysisResult.issues[0].description}"
                    </span>
                  </div>
                  <div className="text-sm">
                    <span className="text-muted-foreground">"severity": </span>
                    <span className="text-foreground">
                      "{analysisResult.issues[0].severity}"
                    </span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <button
        className="absolute bottom-4 left-4 w-8 h-8 bg-muted rounded-full flex items-center justify-center hover:bg-muted/80 transition-colors"
        onClick={onFeedback}
      >
        <div className="w-3 h-3 border-2 border-muted-foreground rounded-full" />
      </button>
    </div>
  );
}

// Main Code Analysis Interface Component
interface CodeAnalysisInterfaceProps {
  className?: string;
  headerProps?: Partial<CodeAnalysisHeaderProps>;
  systemPromptProps?: Partial<SystemPromptProps>;
  historyProps?: Partial<HistoryProps>;
  resultsProps?: Partial<ResultsProps>;
}

export function CodeAnalysisInterface({
  className,
  headerProps,
  systemPromptProps,
  historyProps,
  resultsProps,
}: CodeAnalysisInterfaceProps) {
  return (
    <motion.div
      animate={{ opacity: 1, y: 0 }}
      className={cn(
        'w-full max-w-4xl mx-auto bg-background border border-border rounded-lg overflow-hidden shadow-lg',
        className,
      )}
      initial={{ opacity: 0, y: 20 }}
      transition={{ duration: 0.5 }}
    >
      <CodeAnalysisHeader {...headerProps} />
      <div className="p-6 space-y-4">
        <SystemPrompt {...systemPromptProps} />
        <History {...historyProps} />
        <Results {...resultsProps} />
      </div>
    </motion.div>
  );
}
