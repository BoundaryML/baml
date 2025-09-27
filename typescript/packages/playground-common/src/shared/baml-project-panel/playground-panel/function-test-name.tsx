import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbList,
} from '@baml/ui/breadcrumb';
import { Button } from '@baml/ui/button';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@baml/ui/command';
import { cn } from '@baml/ui/lib/utils';
import { Popover, PopoverContent, PopoverTrigger } from '@baml/ui/popover';
import { Tooltip, TooltipContent, TooltipTrigger } from '@baml/ui/tooltip';
import { useAtomValue, useSetAtom } from 'jotai';
import { atom } from 'jotai';
import { Check, ChevronDown, FlaskConical, FunctionSquare } from 'lucide-react';
import { useMemo, useState } from 'react';
import { vscode } from '../vscode';
import {
  functionObjectAtom,
  runtimeStateAtom,
  selectedItemAtom,
  testcaseObjectAtom,
} from './atoms';

interface FunctionTestNameProps {
  functionName: string;
  testName?: string | null;
  selected?: boolean;
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

  const currentFunction = functions.find((f) => f.name === functionName);
  const availableTests = currentFunction?.tests || [];


  // Component for function dropdown items with jumpToFile
  const FunctionDropdownItem = ({ func }: { func: { name: string; tests: string[] } }) => {
    const fnAtom = useMemo(() => functionObjectAtom(func.name), [func.name]);
    const fn = useAtomValue(fnAtom);

    return (
      <CommandItem
        key={func.name}
        value={func.name}
        onSelect={() => {
          const firstTest = func.tests[0];
          if (firstTest) {
            setSelectedItem(func.name, firstTest);
          } else {
            setSelectedItem(func.name, undefined);
          }
          setFunctionOpen(false);
          if (fn?.span) {
            vscode.jumpToFile(fn.span);
          }
        }}
      >
        <Check
          className={cn(
            'mr-2 h-4 w-4',
            functionName === func.name ? 'opacity-100' : 'opacity-0',
          )}
        />
        <span
          className="text-sm truncate cursor-pointer hover:text-primary hover:underline"
        >
          {func.name}
        </span>
      </CommandItem>
    );
  };

  // Component for test dropdown items with jumpToFile
  const TestDropdownItem = ({ test, functionName }: { test: string; functionName: string }) => {
    const tcAtom = useMemo(
      () => testcaseObjectAtom({ functionName, testcaseName: test }),
      [functionName, test]
    );
    const tc = useAtomValue(tcAtom);

    return (
      <CommandItem
        key={test}
        value={test}
        onSelect={() => {
          setSelectedItem(functionName, test);
          setTestOpen(false);
          if (tc?.span) {
            vscode.jumpToFile(tc.span);
          }
        }}
      >
        <Check
          className={cn(
            'mr-2 h-4 w-4',
            testName === test ? 'opacity-100' : 'opacity-0',
          )}
        />
        <span
          className="text-sm truncate cursor-pointer hover:text-primary hover:underline"
        >
          {test}
        </span>
      </CommandItem>
    );
  };

  return (
    <Breadcrumb>
      <BreadcrumbList className="flex flex-nowrap overflow-hidden min-w-0">
        <BreadcrumbItem className="flex items-center gap-1 min-w-0">
          <Popover open={functionOpen} onOpenChange={setFunctionOpen}>
            <div className="flex items-center gap-1 min-w-0 flex-1">
              <FunctionSquare className="size-4 mr-2 shrink-0" />
              <Tooltip>
                <TooltipTrigger asChild>
                  <button
                    type="button"
                    className="truncate min-w-0 whitespace-nowrap cursor-pointer hover:text-primary bg-transparent border-none p-0 text-left flex-1"
                    onClick={() => {
                      if (fn?.span) {
                        vscode.jumpToFile(fn.span);
                      }
                    }}
                  >
                    {functionName}
                  </button>
                </TooltipTrigger>
                <TooltipContent>{functionName}</TooltipContent>
              </Tooltip>
              <PopoverTrigger asChild>
                <Button variant="ghost" size="sm" className="shrink-0">
                  <ChevronDown className="h-4 w-4" />
                </Button>
              </PopoverTrigger>
              <PopoverContent className="min-w-fit p-0" align="end">
                <Command>
                  <CommandInput
                    placeholder="Search functions..."
                    className="!outline-none focus:!outline-none"
                  />
                  <CommandList>
                    <CommandEmpty>No function found.</CommandEmpty>
                    <CommandGroup>
                      {functions.map((func) => (
                        <FunctionDropdownItem key={func.name} func={func} />
                      ))}
                    </CommandGroup>
                  </CommandList>
                </Command>
              </PopoverContent>
            </div>
          </Popover>
        </BreadcrumbItem>
        {testName && (
          <BreadcrumbItem className="flex items-center gap-1 min-w-0">
            <div className="flex items-center gap-1 min-w-0 flex-1">
              <FlaskConical className="size-4 mr-2 shrink-0" />
              <Tooltip>
                <TooltipTrigger asChild>
                  <button
                    type="button"
                    className="truncate min-w-0 whitespace-nowrap cursor-pointer hover:text-primary bg-transparent border-none p-0 text-left flex-1"
                    onClick={() => {
                      if (tc?.span) {
                        vscode.jumpToFile(tc.span);
                      }
                    }}
                  >
                    {testName}
                  </button>
                </TooltipTrigger>
                <TooltipContent>{testName}</TooltipContent>
              </Tooltip>
              <Popover open={testOpen} onOpenChange={setTestOpen}>
                <PopoverTrigger asChild>
                  <Button variant="ghost" size="sm" className="shrink-0">
                    <ChevronDown className="h-4 w-4" />
                  </Button>
                </PopoverTrigger>
                <PopoverContent className="min-w-fit p-0">
                  <Command>
                    <CommandInput
                      placeholder="Search tests..."
                      className="!outline-none focus:!outline-none"
                    />
                    <CommandList>
                      <CommandEmpty>No test found.</CommandEmpty>
                      <CommandGroup>
                        {availableTests.map((test) => (
                          <TestDropdownItem key={test} test={test} functionName={functionName} />
                        ))}
                      </CommandGroup>
                    </CommandList>
                  </Command>
                </PopoverContent>
              </Popover>
            </div>
          </BreadcrumbItem>
        )}
      </BreadcrumbList>
    </Breadcrumb>
  );
};
