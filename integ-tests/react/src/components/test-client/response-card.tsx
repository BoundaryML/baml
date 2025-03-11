'use client';

import { NetworkTimeline } from '@/components/network-timeline';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useResponseCardConfigWithQueryParams } from '@/lib/store';
import { cn } from '@/lib/utils';
import * as React from 'react';
import type {
  FunctionNames,
  HookOutput,
} from '../../../baml_client/react/hooks';
import { formatError } from './format-error';
import {
  JsonRenderer,
  MarkdownRenderer,
  RawRenderer,
  YamlRenderer,
} from './format-renderers';

type ResponseCardProps = {
  hookResult: HookOutput<FunctionNames>;
  hasStarted: boolean;
  functionName?: FunctionNames; // Keep this optional since we don't use it anymore
};

export function ResponseCard({
  hookResult,
  hasStarted,
  functionName,
}: ResponseCardProps) {
  const {
    isLoading,
    error,
    isError,
    data,
    streamData,
    isPending,
    isStreaming,
    isSuccess,
    finalData,
  } = hookResult;

  // Get the configuration from URL query parameters
  const { config } = useResponseCardConfigWithQueryParams();

  const dataRef = React.useRef<HTMLPreElement>(null);
  const streamDataRef = React.useRef<HTMLPreElement>(null);
  const finalDataRef = React.useRef<HTMLPreElement>(null);

  // Initialize active tab from config
  const [activeTab, setActiveTab] = React.useState<string>(config.defaultTab);

  // Update active tab when config.defaultTab changes
  React.useEffect(() => {
    setActiveTab(config.defaultTab);
  }, [config.defaultTab]);

  // Auto-scroll effect for data tab
  React.useEffect(() => {
    if (dataRef.current) {
      dataRef.current.scrollTop = dataRef.current.scrollHeight;
    }
  }, [data]);

  // Auto-scroll effect for stream data tab
  React.useEffect(() => {
    if (streamDataRef.current) {
      streamDataRef.current.scrollTop = streamDataRef.current.scrollHeight;
    }
  }, [streamData]);

  // Auto-scroll effect for final data tab
  React.useEffect(() => {
    if (finalDataRef.current) {
      finalDataRef.current.scrollTop = finalDataRef.current.scrollHeight;
    }
  }, [finalData]);

  React.useEffect(() => {
    if (error && config.showErrorTab) {
      // Automatically switch to the error tab when an error occurs
      setActiveTab('error');
    }
  }, [error, config.showErrorTab]);

  // Define the visible tabs/sections
  const visibleSections = [
    config.showDataTab ? { id: 'data', label: 'Data' } : null,
    config.showStreamDataTab
      ? { id: 'streamData', label: 'Stream Data' }
      : null,
    config.showFinalDataTab ? { id: 'finalData', label: 'Final Data' } : null,
    config.showErrorTab ? { id: 'error', label: 'Error' } : null,
  ].filter(Boolean) as { id: string; label: string }[];

  // Function to render the content for each section
  const renderSectionContent = (sectionId: string, className?: string) => {
    // Get the content based on the section ID
    const getContentForSection = () => {
      switch (sectionId) {
        case 'data':
          return data;
        case 'streamData':
          return streamData;
        case 'finalData':
          return finalData;
        case 'error':
          return error;
        default:
          return null;
      }
    };

    // Get the content for this section
    const content = getContentForSection();

    // Render error section differently
    if (sectionId === 'error') {
      return error ? (
        <div className="h-full space-y-4 overflow-y-auto">
          <Alert variant="destructive" className={className}>
            <AlertDescription>
              {(() => {
                const { title, message, statusCode, clientName } =
                  formatError(error);
                return (
                  <div className="space-y-2">
                    <div className="flex items-center gap-2">
                      <div className="break-words font-semibold">{title}</div>
                      {statusCode && (
                        <Badge variant="destructive">{statusCode}</Badge>
                      )}
                    </div>
                    {clientName && (
                      <Badge variant="destructive">{clientName}</Badge>
                    )}
                    <pre className="whitespace-pre-wrap break-words font-mono text-sm">
                      {message}
                    </pre>
                  </div>
                );
              })()}
            </AlertDescription>
          </Alert>
        </div>
      ) : (
        <div className="flex h-full items-center justify-center">
          <p className="text-muted-foreground">No errors to display</p>
        </div>
      );
    }

    // Get the appropriate class name for the content container
    const contentClassName = cn(
      'h-full overflow-y-auto whitespace-pre-wrap rounded-md bg-muted p-4 font-mono text-sm',
      className,
    );

    // If no content, show placeholder
    if (!content) {
      const placeholderText = `No ${sectionId} available`;
      return (
        <div className="flex h-full items-center justify-center">
          <p className="text-muted-foreground">{placeholderText}</p>
        </div>
      );
    }

    // Render content based on selected format
    switch (config.outputFormat) {
      case 'json':
        return <JsonRenderer content={content} className={contentClassName} />;
      case 'yaml':
        return <YamlRenderer content={content} className={contentClassName} />;
      case 'markdown':
        return (
          <MarkdownRenderer content={content} className={contentClassName} />
        );
      default:
        // Raw format is the default
        return <RawRenderer content={content} className={contentClassName} />;
    }
  };

  return (
    <div className="flex w-full flex-col items-center gap-6">
      {/* Keep the NetworkTimeline at the same width as the form, and only show if configured */}
      {config.showNetworkTimeline && (
        <div className="w-full max-w-xl">
          <NetworkTimeline hookResult={hookResult} hasStarted={hasStarted} />
        </div>
      )}

      {/* Allow sections/tabs to use full width */}
      <div className="w-full space-y-4">
        {visibleSections.length > 0 &&
          (config.displayMode === 'tabs' ? (
            // Tabs View
            <Tabs
              value={activeTab}
              onValueChange={(value: string) => setActiveTab(value)}
              className="mx-auto max-w-xl"
            >
              <TabsList className="mb-2 flex w-full">
                {visibleSections.map((section) => (
                  <TabsTrigger
                    key={section.id}
                    value={section.id}
                    className="flex-1"
                  >
                    {section.label}
                  </TabsTrigger>
                ))}
              </TabsList>

              {visibleSections.map((section) => (
                <TabsContent
                  key={section.id}
                  value={section.id}
                  className="mt-0"
                >
                  {renderSectionContent(section.id)}
                </TabsContent>
              ))}
            </Tabs>
          ) : (
            // Sections View - horizontal layout with full width utilization
            <div className="flex w-full flex-wrap gap-6 pb-4">
              {visibleSections.map((section, index) => (
                <div
                  key={section.id}
                  className="min-w-[300px] flex-1 overflow-hidden bg-background"
                  style={{
                    flex: `1 1 ${Math.min(450, Math.max(300, 100 / Math.min(visibleSections.length, 3)))}px`,
                    minHeight: '350px',
                  }}
                >
                  <div className="rounded-t-lg border border-border bg-muted/30 px-4 py-2">
                    <h3 className="font-medium text-md">{section.label}</h3>
                  </div>
                  <div className="h-[calc(100%-45px)] overflow-auto">
                    {renderSectionContent(
                      section.id,
                      'rounded-none rounded-b-lg',
                    )}
                  </div>
                </div>
              ))}
            </div>
          ))}
      </div>
    </div>
  );
}
