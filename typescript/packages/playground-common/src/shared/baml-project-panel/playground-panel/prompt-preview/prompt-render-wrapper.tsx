import { Button } from '@baml/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@baml/ui/tabs';
import { Tooltip, TooltipContent, TooltipTrigger } from '@baml/ui/tooltip';
import { TooltipProvider } from '@baml/ui/tooltip';
import { useAtom, useAtomValue } from 'jotai';
import { BarChart2, Check, Copy, Server } from 'lucide-react';
import React from 'react';
import { runtimeAtom } from '../../atoms';
import { selectionAtom } from '../atoms';
import { displaySettingsAtom } from '../preview-toolbar';
import { PromptPreviewContent } from './prompt-preview-content';
import { renderedPromptAtom } from './prompt-preview-content';
import { PromptPreviewCurl } from './prompt-preview-curl';
import { ClientGraphView } from './test-panel/components/ClientGraphView';

// FunctionMetadata component
const FunctionMetadata: React.FC = () => {
  const { selectedFn } = useAtomValue(selectionAtom);
  const { rt } = useAtomValue(runtimeAtom);

  if (!selectedFn) return null;
  if (!rt) return null;

  const clientName = selectedFn?.client_name(rt);

  const metadataItems = [];

  if (clientName) {
    metadataItems.push(
      <TooltipProvider key="provider">
        <Tooltip delayDuration={100}>
          <TooltipTrigger asChild>
            <div className="flex items-center gap-1 px-2 py-1.5 bg-muted/50 rounded-md border">
              <Server className="size-4" />
              <span className="font-mono text-xs hidden md:inline truncate max-w-48">
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

export const PromptRenderWrapper = () => {
  const [displaySettings, setDisplaySettings] = useAtom(displaySettingsAtom);
  const renderedPrompt = useAtomValue(renderedPromptAtom);
  const [showCopied, setShowCopied] = React.useState(false);

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
    <Tabs defaultValue="preview">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <TabsList>
            <TabsTrigger value="preview">Preview</TabsTrigger>
            <TabsTrigger value="curl">cURL</TabsTrigger>
            <TabsTrigger value="client-graph">Client Graph</TabsTrigger>
          </TabsList>
          <FunctionMetadata />
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
            <span className="text-sm hidden md:block">
              {showCopied ? 'Copied!' : 'Copy Prompt'}
            </span>
          </Button>
        </div>

        <div className="flex items-center gap-2">
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
            <span className="text-sm hidden md:block">
              {displaySettings.showTokens ? 'Hide Tokens' : 'Show Tokens'}
            </span>
          </Button>
        </div>
      </div>
      <TabsContent value="preview">
        <PromptPreviewContent />
      </TabsContent>
      <TabsContent value="curl">
        <PromptPreviewCurl />
      </TabsContent>
      <TabsContent value="client-graph">
        <ClientGraphView />
      </TabsContent>
    </Tabs>
  );
};
