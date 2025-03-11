import { Alert, AlertDescription } from '@/components/ui/alert';
import { useMarkdown } from '@/lib/use-markdown';
import { cn } from '@/lib/utils';
import yaml from 'js-yaml';
import { ErrorBoundary } from 'react-error-boundary';

// Type for renderer props
type FormatterProps = {
  content: unknown;
  className?: string;
};

// Error fallback component
function FormattingErrorFallback({
  error,
  resetErrorBoundary,
}: { error: Error; resetErrorBoundary: () => void }) {
  return (
    <Alert variant="destructive" className="mb-2">
      <AlertDescription>
        <div className="flex flex-col gap-2">
          <p className="font-medium">Cannot render in this format</p>
          <p className="text-sm">
            The content cannot be displayed in the selected format.
          </p>
          <button
            type="button"
            onClick={resetErrorBoundary}
            className="text-xs underline self-start cursor-pointer hover:no-underline"
          >
            Try again
          </button>
        </div>
      </AlertDescription>
    </Alert>
  );
}

// Raw content renderer (no special formatting)
export function RawRenderer({ content, className }: FormatterProps) {
  if (content === null || content === undefined) {
    return <p>No content available</p>;
  }

  const displayContent =
    typeof content === 'string' ? content : JSON.stringify(content, null, 2);

  return <pre className={className}>{displayContent}</pre>;
}

// JSON renderer with error boundary
export function JsonRenderer({ content, className }: FormatterProps) {
  const formatJson = (data: unknown): string => {
    if (data === null || data === undefined) {
      return '';
    }

    // If already a string, try to parse it as JSON first
    if (typeof data === 'string') {
      try {
        const parsedJson = JSON.parse(data);
        return JSON.stringify(parsedJson, null, 2);
      } catch {
        // If it can't be parsed as JSON, throw an error
        throw new Error('Content is not valid JSON');
      }
    }

    // Otherwise, stringify the object
    return JSON.stringify(data, null, 2);
  };

  return (
    <ErrorBoundary
      FallbackComponent={FormattingErrorFallback}
      onReset={() => {
        /* Reset state if needed */
      }}
    >
      <pre className={className}>{formatJson(content)}</pre>
    </ErrorBoundary>
  );
}

// YAML renderer with error boundary
export function YamlRenderer({ content, className }: FormatterProps) {
  const formatYaml = (data: unknown): string => {
    if (data === null || data === undefined) {
      return '';
    }

    try {
      // If it's a string, try to parse as JSON first
      // (useful for API responses that are JSON strings)
      if (typeof data === 'string') {
        try {
          const parsed = JSON.parse(data);
          return yaml.dump(parsed);
        } catch {
          // If parsing as JSON fails, try to dump it as YAML directly
          return yaml.dump(data);
        }
      }

      // Otherwise dump the object as YAML
      return yaml.dump(data);
    } catch (error) {
      throw new Error('Content cannot be formatted as YAML');
    }
  };

  return (
    <ErrorBoundary
      FallbackComponent={FormattingErrorFallback}
      onReset={() => {
        /* Reset state if needed */
      }}
    >
      <pre className={className}>{formatYaml(content)}</pre>
    </ErrorBoundary>
  );
}

// Markdown renderer with error boundary
export function MarkdownRenderer({ content, className }: FormatterProps) {
  const markdownContent =
    typeof content === 'string' ? content : JSON.stringify(content, null, 2);

  const markdownRef = useMarkdown(markdownContent);

  return (
    <ErrorBoundary
      FallbackComponent={FormattingErrorFallback}
      onReset={() => {
        /* Reset state if needed */
      }}
    >
      <div ref={markdownRef} className={cn('markdown-content', className)} />
    </ErrorBoundary>
  );
}
