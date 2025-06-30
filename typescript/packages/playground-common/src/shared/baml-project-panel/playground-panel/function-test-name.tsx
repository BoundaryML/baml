import { useAtomValue } from 'jotai';
import { ChevronRight, FlaskConical, FunctionSquare } from 'lucide-react';
import { useMemo } from 'react';
import { vscode } from '../vscode';
import { functionObjectAtom, testcaseObjectAtom } from './atoms';
import { cn } from '@baml/ui/lib/utils';

interface FunctionTestNameProps {
  functionName: string;
  testName: string;
  selected?: boolean;
}

interface StringSpan {
  start: number;
  end: number;
  source_file: string;
  value: string;
}

export const FunctionTestName: React.FC<FunctionTestNameProps> = ({
  functionName,
  testName,
  selected,
}) => {
  const functionAtom = useMemo(
    () => functionObjectAtom(functionName),
    [functionName],
  );
  const testcaseAtom = useMemo(
    () => testcaseObjectAtom({ functionName, testcaseName: testName }),
    [functionName, testName],
  );
  const fn = useAtomValue(functionAtom);
  const tc = useAtomValue(testcaseAtom);
  const createSpan = (span: {
    start: number;
    end: number;
    file_path: string;
    start_line: number;
  }) => ({
    start: span.start,
    end: span.end,
    source_file: span.file_path,
    value: `${span.file_path.split('/').pop() ?? '<file>.baml'}:${span.start_line + 1}`,
  });

  return (
    <div
      className={cn(
        'flex w-full text-sm',
        !selected && 'text-foreground/90',
        'flex-col sm:flex-row sm:items-center sm:space-x-2 sm:gap-y-0 gap-y-1 items-start',
      )}
    >
      <div
        className="flex items-center cursor-pointer hover:text-primary gap-1"
        onClick={() => {
          if (fn?.span) {
            vscode.postMessage({
              command: 'jumpToFile',
              span: createSpan(fn.span),
            });
          }
        }}
      >
        <FunctionSquare className="size-4" />
        {functionName}
      </div>
      <ChevronRight className="size-4 sm:block hidden" />
      <div
        className="flex items-center cursor-pointer hover:text-primary gap-1"
        onClick={() => {
          if (tc?.span) {
            vscode.postMessage({
              command: 'jumpToFile',
              span: createSpan(tc.span),
            });
          }
        }}
      >
        <FlaskConical className="size-4" />
        {testName}
      </div>
    </div>
  );
};
