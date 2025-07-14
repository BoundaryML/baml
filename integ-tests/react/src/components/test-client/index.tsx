'use client';

import {
  exampleParser,
  getHookConfig,
  getInputConfigs,
  useResponseCardConfigWithQueryParams,
} from '~/lib/store';
import { Button } from '@baml/ui/button';
import { Loader2 } from 'lucide-react';
import { useQueryState } from 'nuqs';
import * as React from 'react';
import type { FunctionNames } from '../../../baml_client/react/hooks';
import { InputField } from './components/InputField';
import { useFormState } from './hooks/useFormState';
import { useFormSubmission } from './hooks/useFormSubmission';
import { ResponseCard } from './response-card';

export function TestClient() {
  // Use the query parameter for the selected example
  const [selectedExample] = useQueryState('example', exampleParser);

  // Get streaming configuration from the global config
  const { config } = useResponseCardConfigWithQueryParams();

  // Get the hook config for the selected example
  const hookConfig = getHookConfig(selectedExample as FunctionNames);

  // Get input configurations
  const inputConfigs = getInputConfigs(selectedExample as FunctionNames);

  // Create a memoized hook to avoid re-rendering issues
  const CurrentHook = React.useMemo(() => hookConfig?.hook, [selectedExample]);

  // Use the hook dynamically based on the selected example
  const hookResult = CurrentHook({
    stream: config.isStreamingEnabled as true,
    onStreamData: (response: unknown) => {},
  });

  // Initialize form state with our custom hook
  const {
    formValues,
    fileInputs,
    imageSourceTypes,
    imageUrlPreviews,
    isFormValid,
    handleTextInputChange,
    handleFileInputChange,
    handleImageUrlChange,
    toggleImageSourceType,
    resetForm,
  } = useFormState(inputConfigs);

  // Form submission logic
  const { hasStarted, setHasStarted, handleSubmit } = useFormSubmission({
    inputConfigs,
    formValues,
    fileInputs,
    imageSourceTypes,
    isFormValid,
    mutate: hookResult.mutate,
    hookResult,
  });

  // Reset form state when selected example changes
  const prevExampleRef = React.useRef<string | null>(null);

  React.useEffect(() => {
    if (prevExampleRef.current !== selectedExample) {
      resetForm();
      // Update ref to current example
      prevExampleRef.current = selectedExample;
    }
  }, [selectedExample, resetForm]);

  const { isLoading, isSuccess, isError, reset } = hookResult;

  return (
    <div className="flex w-full flex-col items-center gap-6">
      <div className="w-full max-w-xl">
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-4">
            {/* Render input fields dynamically based on configuration */}
            {inputConfigs.map((input) => (
              <InputField
                key={input.key}
                input={input}
                formValues={formValues}
                fileInputs={fileInputs}
                imageSourceTypes={imageSourceTypes}
                imageUrlPreviews={imageUrlPreviews}
                onTextChange={handleTextInputChange}
                onFileChange={handleFileInputChange}
                onImageUrlChange={handleImageUrlChange}
                onToggleImageSourceType={toggleImageSourceType}
                disabled={isLoading}
              />
            ))}

            <div className="flex items-center justify-between space-x-2 pt-2">
              {!isSuccess && !isError && (
                <Button
                  type="submit"
                  disabled={isLoading || !isFormValid}
                  className="flex-1"
                >
                  {isLoading && (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  )}
                  {isLoading ? 'Processing...' : 'Submit'}
                </Button>
              )}
              {(isSuccess || isError) && (
                <Button
                  variant="outline"
                  className="flex-1"
                  disabled={isLoading}
                  onClick={() => {
                    setHasStarted(false);
                    reset();
                  }}
                >
                  Reset
                </Button>
              )}
            </div>
          </div>
        </form>
      </div>

      {/* Response card at full width */}
      <div className="w-full">
        <ResponseCard hookResult={hookResult} hasStarted={hasStarted} />
      </div>
    </div>
  );
}
