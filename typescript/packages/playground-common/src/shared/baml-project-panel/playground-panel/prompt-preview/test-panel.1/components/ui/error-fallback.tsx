import { Button } from '@baml/ui/button';
import { memo } from 'react';
import { ErrorBoundary } from 'react-error-boundary';

export const ErrorFallback = memo(({
  error,
  resetErrorBoundary,
}: { error: Error; resetErrorBoundary: () => void }) => (
  <div className="p-4 text-center">
    <div className="text-destructive text-sm mb-2">Something went wrong</div>
    <div className="text-destructive text-sm mb-2">{error.message}</div>
    <Button onClick={resetErrorBoundary} variant="outline">
      Try again
    </Button>
  </div>
));

ErrorFallback.displayName = 'ErrorFallback';

export const SafeErrorBoundary = memo(({
  children,
  resetKeys = []
}: {
  children: React.ReactNode;
  resetKeys?: unknown[];
}) => (
  <ErrorBoundary
    FallbackComponent={ErrorFallback}
    resetKeys={resetKeys}
  >
    {children}
  </ErrorBoundary>
));

SafeErrorBoundary.displayName = 'SafeErrorBoundary';