'use client';

import { Button } from '@baml/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@baml/ui/dropdown-menu';
import { cn } from '@baml/ui/lib/utils';
import { SidebarTrigger } from '@baml/ui/sidebar';
import { Tooltip, TooltipContent, TooltipTrigger } from '@baml/ui/tooltip';
import { TooltipProvider } from '@baml/ui/tooltip';
import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai';
import {
  BarChart2,
  Check,
  Copy,
  Key,
  Play,
  Settings,
  Split,
  Workflow,
} from 'lucide-react';
import React from 'react';
import { ThemeToggle } from '../theme/ThemeToggle';
import {
  areTestsRunningAtom,
  selectedItemAtom,
  showEnvDialogAtom,
} from './atoms';
import { areEnvVarsMissingAtom } from './atoms';
import { FunctionTestName } from './function-test-name';
import { renderedPromptAtom } from './prompt-preview/prompt-preview-content';
import { isParallelTestsEnabledAtom } from './prompt-preview/test-panel/atoms';
import { useRunBamlTests } from './prompt-preview/test-panel/test-runner';

// Check if we're in a VSCode environment
const isVSCodeEnvironment =
  typeof window !== 'undefined' && !('vscode' in window);

export const displaySettingsAtom = atom({
  showTokens: false,
  showClientCallGraph: false,
  showParallelTests: false,
});

const RunButton: React.FC<{ className?: string }> = ({ className }) => {
  const runBamlTests = useRunBamlTests();
  const isRunning = useAtomValue(areTestsRunningAtom);
  const selected = useAtomValue(selectedItemAtom);
  return (
    <Button
      variant="default"
      size="xs"
      className={cn(
        'items-center bg-purple-500 hover:bg-purple-700 disabled:bg-muted disabled:text-muted-foreground dark:bg-purple-600 text-white dark:hover:bg-purple-800 gap-2',
        className,
      )}
      disabled={isRunning || selected === undefined}
      onClick={() => {
        if (selected) {
          void runBamlTests([
            { functionName: selected[0], testName: selected[1] },
          ]);
        }
      }}
    >
      <Play className="size-3" />
      <div className="text-xs hidden md:block">Run Test</div>
    </Button>
  );
};

export const isClientCallGraphEnabledAtom = atom(false);

export function PreviewToolbar() {
  const [displaySettings, setDisplaySettings] = useAtom(displaySettingsAtom);
  const selections = useAtomValue(selectedItemAtom);
  const setShowEnvDialog = useSetAtom(showEnvDialogAtom);

  const options: {
    label: string;
    icon: React.FC<React.SVGProps<SVGSVGElement>>;
    value: 'tokens';
  }[] = [{ label: 'Token Counts', icon: BarChart2, value: 'tokens' }];

  const areEnvVarsMissing = useAtomValue(areEnvVarsMissingAtom);
  const [isClientCallGraphEnabled, setIsClientCallGraphEnabled] = useAtom(
    isClientCallGraphEnabledAtom,
  );
  const renderedPrompt = useAtomValue(renderedPromptAtom);
  const [showCopied, setShowCopied] = React.useState(false);
  const [isParallelTestsEnabled, setIsParallelTestsEnabled] = useAtom(
    isParallelTestsEnabledAtom,
  );

  const handleCopy = () => {
    if (!renderedPrompt) return;
    navigator.clipboard.writeText(
      renderedPrompt
        .as_chat()
        ?.map(
          (msg) =>
            `${msg.role}:\n${msg.parts.map((part) => part.as_text()).join('\n')}`,
        )
        .join('\n\n') ?? '',
    );
    setShowCopied(true);
    setTimeout(() => setShowCopied(false), 1500);
  };

  return (
    <div className="flex flex-col gap-1">
      <div
        className={cn(
          'flex flex-row gap-1 items-center',
          selections === undefined ? 'justify-end' : 'justify-start',
        )}
      >
        {selections !== undefined && (
          <div className="flex flex-row items-center gap-2">
            <FunctionTestName
              functionName={selections[0]}
              testName={selections[1]}
            />
            <TooltipProvider>
              <Tooltip delayDuration={100}>
                <TooltipTrigger asChild>
                  <span>
                    <RunButton className="ml-1" />
                  </span>
                </TooltipTrigger>
                <TooltipContent>
                  <p>{`Run ${selections[1]}`}</p>
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
            <TooltipProvider>
              <Tooltip delayDuration={100}>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="xs"
                    className="flex gap-2 items-center text-muted-foreground/70 hover:text-foreground"
                    onClick={handleCopy}
                  >
                    {showCopied ? (
                      <Check className="size-4" />
                    ) : (
                      <Copy className="size-4" />
                    )}
                    {showCopied ? (
                      <span className="text-sm hidden md:block">Copied!</span>
                    ) : (
                      <span className="text-sm hidden md:block">
                        Copy Prompt
                      </span>
                    )}
                  </Button>
                </TooltipTrigger>
                <TooltipContent>
                  <p>Copy Prompt</p>
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
          </div>
        )}

        <TooltipProvider>
          <Tooltip delayDuration={100}>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="xs"
                className="flex gap-2 items-center text-muted-foreground/70 ml-auto"
                onClick={() => setShowEnvDialog(true)}
              >
                <div className="relative">
                  <Key className="size-4" />
                  {areEnvVarsMissing && (
                    <div className="absolute -top-1 -right-1 w-2 h-2 bg-orange-500 rounded-full" />
                  )}
                </div>
                <span className="text-sm hidden md:block">API Keys</span>
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              <p>Manage API Keys</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="xs"
              className="flex gap-2 items-center text-muted-foreground/70"
            >
              <Settings className="size-4" />
              <span className="text-sm hidden md:block">Settings</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            {/* <DropdownMenuLabel>Display</DropdownMenuLabel> */}
            {/* {options.map((option) => (
              <DropdownMenuCheckboxItem
                key={option.label}
                checked={displaySettings. === option.value}
                onCheckedChange={() => setDisplaySettings(option.value)}
              >
                <option.icon className='mr-2 size-4' />
                {option.label}
              </DropdownMenuCheckboxItem>
            ))} */}
            {/* <DropdownMenuSeparator /> */}
            <DropdownMenuLabel>Testing</DropdownMenuLabel>
            <DropdownMenuCheckboxItem
              checked={isParallelTestsEnabled}
              onCheckedChange={setIsParallelTestsEnabled}
            >
              <Split className="mr-2 size-4" />
              Enable Parallel Testing
            </DropdownMenuCheckboxItem>
            <DropdownMenuSeparator />
            <DropdownMenuLabel>Visualization</DropdownMenuLabel>
            <DropdownMenuCheckboxItem
              checked={isClientCallGraphEnabled}
              onCheckedChange={setIsClientCallGraphEnabled}
            >
              <Workflow className="mr-2 w-4 h-4" />
              LLM Client Call Graph
            </DropdownMenuCheckboxItem>
          </DropdownMenuContent>
        </DropdownMenu>

        <SidebarTrigger className="flex gap-2 items-center text-muted-foreground/70 hover:text-foreground" />

        {!isVSCodeEnvironment && <ThemeToggle />}
      </div>

      <div className="flex items-center space-x-4 w-full" />
    </div>
  );
}
