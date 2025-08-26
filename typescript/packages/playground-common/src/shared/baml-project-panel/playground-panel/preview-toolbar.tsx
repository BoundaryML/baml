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
import { Key, Play, Settings } from 'lucide-react';
import type React from 'react';
import {
  type BamlConfigAtom,
  bamlConfig,
} from '../../../baml_wasm_web/bamlConfig';
import { areApiKeysMissingAtom } from '../../../components/api-keys-dialog/atoms';
import { showApiKeyDialogAtom } from '../../../components/api-keys-dialog/atoms';
import { proxyUrlAtom } from '../atoms';
import { ThemeToggle } from '../theme/ThemeToggle';
import { vscode } from '../vscode';
import { areTestsRunningAtom, selectedItemAtom, selectionAtom } from './atoms';
import { FunctionTestName } from './function-test-name';
import { isParallelTestsEnabledAtom } from './prompt-preview/test-panel/atoms';
import { useRunBamlTests } from './prompt-preview/test-panel/test-runner';
import { betaFeatureEnabledAtom, isVSCodeEnvironment } from '../feature-flags';
import { vscodeSettingsAtom } from '../atoms';

export const displaySettingsAtom = atom({
  showTokens: false,
  showClientCallGraph: false,
  showParallelTests: false,
});

// RunButton component
const RunButton: React.FC<{ className?: string }> = ({ className }) => {
  const runBamlTests = useRunBamlTests();
  const isRunning = useAtomValue(areTestsRunningAtom);
  const selected = useAtomValue(selectedItemAtom);

  return (
    <Button
      variant="default"
      size="xs"
      className={cn('cursor-pointer items-center gap-2 flex-shrink-0 min-w-fit bg-purple-600 hover:bg-purple-700 text-white', className)}
      disabled={isRunning || selected === undefined}
      onClick={() => {
        if (selected) {
          void runBamlTests([
            { functionName: selected[0], testName: selected[1] },
          ]);
        }
      }}
    >
      <Play className="size-4 flex-shrink-0" />
      <span className="text-sm whitespace-nowrap">Run Test</span>
    </Button>
  );
};

export const isClientCallGraphEnabledAtom = atom(false);

export function PreviewToolbar() {
  const selections = useAtomValue(selectedItemAtom);
  const { selectedFn } = useAtomValue(selectionAtom);
  const setShowApiKeyDialog = useSetAtom(showApiKeyDialogAtom);
  const { open: isSidebarOpen } = useSidebar();

  // Detect platform for keyboard shortcut
  const isMac =
    typeof window !== 'undefined' &&
    navigator.userAgent.toUpperCase().indexOf('MAC') >= 0;
  const sidebarShortcut = isMac ? 'Cmd+U' : 'Ctrl+U';

  const areApiKeysMissing = useAtomValue(areApiKeysMissingAtom);
  const [isParallelTestsEnabled, setIsParallelTestsEnabled] = useAtom(
    isParallelTestsEnabledAtom,
  );
  const proxySettings = useAtomValue(proxyUrlAtom);
  const setBamlConfig = useSetAtom(bamlConfig);
  
  // Beta feature flag settings
  const [betaFeatureEnabled, setBetaFeatureEnabled] = useAtom(betaFeatureEnabledAtom);
  const vscodeSettings = useAtomValue(vscodeSettingsAtom);
  const isInVSCode = isVSCodeEnvironment();
  
  // For VSCode, use VSCode settings directly (read-only); for standalone, use atom
  const displayBetaEnabled = isInVSCode 
    ? (vscodeSettings?.featureFlags?.includes('beta') ?? false)
    : betaFeatureEnabled;
  
  const handleBetaToggle = (enabled: boolean) => {
    // This function only runs in standalone mode (not VSCode)
    setBetaFeatureEnabled(enabled);
    toast.success('Beta Features Toggled', {
      description: `Beta features ${enabled ? 'enabled' : 'disabled'}.`,
    });
  };

  // Hide text when sidebar is open or on smaller screens
  const getButtonTextClass = () => {
    if (isSidebarOpen) {
      return 'text-sm hidden whitespace-nowrap';
    }
    return 'text-sm hidden md:block whitespace-nowrap';
  };

  return (
    <div className="flex flex-col gap-1 overflow-hidden w-full">
      <div
        className={cn(
          'flex flex-row gap-1 items-center min-w-0 w-full',
          selectedFn === undefined ? 'justify-end' : 'justify-between',
        )}
      >
        {selectedFn !== undefined && (
          <div className="flex flex-col gap-1 min-w-0 flex-1 overflow-hidden">
            <div className="flex flex-row items-center gap-2 min-w-0">
              <div className="min-w-0 flex-1 overflow-hidden">
                <FunctionTestName
                  functionName={selectedFn.name}
                  testName={selections?.[1]}
                />
              </div>

              <RunButton />
            </div>
          </div>
        )}

        <div className="flex items-center gap-2 flex-shrink-0">
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
            <DropdownMenuContent align="start" className="min-w-fit p-0">
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
              <DropdownMenuLabel className="text-xs px-2 py-1.5">
                Testing
              </DropdownMenuLabel>
              <DropdownMenuCheckboxItem
                checked={isParallelTestsEnabled}
                onCheckedChange={setIsParallelTestsEnabled}
                className="text-sm px-2 py-1.5 pl-7"
              >
                Enable Parallel Testing
              </DropdownMenuCheckboxItem>
              <DropdownMenuSeparator />
              <DropdownMenuLabel className="text-xs px-2 py-1.5">
                Network
              </DropdownMenuLabel>
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
                className="text-sm px-2 py-1.5 pl-7"
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
              <DropdownMenuSeparator />
              <DropdownMenuLabel className="text-xs px-2 py-1.5">
                Experimental Features
              </DropdownMenuLabel>
              
              {/* Beta Features - Only show toggle in standalone fiddle, not in VSCode */}
              {!isInVSCode ? (
                <DropdownMenuCheckboxItem
                  checked={displayBetaEnabled}
                  onCheckedChange={handleBetaToggle}
                  className="text-sm px-2 py-1.5 pl-7"
                >
                  <TooltipProvider>
                    <Tooltip delayDuration={300}>
                      <TooltipTrigger asChild>
                        <span>Beta Features</span>
                      </TooltipTrigger>
                      <TooltipContent side="left" className="text-xs w-80">
                        Enable experimental BAML features and suppress experimental warnings.
                        <br />
                        <br />
                        <b>Standalone:</b> This setting is saved locally 
                        and persists across sessions.
                      </TooltipContent>
                    </Tooltip>
                  </TooltipProvider>
                </DropdownMenuCheckboxItem>
              ) : (
                /* VSCode - Show read-only status instead of toggle */
                <div className="text-sm px-2 py-1.5 pl-7 text-muted-foreground">
                  <TooltipProvider>
                    <Tooltip delayDuration={300}>
                      <TooltipTrigger asChild>
                        <span>
                          Beta Features: {displayBetaEnabled ? 'Enabled' : 'Disabled'}
                        </span>
                      </TooltipTrigger>
                      <TooltipContent side="left" className="text-xs w-80">
                        Beta features are controlled by VSCode settings.
                        <br />
                        <br />
                        <b>To modify:</b> Open VSCode settings and search for "baml.featureFlags"
                        <br />
                        <br />
                        Current status: {displayBetaEnabled ? 'Beta features are enabled' : 'Beta features are disabled'}
                      </TooltipContent>
                    </Tooltip>
                  </TooltipProvider>
                </div>
              )}
            </DropdownMenuContent>
          </DropdownMenu>

          <TooltipProvider>
            <Tooltip delayDuration={300}>
              <TooltipTrigger asChild>
                <SidebarTrigger />
              </TooltipTrigger>
              <TooltipContent>
                <div className="flex items-center gap-2">
                  <span>
                    {isSidebarOpen ? 'Close Sidebar' : 'Open Sidebar'}
                  </span>
                  <kbd className="pointer-events-none inline-flex h-5 select-none items-center gap-1 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground opacity-100">
                    {sidebarShortcut}
                  </kbd>
                </div>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>

          {!isVSCodeEnvironment && <ThemeToggle />}
        </div>
      </div>

      <div className="flex items-center space-x-4 w-full" />
    </div>
  );
}
