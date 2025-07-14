import { useAtomValue, useSetAtom } from 'jotai';
import { atom } from 'jotai';
import { ChevronRight, FlaskConical, FunctionSquare, ChevronDown, Check } from 'lucide-react';
import { useMemo, useState } from 'react';
import { vscode } from '../vscode';
import { functionObjectAtom, testcaseObjectAtom, runtimeStateAtom, selectedItemAtom } from './atoms';
import { cn } from '@baml/ui/lib/utils';
import { Breadcrumb, BreadcrumbList, BreadcrumbItem, BreadcrumbSeparator } from '@baml/ui/breadcrumb';
import { Button } from '@baml/ui/button';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@baml/ui/command';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@baml/ui/popover';
import { Tooltip, TooltipTrigger, TooltipContent } from '@baml/ui/tooltip';

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

const functionsAtom = atom((get) => {
  const runtimeState = get(runtimeStateAtom);
  if (!runtimeState) {
    return [];
  }
  return runtimeState.functions.map((f) => ({
    name: f.name,
    tests: f.test_cases.map((t) => t.name),
  }));
});

export const FunctionTestName: React.FC<FunctionTestNameProps> = ({
  functionName,
  testName,
}) => {
  const [functionOpen, setFunctionOpen] = useState(false);
  const [testOpen, setTestOpen] = useState(false);

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
  const functions = useAtomValue(functionsAtom);
  const setSelectedItem = useSetAtom(selectedItemAtom);

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

  const currentFunction = functions.find(f => f.name === functionName);
  const availableTests = currentFunction?.tests || [];

  return (
    <Breadcrumb>
      <BreadcrumbList className="flex flex-nowrap overflow-x-auto">
        <BreadcrumbItem className="flex items-center gap-1">
          <div className="flex items-center gap-1 min-w-0 max-w-[120px] sm:max-w-[240px] md:max-w-[300px] shrink">
            <FunctionSquare className="size-4 mr-2 shrink-0" />
            <Tooltip>
              <TooltipTrigger asChild>
                <span
                  className="truncate min-w-0 whitespace-nowrap cursor-pointer hover:text-primary"
                  onClick={() => {
                    if (fn?.span) {
                      vscode.postMessage({
                        command: 'jumpToFile',
                        span: createSpan(fn.span),
                      });
                    }
                  }}
                >
                  {functionName}
                </span>
              </TooltipTrigger>
              <TooltipContent>
                {functionName}
              </TooltipContent>
            </Tooltip>
            <Popover open={functionOpen} onOpenChange={setFunctionOpen}>
              <PopoverTrigger asChild>
                <Button
                  variant="ghost"
                  size="sm"
                >
                  <ChevronDown className="h-4 w-4" />
                </Button>
              </PopoverTrigger>
              <PopoverContent className="min-w-fit p-0">
                <Command>
                  <CommandInput placeholder="Search functions..." className="!outline-none focus:!outline-none" />
                  <CommandList>
                    <CommandEmpty>No function found.</CommandEmpty>
                    <CommandGroup>
                      {functions.map((func) => (
                        <CommandItem
                          key={func.name}
                          value={func.name}
                          onSelect={() => {
                            const firstTest = func.tests[0];
                            if (firstTest) {
                              setSelectedItem(func.name, firstTest);
                            }
                            setFunctionOpen(false);
                          }}
                        >
                          <Check
                            className={cn(
                              "mr-2 h-4 w-4",
                              functionName === func.name
                                ? "opacity-100"
                                : "opacity-0"
                            )}
                          />
                          {func.name}
                        </CommandItem>
                      ))}
                    </CommandGroup>
                  </CommandList>
                </Command>
              </PopoverContent>
            </Popover>
          </div>
        </BreadcrumbItem>
        {/* <BreadcrumbSeparator>/</BreadcrumbSeparator> */}
        <BreadcrumbItem className="flex items-center gap-1">
          <div className="flex items-center gap-1 min-w-0 max-w-[120px] sm:max-w-[240px] md:max-w-[300px] shrink">
            <FlaskConical className="size-4 mr-2 shrink-0" />
            <Tooltip>
              <TooltipTrigger asChild>
                <span
                  className="truncate min-w-0 whitespace-nowrap cursor-pointer hover:text-primary"
                  onClick={() => {
                    if (tc?.span) {
                      vscode.postMessage({
                        command: 'jumpToFile',
                        span: createSpan(tc.span),
                      });
                    }
                  }}
                >
                  {testName}
                </span>
              </TooltipTrigger>
              <TooltipContent>
                {testName}
              </TooltipContent>
            </Tooltip>
            <Popover open={testOpen} onOpenChange={setTestOpen}>
              <PopoverTrigger asChild>
                <Button
                  variant="ghost"
                  size="sm"
                >
                  <ChevronDown className="h-4 w-4" />
                </Button>
              </PopoverTrigger>
              <PopoverContent className="min-w-fit p-0">
                <Command>
                  <CommandInput placeholder="Search tests..." className="!outline-none focus:!outline-none" />
                  <CommandList>
                    <CommandEmpty>No test found.</CommandEmpty>
                    <CommandGroup>
                      {availableTests.map((test) => (
                        <CommandItem
                          key={test}
                          value={test}
                          onSelect={() => {
                            setSelectedItem(functionName, test);
                            setTestOpen(false);
                          }}
                        >
                          <Check
                            className={cn(
                              "mr-2 h-4 w-4",
                              testName === test
                                ? "opacity-100"
                                : "opacity-0"
                            )}
                          />
                          {test}
                        </CommandItem>
                      ))}
                    </CommandGroup>
                  </CommandList>
                </Command>
              </PopoverContent>
            </Popover>
          </div>
        </BreadcrumbItem>
      </BreadcrumbList>
    </Breadcrumb>
  );
};
