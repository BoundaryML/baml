'use client';

import { Button } from '@baml/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@baml/ui/tabs';
import { Tooltip, TooltipContent, TooltipTrigger } from '@baml/ui/tooltip';
import { TooltipProvider } from '@baml/ui/tooltip';
import { useSidebar } from '@baml/ui/sidebar';
import { useAtom, useAtomValue, useSetAtom } from 'jotai';
import { BarChart2, Check, Copy, Server, Eye, Code2, Network } from 'lucide-react';
import React, { useEffect } from 'react';
import { runtimeAtom, betaFeatureEnabledAtom } from '../../atoms';
import { selectionAtom, activeTabAtom, viewModeAtom } from '../atoms';
import { displaySettingsAtom } from '../preview-toolbar';
import { PromptPreviewContent } from './prompt-preview-content';
import { renderedPromptAtom } from './prompt-preview-content';
import { PromptPreviewCurl, curlAtom } from './prompt-preview-curl';
import { GraphView } from './graph-view';
import { ReactFlowProvider } from '@xyflow/react';

const GraphHost = () => (
  <div className="h-full">
    <ReactFlowProvider>
      <GraphView />
    </ReactFlowProvider>
  </div>
);

// Function Metadata component (client info badge)
const FunctionMetadata: React.FC = () => {
  const { selectedFn } = useAtomValue(selectionAtom);
  const { rt } = useAtomValue(runtimeAtom);
  const { open: isSidebarOpen } = useSidebar();

  if (!selectedFn) return null;
  if (!rt) return null;

  const clientName = selectedFn?.clientName;

  // Hide text when sidebar is open or on smaller screens
  const getButtonTextClass = () => {
    return 'font-mono text-xs hidden md:inline truncate max-w-48';
  };

  const metadataItems = [];

  if (clientName) {
    metadataItems.push(
      <TooltipProvider key="provider">
        <Tooltip delayDuration={100}>
          <TooltipTrigger asChild>
            <div className="flex items-center gap-1 px-2 py-1.5 bg-muted/50 rounded-md border">
              <Server className="size-4" />
              <span className={getButtonTextClass()}>
                {clientName}
              </span>
            </div>
          </TooltipTrigger>
          <TooltipContent side="top" className="p-4 w-64">
            <div className="space-y-3">
              <div className="text-xs font-medium text-foreground">
                Client Information
              </div>
              <div className="space-y-2">
                <div className="flex items-center gap-2 text-xs">
                  <Server className="size-3 text-muted-foreground" />
                  <span className="font-mono text-xs text-muted-foreground">
                    Client Name:
                  </span>
                </div>
                <div className="px-3 py-2 bg-muted/50 rounded border">
                  <p className="font-mono text-xs break-all">{clientName}</p>
                </div>
              </div>
            </div>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>,
    );
  }

  if (metadataItems.length === 0) return null;

  return (
    <div className="flex items-center gap-2 text-xs text-muted-foreground">
      {metadataItems}
    </div>
  );
};

/**
 * UnifiedPromptPreview - Merges PromptPreview and WorkflowApp functionality
 *
 * This component:
 * - Shows Preview/cURL tabs for standalone LLM functions
 * - Adds a Graph tab for LLM functions that are part of a workflow
 * - Hides tabs entirely for non-LLM functions in workflows (just shows graph)
 */
export const UnifiedPromptPreview = () => {
  const [displaySettings, setDisplaySettings] = useAtom(displaySettingsAtom);
  const renderedPrompt = useAtomValue(renderedPromptAtom);
  const { open: isSidebarOpen } = useSidebar();
  const viewMode = useAtomValue(viewModeAtom);
  const [activeTab, setActiveTab] = useAtom(activeTabAtom);
  const [showCopied, setShowCopied] = React.useState(false);

  // Auto-switch to default tab when view mode changes
  useEffect(() => {
    setActiveTab(viewMode.defaultTab);
  }, [viewMode.defaultTab, viewMode.showLLMTabs, setActiveTab]);


  // Hide text when sidebar is open or on smaller screens
  const getButtonTextClass = () => {
    if (isSidebarOpen) {
      return 'text-sm hidden whitespace-nowrap';
    }
    return 'text-sm hidden md:block whitespace-nowrap';
  };

  const handleCopy = () => {
    // If the cURL tab is active, we can't copy here - the PromptPreviewCurl component handles its own copying
    if (activeTab === 'curl') {
      // The cURL component has its own copy button
      return;
    }

    // Copy the human-readable prompt preview
    if (!renderedPrompt) return;

    let textToCopy = '';
    if (renderedPrompt.type === 'chat' && renderedPrompt.messages) {
      textToCopy = renderedPrompt.messages
        .map(
          (msg) =>
            `${msg.role}:\n${msg.parts.map((part) => part.content).join('\n')}`,
        )
        .join('\n\n');
    } else if (renderedPrompt.type === 'completion' && renderedPrompt.text) {
      textToCopy = renderedPrompt.text;
    }

    navigator.clipboard.writeText(textToCopy);
    setShowCopied(true);
    setTimeout(() => setShowCopied(false), 1500);
  };

  // Only show graph when graph tab exists and is active
  const showGraph = viewMode.showGraphTab && activeTab === 'graph';

  // If no tabs should be shown (non-LLM function in workflow), render graph directly
  return (
    <div className="relative h-full">
      {showGraph && <GraphHost />}
      {viewMode.showTabBar && (
        <Tabs
          value={activeTab}
          onValueChange={(v) => setActiveTab(v as any)}
          className="absolute inset-0 flex flex-col pointer-events-none gap-0"
        >
          <div className="pointer-events-auto flex items-center justify-between gap-2 px-2 py-0">
            <div className="flex items-center gap-2">
              <TabsList>
                {viewMode.showLLMTabs && (
                  <>
                    <TabsTrigger value="preview">
                      <Eye className="size-3.5" />
                      <span className="ml-1">Prompt</span>
                    </TabsTrigger>
                    <TabsTrigger value="curl">
                      <Code2 className="size-3.5" />
                      <span className="ml-1">cURL</span>
                    </TabsTrigger>
                  </>
                )}
                {viewMode.showGraphTab && (
                  <TabsTrigger value="graph">
                    <Network className="size-3.5" />
                    <span className="ml-1">Workflow</span>
                  </TabsTrigger>
                )}
              </TabsList>
              <FunctionMetadata />
              {viewMode.showLLMTabs && activeTab !== 'curl' && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="flex items-center gap-2 text-muted-foreground/70 hover:text-foreground"
                  onClick={handleCopy}
                >
                  {showCopied ? (
                    <Check className="size-4 flex-shrink-0" />
                  ) : (
                    <Copy className="size-4 flex-shrink-0" />
                  )}
                  <span className={getButtonTextClass()}>
                    {showCopied ? 'Copied!' : 'Copy Prompt'}
                  </span>
                </Button>
              )}
            </div>
            <div className="flex items-center gap-2">
              {activeTab === 'preview' && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="flex items-center gap-2 text-muted-foreground/70 hover:text-foreground"
                  onClick={() =>
                    setDisplaySettings((prev) => ({
                      ...prev,
                      showTokens: !prev.showTokens,
                    }))
                  }
                >
                  <BarChart2 className="size-4" />
                  <span className={getButtonTextClass()}>
                    {displaySettings.showTokens ? 'Hide Tokens' : 'Show Tokens'}
                  </span>
                </Button>
              )}
            </div>
          </div>
          {viewMode.showLLMTabs && (
            <>
              <TabsContent
                value="preview"
                className="flex-1 pointer-events-auto bg-background/95 mt-0 pt-1 overflow-auto px-2"
              >
                {activeTab === 'preview' && <PromptPreviewContent />}
              </TabsContent>
              <TabsContent
                value="curl"
                className="flex-1 pointer-events-auto bg-background/95 mt-0 pt-1 overflow-auto px-2"
              >
                {activeTab === 'curl' && <PromptPreviewCurl />}
              </TabsContent>
            </>
          )}
          {viewMode.showGraphTab && (
            <TabsContent value="graph" className="flex-1 pointer-events-none" />
          )}
        </Tabs>
      )}
    </div>
  );
};
