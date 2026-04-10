'use client';

import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { atom, useAtom, useAtomValue } from 'jotai';
import { Braces, Bug, ChevronDown, FileJson, PlayCircle } from 'lucide-react';
import React from 'react';
import { selectedItemAtom } from './atoms';
import { areTestsRunningAtom } from './atoms';
import { FunctionTestName } from './function-test-name';
import { useRunTests } from './prompt-preview/test-panel/test-runner';

export const renderModeAtom = atom<'prompt' | 'curl' | 'tokens'>('prompt');

const RunButton: React.FC = () => {
  const { setRunningTests } = useRunTests();
  const isRunning = useAtomValue(areTestsRunningAtom);
  const selected = useAtomValue(selectedItemAtom);
  return (
    <Button
      variant="outline"
      className="h-fit border-green-500 bg-green-500 px-2 py-1 text-xs text-white transition-colors duration-200 hover:bg-green-600 hover:text-white disabled:border-muted-foreground disabled:text-muted-foreground disabled:hover:bg-white disabled:hover:text-muted-foreground"
      disabled={isRunning || selected === undefined}
      onClick={() => {
        if (selected) {
          void setRunningTests([
            { functionName: selected[0], testName: selected[1] },
          ]);
        }
      }}
    >
      <PlayCircle className="mr-0 h-4 w-4" />
      Run
    </Button>
  );
};

export default function Component() {
  const [renderMode, setRenderMode] = useAtom(renderModeAtom);
  const selections = useAtomValue(selectedItemAtom);

  const options: {
    label: string;
    icon: React.FC<React.SVGProps<SVGSVGElement>>;
    value: 'prompt' | 'curl' | 'tokens';
  }[] = [
    { label: 'Prompt', icon: FileJson, value: 'prompt' },
    { label: 'Token Visualization', icon: Braces, value: 'tokens' },
    { label: 'Raw cURL', icon: Bug, value: 'curl' },
  ];

  const selectedOption = options.find((opt) => opt.value === renderMode);

  const SelectedIcon = selectedOption?.icon || FileJson;

  return (
    <div className="flex flex-col gap-1">
      {selections !== undefined && (
        <FunctionTestName
          functionName={selections[0]}
          testName={selections[1]}
        />
      )}
      <div className="flex w-full items-center space-x-4">
        <RunButton />
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="outline"
              className="text-black-600 h-fit bg-white px-2 py-1 text-xs transition-colors duration-200 hover:bg-indigo-100"
            >
              <SelectedIcon className="mr-2 h-4 w-4" />
              {selectedOption?.label}
              <ChevronDown className="ml-2 h-4 w-4 opacity-50" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            align="start"
            className="mt-1 rounded-md bg-white p-1 shadow-lg"
          >
            {options.map((option) => (
              <DropdownMenuItem
                key={option.label}
                onSelect={() => setRenderMode(option.value)}
                className="flex items-center rounded-md px-2 py-1 text-xs text-gray-700 transition-colors duration-200 hover:bg-indigo-100 hover:text-indigo-600"
              >
                <option.icon className="mr-2 h-4 w-4" />
                {option.label}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}
