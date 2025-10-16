'use client';
import { Button } from '@baml/ui/button';
import { RefreshCcw } from 'lucide-react';
import type React from 'react';
import { ErrorBoundary, type FallbackProps } from 'react-error-boundary';

const ErrorFallback: (message?: string) => React.FC<FallbackProps> = (
  message,
) => {
  const FB = ({ error, resetErrorBoundary }: FallbackProps) => {
    return (
      <div
        role="alert"
        className="p-4 rounded border bg-vscode-notifications-background border-vscode-notifications-border"
      >
        <div className="flex justify-between items-center mb-4">
          <p className="font-medium text-vscode-foreground">
            {message ?? 'Something went wrong'}
          </p>
          <Button
            onClick={resetErrorBoundary}
            variant="outline"
            className="hover:bg-vscode-button-hover-background"
          >
            <RefreshCcw className="w-4 h-4" />
            Reload
          </Button>
        </div>

        {error instanceof Error && (
          <div className="space-y-2">
            {error.message && (
              <pre className="p-3 text-sm whitespace-pre-wrap rounded border bg-vscode-editor-background border-vscode-panel-border">
                {error.message}
              </pre>
            )}
            {error.stack && (
              <pre className="p-3 text-sm whitespace-pre-wrap rounded border bg-vscode-editor-background border-vscode-panel-border">
                {error.stack}
              </pre>
            )}
            {error && Object.keys(error).length > 0 && (
              <pre className="p-3 text-sm whitespace-pre-wrap rounded border bg-vscode-editor-background border-vscode-panel-border">
                {JSON.stringify(error, null, 2)}
              </pre>
            )}
          </div>
        )}
        {error && typeof error === 'string' && (
          <pre className="p-3 text-sm whitespace-pre-wrap rounded border bg-vscode-editor-background border-vscode-panel-border">
            {error}
          </pre>
        )}
      </div>
    );
  };
  return FB;
};

interface MyErrorBoundaryProps {
  children: React.ReactNode;
  message?: string;
}

export const CustomErrorBoundary: React.FC<MyErrorBoundaryProps> = ({
  children,
  message,
}) => {
  return (
    <ErrorBoundary
      FallbackComponent={ErrorFallback(message)}
      onReset={() => {
        // Reset the state of your app so the error doesn't happen again
        window.location.reload();
      }}
    >
      {children}
    </ErrorBoundary>
  );
};

