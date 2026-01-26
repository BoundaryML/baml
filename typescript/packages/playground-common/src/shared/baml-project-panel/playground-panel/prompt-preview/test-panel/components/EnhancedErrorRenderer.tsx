'use client';
import React, { useMemo, useCallback, useState } from 'react';
import { AlertCircle, RefreshCw, ExternalLink, WifiOff, ChevronDown, ChevronUp, Copy, Check, X, List } from 'lucide-react';
import { Button } from '@baml/ui/button';
import { useAtomValue } from 'jotai';
import { ErrorWarningDialog } from '../../../../../../components/ErrorWarningDialog';
import { diagnosticsAtom } from '../../../../../../sdk/atoms/core.atoms';


// Type definitions for error handlers
export interface ErrorContext {
  errorMessage: string;
  functionName?: string;
  testName?: string;
  onRetry?: () => void;
}

export interface CustomErrorRenderer {
  test: (errorMessage: string) => boolean;
  render: (context: ErrorContext) => React.ReactNode;
  priority: number; // Higher priority handlers are checked first
}

// Registry of custom error renderers
const errorRenderers: CustomErrorRenderer[] = [];

// Register a custom error renderer
export const registerErrorRenderer = (renderer: CustomErrorRenderer) => {
  errorRenderers.push(renderer);
  errorRenderers.sort((a, b) => b.priority - a.priority);
};

// Enhanced copy button component with state management
const CopyErrorButton: React.FC<{ errorMessage: string }> = ({ errorMessage }) => {
  const [copyStatus, setCopyStatus] = React.useState<'idle' | 'copying' | 'success' | 'error'>('idle');

  const handleCopy = async () => {
    setCopyStatus('copying');
    try {
      await navigator.clipboard.writeText(errorMessage);
      setCopyStatus('success');
      setTimeout(() => setCopyStatus('idle'), 2000);
    } catch (err) {
      console.error('Failed to copy to clipboard:', err);
      setCopyStatus('error');
      setTimeout(() => setCopyStatus('idle'), 2000);
    }
  };

  const getButtonStyle = () => {
    if (copyStatus === 'success') {
      return {
        color: '#16a34a',
        background: `linear-gradient(rgba(34, 197, 94, 0.05), rgba(34, 197, 94, 0.05)), var(--vscode-editor-background)`,
        borderColor: 'rgba(34, 197, 94, 0.3)',
      };
    }
    return {
      color: '#dc2626',
      background: `linear-gradient(rgba(220, 38, 38, 0.03), rgba(220, 38, 38, 0.03)), var(--vscode-editor-background)`,
      borderColor: 'rgba(220, 38, 38, 0.3)',
    };
  };

  return (
    <Button
      onClick={handleCopy}
      disabled={copyStatus === 'copying'}
      variant="outline"
      size="xs"
      className="flex gap-1 items-center text-xs px-2 py-0 rounded h-7 transition-all duration-200 border-[var(--vscode-panel-border)] text-[var(--vscode-charts-red)] bg-[var(--vscode-editor-background)] cursor-pointer hover:opacity-80 disabled:cursor-not-allowed"
      style={getButtonStyle()}
    >
      {copyStatus === 'copying' && (
        <div className="w-3 h-3 border border-red-600 border-t-transparent rounded-full animate-spin" />
      )}
      {copyStatus === 'success' && <Check className="w-3 h-3" />}
      {copyStatus === 'error' && <X className="w-3 h-3" />}
      {copyStatus === 'idle' && <Copy className="w-3 h-3" />}
      <span>
        {copyStatus === 'copying' && 'Copying...'}
        {copyStatus === 'success' && 'Copied!'}
        {copyStatus === 'error' && 'Failed'}
        {copyStatus === 'idle' && 'Copy Error'}
      </span>
    </Button>
  );
};

// Enhanced collapsible error details with webview-media styling
const ErrorDetails: React.FC<{ errorMessage: string }> = ({ errorMessage }) => {
  const [isExpanded, setIsExpanded] = React.useState(false);

  return (
    <>
      <Button
        onClick={() => setIsExpanded(!isExpanded)}
        aria-label={isExpanded ? 'Hide full error details' : 'Show full error details'}
        variant="outline"
        size="xs"
        className="inline-flex items-center gap-1 text-xs px-2 py-0 h-7 transition-colors duration-150 border-[var(--vscode-panel-border)] text-[var(--vscode-charts-red)] bg-[var(--vscode-editor-background)] cursor-pointer hover:opacity-80"
        style={{
          color: '#dc2626',
          background: 'var(--vscode-editor-background)',
          borderColor: 'rgba(220, 38, 38, 0.3)',
        }}
      >
        {isExpanded ? (
          <>
            <ChevronUp className="w-3 h-3" />
            <span>Hide full error details</span>
          </>
        ) : (
          <>
            <ChevronDown className="w-3 h-3" />
            <span>Show full error details</span>
          </>
        )}
      </Button>
      {isExpanded && (
        <div
          className="basis-full w-full mt-2 p-3 bg-[var(--vscode-editor-background)] border border-[var(--vscode-panel-border)] rounded-md"
          style={{
            background: `linear-gradient(rgba(220, 38, 38, 0.03), rgba(220, 38, 38, 0.03)), var(--vscode-editor-background)`,
            borderColor: 'rgba(220, 38, 38, 0.2)',
          }}
        >
          <pre className="text-xs whitespace-pre-wrap break-words font-mono text-[var(--vscode-charts-red)]" style={{ color: '#dc2626' }}>
            {errorMessage}
          </pre>
        </div>
      )}
    </>
  );
};

// Custom Alert-like components using red error styling
const ErrorAlert: React.FC<{
  children: React.ReactNode;
  variant?: 'default' | 'destructive';
  className?: string;
}> = ({ children, variant = 'default', className = '' }) => (
  <div
    className={`
      border border-[var(--vscode-panel-border)] bg-[var(--vscode-editor-background)] rounded-lg p-4
      ${variant === 'destructive' ? 'border-l-4 border-l-red-500' : ''}
      ${className}
    `}
    style={{
      background: `linear-gradient(rgba(220, 38, 38, 0.03), rgba(220, 38, 38, 0.03)), var(--vscode-editor-background)`,
      borderColor: 'rgba(220, 38, 38, 0.2)',
    }}
  >
    {children}
  </div>
);

const ErrorAlertTitle: React.FC<{
  children: React.ReactNode;
  className?: string;
}> = ({ children, className = '' }) => (
  <div className={`flex items-center gap-2 text-sm font-medium text-[var(--vscode-charts-red)] mb-3 ${className}`} style={{ color: '#dc2626' }}>
    {children}
  </div>
);

const ErrorAlertDescription: React.FC<{
  children: React.ReactNode;
  className?: string;
}> = ({ children, className = '' }) => (
  <div className={`text-[var(--vscode-charts-red)] ${className}`} style={{ color: '#dc2626' }}>
    {children}
  </div>
);

const ErrorBadge: React.FC<{
  children: React.ReactNode;
  variant?: 'outline';
  className?: string;
}> = ({ children, variant = 'outline', className = '' }) => (
  <span
    className={`
      inline-flex items-center px-2 py-1 rounded text-xs font-medium text-[var(--vscode-charts-red)]
      ${variant === 'outline' ? 'border border-[var(--vscode-panel-border)] bg-[var(--vscode-editor-background)]' : 'bg-red-500 text-white'}
      ${className}
    `}
    style={{
      color: variant === 'outline' ? '#dc2626' : undefined,
      ...(variant === 'outline' ? {
        background: `linear-gradient(rgba(220, 38, 38, 0.05), rgba(220, 38, 38, 0.05)), var(--vscode-editor-background)`,
        borderColor: 'rgba(220, 38, 38, 0.3)',
      } : {})
    }}
  >
    {children}
  </span>
);

// Error Footer Component for action buttons
const ErrorFooter: React.FC<{
  children: React.ReactNode;
  className?: string;
}> = ({ children, className = '' }) => (
  <div
    className={`flex flex-wrap gap-2 pt-3 mt-3 border-t border-[var(--vscode-panel-border)] ${className}`}
    style={{ borderTopColor: 'rgba(220, 38, 38, 0.2)' }}
  >
    {children}
  </div>
);

// Component to show error/warning count and access all errors
const AllErrorsButton: React.FC = () => {
  const [showDialog, setShowDialog] = useState(false);
  const diagnostics = useAtomValue(diagnosticsAtom) as Array<any>;
  const errors = diagnostics.filter((d) => d.type === 'error');
  const warnings = diagnostics.filter((d) => d.type === 'warning');

  if (errors.length === 0 && warnings.length === 0) {
    return null;
  }

  return (
    <>
      <Button
        variant="outline"
        size="xs"
        onClick={() => setShowDialog(true)}
        className="h-7 px-2 text-xs border-[var(--vscode-panel-border)] text-[var(--vscode-charts-red)] hover:bg-[var(--vscode-editor-background)] cursor-pointer hover:opacity-80"
        style={{
          color: '#dc2626',
          borderColor: 'rgba(220, 38, 38, 0.3)',
          background: 'var(--vscode-editor-background)',
        }}
        title={`View all ${errors.length + warnings.length} issue(s)`}
      >
        <List className="w-3 h-3 mr-1" />
        {errors.length > 0 && `${errors.length} error${errors.length > 1 ? 's' : ''}`}
        {errors.length > 0 && warnings.length > 0 && ', '}
        {warnings.length > 0 && `${warnings.length} warning${warnings.length > 1 ? 's' : ''}`}
      </Button>
      <ErrorWarningDialog open={showDialog} onOpenChange={setShowDialog} />
    </>
  );
};

// Default fallback error renderer
const DefaultErrorRenderer: React.FC<{ context: ErrorContext }> = ({ context }) => (
  <ErrorAlert variant="destructive">
    <ErrorAlertTitle>
      <AlertCircle className="h-4 w-4" />
      Syntax Error
    </ErrorAlertTitle>
    <ErrorAlertDescription className="space-y-3">
      <div
        className="p-3 bg-[var(--vscode-editor-background)] border border-[var(--vscode-panel-border)] rounded-md"
        style={{
          background: `linear-gradient(rgba(220, 38, 38, 0.06), rgba(220, 38, 38, 0.06)), var(--vscode-editor-background)`,
          borderColor: 'rgba(220, 38, 38, 0.3)',
        }}
      >
        <pre className="text-xs whitespace-pre-wrap break-all font-mono text-[var(--vscode-charts-red)]" style={{ color: '#dc2626' }}>
          {context.errorMessage}
        </pre>
      </div>

      <ErrorFooter>
        {context.onRetry && (
          <Button
            variant="outline"
            size="xs"
            onClick={context.onRetry}
            className="h-7 px-2 text-xs border-[var(--vscode-panel-border)] text-[var(--vscode-charts-red)] hover:bg-[var(--vscode-editor-background)] cursor-pointer hover:opacity-80"
            style={{
              color: '#dc2626',
              borderColor: 'rgba(220, 38, 38, 0.3)',
              background: 'var(--vscode-editor-background)',
            }}
          >
            <RefreshCw className="w-3 h-3 mr-1" />
            Retry
          </Button>
        )}
        <CopyErrorButton errorMessage={context.errorMessage} />
        <AllErrorsButton />
      </ErrorFooter>
    </ErrorAlertDescription>
  </ErrorAlert>
);

// WASM panic error renderer
const WasmPanicErrorRenderer: CustomErrorRenderer = {
  test: (errorMessage: string) => errorMessage.includes('WASM panic:'),
  priority: 150,
  render: (context: ErrorContext) => {
    // Extract panic message
    const panicMessage = context.errorMessage.replace('WASM panic: ', '');

    return (
      <ErrorAlert variant="destructive">
        <ErrorAlertTitle>
          <AlertCircle className="h-4 w-4" />
          Internal Runtime Error
        </ErrorAlertTitle>
        <ErrorAlertDescription className="space-y-3">
          <p className="text-sm text-[var(--vscode-charts-red)]" style={{ color: '#dc2626' }}>
            The BAML runtime encountered an unexpected error and needs to be restarted.
          </p>

          <div
            className="p-3 bg-[var(--vscode-editor-background)] border border-[var(--vscode-panel-border)] rounded-md"
            style={{
              background: `linear-gradient(rgba(220, 38, 38, 0.06), rgba(220, 38, 38, 0.06)), var(--vscode-editor-background)`,
              borderColor: 'rgba(220, 38, 38, 0.3)',
            }}
          >
            <pre className="text-xs whitespace-pre-wrap break-words font-mono text-[var(--vscode-charts-red)]" style={{ color: '#dc2626' }}>
              {panicMessage}
            </pre>
          </div>

          <p className="text-xs text-[var(--vscode-charts-red)]" style={{ color: '#dc2626' }}>
            Please reopen the playground to continue. Reach out to us at boundaryml.com/discord.
          </p>

          <ErrorFooter>
            <CopyErrorButton errorMessage={context.errorMessage} />
            <AllErrorsButton />
          </ErrorFooter>
        </ErrorAlertDescription>
      </ErrorAlert>
    );
  }
};

// Media fetch error renderer with enhanced styling
const MediaFetchErrorRenderer: CustomErrorRenderer = {
  test: (errorMessage: string) => errorMessage.startsWith('Failed to fetch media'),
  priority: 100,
  render: (context: ErrorContext) => {
    // Extract additional info from the error message if available
    const lines = context.errorMessage.split('\n');
    const mainError = lines[0] || context.errorMessage;

    // Try to extract URL and status from the error message
    const urlMatch = mainError.match(/https?:\/\/[^\s,]+/);
    const statusMatch = mainError.match(/(\d{3})\s/);

    const url = urlMatch ? urlMatch[0] : null;
    const statusCode = statusMatch ? statusMatch[1] : null;

    return (
      <ErrorAlert variant="destructive">
        <ErrorAlertTitle>
          <WifiOff className="h-4 w-4" />
          Media Fetch Failed
          {statusCode && (
            <ErrorBadge variant="outline" className="text-xs">
              HTTP {statusCode}
            </ErrorBadge>
          )}
        </ErrorAlertTitle>
        <ErrorAlertDescription className="space-y-3">
          <p className="text-sm text-[var(--vscode-charts-red)]" style={{ color: '#dc2626' }}>
            Unable to fetch media content. This could be due to:
          </p>

          <ul className="text-sm space-y-1 ml-4 list-disc text-[var(--vscode-charts-red)]" style={{ color: '#dc2626' }}>
            <li>Network connectivity issues</li>
            <li>Invalid or expired URL</li>
            <li>Server-side restrictions or rate limiting</li>
            <li>CORS policy blocking the request</li>
          </ul>

          <ErrorFooter>
            {context.onRetry && (
              <Button
                variant="outline"
                size="xs"
                onClick={context.onRetry}
                className="h-7 px-2 text-xs border-[var(--vscode-panel-border)] text-[var(--vscode-charts-red)] hover:bg-[var(--vscode-editor-background)] cursor-pointer hover:opacity-80"
                style={{
                  color: '#dc2626',
                  borderColor: 'rgba(220, 38, 38, 0.3)',
                  background: 'var(--vscode-editor-background)',
                }}
              >
                <RefreshCw className="w-3 h-3 mr-1" />
                Retry Test
              </Button>
            )}
            <CopyErrorButton errorMessage={context.errorMessage} />
            {url && (
              <Button
                asChild
                variant="outline"
                size="xs"
                className="h-7 px-2 text-xs gap-1 border-[var(--vscode-panel-border)] text-[var(--vscode-charts-red)] hover:bg-[var(--vscode-editor-background)] cursor-pointer hover:opacity-80"
                style={{
                  color: '#dc2626',
                  borderColor: 'rgba(220, 38, 38, 0.3)',
                  background: 'var(--vscode-editor-background)',
                }}
              >
                <a
                  href={url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="flex gap-1 items-center cursor-pointer"
                  title={url}
                >
                  <ExternalLink className="w-3 h-3" />
                  <span>Open Link</span>
                </a>
              </Button>
            )}
            <AllErrorsButton />
            {lines.length > 1 && (
              <ErrorDetails errorMessage={context.errorMessage} />
            )}
          </ErrorFooter>
        </ErrorAlertDescription>
      </ErrorAlert>
    );
  }
};

// Initialize default error renderers (only once)
let defaultRenderersRegistered = false;
const initializeDefaultRenderers = () => {
  if (!defaultRenderersRegistered) {
    registerErrorRenderer(WasmPanicErrorRenderer);
    registerErrorRenderer(MediaFetchErrorRenderer);
    defaultRenderersRegistered = true;
  }
};

// Main Enhanced Error Renderer component
export interface EnhancedErrorRendererProps {
  errorMessage: string;
  functionName?: string;
  testName?: string;
  onRetry?: () => void;
  className?: string;
}

// Memoized component to prevent unnecessary re-renders
export const EnhancedErrorRenderer: React.FC<EnhancedErrorRendererProps> = React.memo(({
  errorMessage,
  functionName,
  testName,
  onRetry,
  className
}) => {
  // Initialize default renderers
  initializeDefaultRenderers();

  // Create a stable context object that doesn't depend on function references
  const context: ErrorContext = useMemo(() => ({
    errorMessage,
    functionName,
    testName,
    onRetry: onRetry ? () => onRetry() : undefined
  }), [errorMessage, functionName, testName, Boolean(onRetry)]);

  // Find the appropriate error renderer
  const renderer = useMemo(() =>
    errorRenderers.find(r => r.test(errorMessage)),
    [errorMessage]
  );

  const renderedContent = useMemo(() => {
    if (renderer) {
      return renderer.render(context);
    }
    return <DefaultErrorRenderer context={context} />;
  }, [renderer, context]);

  return (
    <div className={className}>
      {renderedContent}
    </div>
  );
}, (prevProps, nextProps) => {
  // Custom comparison function for React.memo
  // Only compare primitive values to avoid function reference issues
  return (
    prevProps.errorMessage === nextProps.errorMessage &&
    prevProps.functionName === nextProps.functionName &&
    prevProps.testName === nextProps.testName &&
    prevProps.className === nextProps.className &&
    // Compare onRetry presence, not the function itself
    Boolean(prevProps.onRetry) === Boolean(nextProps.onRetry)
  );
});

EnhancedErrorRenderer.displayName = 'EnhancedErrorRenderer'; 