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
import { useSidebar } from '@baml/ui/sidebar';
import { toast } from '@baml/ui/sonner';
import { Tooltip, TooltipContent, TooltipTrigger } from '@baml/ui/tooltip';
import { TooltipProvider } from '@baml/ui/tooltip';
import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai';
import { BarChart2, Check, Copy, Key, Play, Settings } from 'lucide-react';
import React from 'react';
import {
  type BamlConfigAtom,
  bamlConfig,
} from '../../../baml_wasm_web/bamlConfig';
import { areApiKeysMissingAtom } from '../../../components/api-keys-dialog/atoms';
import { showApiKeyDialogAtom } from '../../../components/api-keys-dialog/atoms';
import { proxyUrlAtom } from '../atoms';
import { ThemeToggle } from '../theme/ThemeToggle';
import { vscode } from '../vscode';
import { areTestsRunningAtom, selectedItemAtom } from './atoms';
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
      className={cn('items-center gap-2 flex-shrink-0 min-w-fit', className)}
      disabled={isRunning || selected === undefined}
      onClick={() => {
        if (selected) {
          void runBamlTests([
            { functionName: selected[0], testName: selected[1] },
          ]);
        }
      }}
    >
      <Play className="size-3 flex-shrink-0" />
      <div className="text-xs hidden md:block whitespace-nowrap">Run Test</div>
    </Button>
  );
};

export const isClientCallGraphEnabledAtom = atom(false);

export function PreviewToolbar() {
  const selections = useAtomValue(selectedItemAtom);
  const setShowApiKeyDialog = useSetAtom(showApiKeyDialogAtom);
  const { open: isSidebarOpen } = useSidebar();

  const options: {
    label: string;
    icon: React.FC<React.SVGProps<SVGSVGElement>>;
    value: 'tokens';
  }[] = [{ label: 'Token Counts', icon: BarChart2, value: 'tokens' }];

  const areApiKeysMissing = useAtomValue(areApiKeysMissingAtom);
  const renderedPrompt = useAtomValue(renderedPromptAtom);
  const [showCopied, setShowCopied] = React.useState(false);
  const [isParallelTestsEnabled, setIsParallelTestsEnabled] = useAtom(
    isParallelTestsEnabledAtom,
  );
  const proxySettings = useAtomValue(proxyUrlAtom);
  const setBamlConfig = useSetAtom(bamlConfig);

  // Adjust text visibility based on sidebar state
  const getButtonTextClass = () => {
    if (isSidebarOpen) {
      return 'text-sm hidden lg:block whitespace-nowrap';
    }
    return 'text-sm hidden md:block whitespace-nowrap';
  };

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
    <div className="flex flex-col gap-1 overflow-x-clip">
      <div
        className={cn(
          'flex flex-row gap-1 items-center min-w-0',
          selections === undefined ? 'justify-end' : 'justify-start',
        )}
        style={{ minWidth: 'fit-content' }}
      >
        {selections !== undefined && (
          <div className="flex flex-row items-center gap-2 min-w-0 flex-shrink">
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
                    className="flex gap-2 items-center text-muted-foreground/70 hover:text-foreground flex-shrink-0 min-w-fit"
                    onClick={handleCopy}
                  >
                    {showCopied ? (
                      <Check className="size-4 flex-shrink-0" />
                    ) : (
                      <Copy className="size-4 flex-shrink-0" />
                    )}
                    {showCopied ? (
                      <span className={getButtonTextClass()}>Copied!</span>
                    ) : (
                      <span className={getButtonTextClass()}>Copy Prompt</span>
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

        <div className="flex items-center gap-2 ml-auto flex-shrink-0">
          <TooltipProvider>
            <Tooltip delayDuration={100}>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="xs"
                  className="flex gap-2 items-center text-muted-foreground/70 relative flex-shrink-0 min-w-fit"
                  onClick={() => setShowApiKeyDialog(true)}
                >
                  <Key className="size-4 flex-shrink-0" />
                  <span className={getButtonTextClass()}>API Keys</span>
                  {areApiKeysMissing && (
                    <div className="absolute top-0 -right-1 w-2 h-2 bg-orange-500 rounded-full" />
                  )}
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
                className="flex gap-2 items-center text-muted-foreground/70 flex-shrink-0 min-w-fit"
              >
                <Settings className="size-4 flex-shrink-0" />
                <span className={getButtonTextClass()}>Settings</span>
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
                Enable Parallel Testing
              </DropdownMenuCheckboxItem>
              <DropdownMenuSeparator />
              <DropdownMenuLabel>Network</DropdownMenuLabel>
              <DropdownMenuCheckboxItem
                checked={proxySettings.proxyEnabled}
                onCheckedChange={async (checked) => {
                  try {
                    await vscode.setProxySettings(!!checked);
                    // Update local config to reflect the change immediately
                    setBamlConfig((prev: BamlConfigAtom) => ({
                      ...prev,
                      config: {
                        ...prev.config,
                      },
                    }));
                  } catch (error) {
                    console.error('Failed to update proxy settings:', error);
                    toast.error('Error updating proxy settings', {
                      description: 'Please try again',
                    });
                  }
                }}
              >
                <TooltipProvider>
                  <Tooltip delayDuration={300}>
                    <TooltipTrigger asChild>
                      <span>VSCode Proxy (CORS bypass)</span>
                    </TooltipTrigger>
                    <TooltipContent side="left" className="text-xs w-80">
                      The BAML playground directly calls the LLM provider's API.
                      Some providers make it difficult for browsers to call
                      their API due to CORS restrictions.
                      <br />
                      <br />
                      To get around this, the BAML VSCode extension includes a{' '}
                      <b>localhost proxy</b> that sits between your browser and
                      the LLM provider's API.
                      <br />
                      <br />
                      <b>
                        BAML MAKES NO NETWORK CALLS BEYOND THE LLM PROVIDER'S
                        API YOU SPECIFY.
                      </b>
                    </TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              </DropdownMenuCheckboxItem>
            </DropdownMenuContent>
          </DropdownMenu>

          <SidebarTrigger />

          {!isVSCodeEnvironment && <ThemeToggle />}
        </div>
      </div>

      <div className="flex items-center space-x-4 w-full" />
    </div>
  );
}
