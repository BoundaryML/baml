import { Badge } from '@baml/ui/badge';
import { cn } from '@baml/ui/lib/utils';
import { useAtom } from 'jotai';
import { Brain, Clock } from 'lucide-react';
import type * as React from 'react';
import type { TestHistoryRun } from '../atoms';
import { MarkdownRenderer } from './MarkdownRenderer';
import { ParsedResponseRenderer } from './ParsedResponseRender';
import { tabularViewConfigAtom } from './atoms';

interface SimpleCardViewProps {
  currentRun?: TestHistoryRun;
}

export const SimpleCardView: React.FC<SimpleCardViewProps> = ({
  currentRun,
}) => {
  const [config] = useAtom(tabularViewConfigAtom);
  return (
    <div className="space-y-4">
      {currentRun?.tests.map((test, index) => (
        <div
          key={index}
          className={cn(
            'rounded-lg border p-4 transition-colors',
            'hover:bg-muted/70 dark:bg-muted/50',
          )}
        >
          <div className="mb-2 flex items-center justify-between">
            <div className="font-mono text-sm">
              {test.functionName}/{test.testName}
            </div>
            <div className="flex space-x-2">
              {test.response.status === 'done' && test.response.response && (
                <>
                  <Badge
                    variant="outline"
                    className="flex items-center space-x-1"
                  >
                    <Brain className="h-3 w-3" />
                    <span>{test.response.response.llm_response()?.model}</span>
                  </Badge>
                  <Badge
                    variant="outline"
                    className="flex items-center space-x-1"
                  >
                    <Clock className="h-3 w-3" />
                    <span>{(test.response.latency_ms / 1000).toFixed(2)}s</span>
                  </Badge>
                </>
              )}
            </div>
          </div>
          {test.response.status === 'done' &&
            test.response.response?.parsed_response() &&
            (config.responseViewType === 'pretty' ? (
              <MarkdownRenderer
                source={JSON.stringify(
                  JSON.parse(
                    test.response.response.parsed_response()?.value ?? '{}',
                  ),
                  null,
                  2,
                )}
              />
            ) : (
              <ParsedResponseRenderer response={test.response.response} />
            ))}
        </div>
      ))}
    </div>
  );
};
