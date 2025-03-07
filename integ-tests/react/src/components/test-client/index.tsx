'use client';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import {
  type InputFieldConfig,
  exampleParser,
  getHookConfig,
  getInputConfigs,
  useResponseCardConfigWithQueryParams,
} from '@/lib/store';
import { Loader2, Upload } from 'lucide-react';
import { useQueryState } from 'nuqs';
import * as React from 'react';
import type { FunctionNames } from '../../../baml_client/react/hooks';
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
    onStreamData: (response: any) => {},
  });

  const {
    isLoading,
    error,
    isError,
    isSuccess,
    mutate,
    status,
    data,
    streamData,
  } = hookResult;

  // State to hold form values for all inputs
  const [formValues, setFormValues] = React.useState<Record<string, any>>({});
  const [hasStarted, setHasStarted] = React.useState(false);

  // State for file inputs
  const [fileInputs, setFileInputs] = React.useState<
    Record<string, File | null>
  >({});

  // Reset form state when selected example changes
  // Use a ref to track the previous example to avoid unnecessary resets
  const prevExampleRef = React.useRef<string | null>(null);

  React.useEffect(() => {
    // Only reset when example actually changes
    if (prevExampleRef.current !== selectedExample) {
      setFormValues({});
      setFileInputs({});
      setHasStarted(false);

      if (hookResult.reset) {
        hookResult.reset();
      }

      // Update ref to current example
      prevExampleRef.current = selectedExample;
    }
  }, [selectedExample]); // Only depend on selectedExample, not hookResult

  const handleTextInputChange = (key: string, value: string) => {
    setFormValues((prev) => ({
      ...prev,
      [key]: value,
    }));
  };

  const handleFileInputChange = (key: string, file: File | null) => {
    setFileInputs((prev) => ({
      ...prev,
      [key]: file,
    }));
  };

  // Check if form is valid (all required inputs have values)
  const isFormValid = React.useMemo(() => {
    return inputConfigs.every((input) => {
      if (!input.required) return true;
      if (input.type === 'image') {
        return !!fileInputs[input.key];
      }
      return !!formValues[input.key]?.trim();
    });
  }, [inputConfigs, formValues, fileInputs]);

  const handleSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!isFormValid) return;

    setHasStarted(true);

    // Prepare payload based on input configurations
    const payload: any = {};

    // Add text inputs
    for (const key of Object.keys(formValues)) {
      payload[key] = formValues[key];
    }

    // Add file inputs - convert to appropriate format if needed
    // This might need to be adjusted based on how your hooks expect file data
    for (const [key, file] of Object.entries(fileInputs)) {
      if (file) {
        // For image files, you might need to convert to base64 or another format
        // This depends on what the API expects
        try {
          const base64 = await convertFileToBase64(file);
          payload[key] = base64;
        } catch (error) {
          console.error('Error converting file:', error);
        }
      }
    }

    // Use the first input as the main payload if there's only one
    if (inputConfigs.length === 1 && inputConfigs[0].type !== 'image') {
      await mutate(formValues[inputConfigs[0].key]);
    } else {
      // Otherwise pass the full payload object
      await mutate(payload);
    }
  };

  // Helper function to convert files to base64
  const convertFileToBase64 = (file: File): Promise<string> => {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.readAsDataURL(file);
      reader.onload = () => resolve(reader.result as string);
      reader.onerror = (error) => reject(error);
    });
  };

  // Reset hasStarted when the request is complete or reset
  React.useEffect(() => {
    if (!isLoading && !streamData && !data && !error) {
      setHasStarted(false);
    }
  }, [isLoading, streamData, data, error]);

  // Render input component based on type
  const renderInputComponent = (input: InputFieldConfig) => {
    // Remove debug logging
    switch (input.type) {
      case 'text':
        return (
          <Input
            id={input.key}
            type="text"
            value={formValues[input.key] || ''}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
              handleTextInputChange(input.key, e.target.value)
            }
            placeholder={input.placeholder}
            disabled={isLoading}
          />
        );
      case 'textarea':
        return (
          <Textarea
            id={input.key}
            value={formValues[input.key] || ''}
            onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) =>
              handleTextInputChange(input.key, e.target.value)
            }
            placeholder={input.placeholder}
            disabled={isLoading}
          />
        );
      case 'image':
        return (
          <div className="flex flex-col space-y-2">
            <div className="flex items-center gap-2">
              <Input
                id={input.key}
                type="file"
                accept={input.accept || 'image/*'}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                  const file = e.target.files?.[0] || null;
                  handleFileInputChange(input.key, file);
                }}
                disabled={isLoading}
                className="hidden"
              />
              <Button
                type="button"
                variant="outline"
                onClick={() => document.getElementById(input.key)?.click()}
                disabled={isLoading}
              >
                <Upload className="mr-2 h-4 w-4" />
                {fileInputs[input.key] ? 'Change Image' : 'Upload Image'}
              </Button>
              {fileInputs[input.key] && (
                <span className="text-gray-500 text-sm">
                  {fileInputs[input.key]?.name}
                </span>
              )}
            </div>
            {fileInputs[input.key] && (
              <div className="mt-2 max-w-sm">
                <img
                  src={URL.createObjectURL(fileInputs[input.key] as File)}
                  alt="Preview"
                  className="max-h-40 rounded-md object-contain"
                />
              </div>
            )}
          </div>
        );
      default:
        return null;
    }
  };

  return (
    <div className="flex w-full flex-col items-center gap-6">
      <div className="w-full max-w-xl">
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-4">
            {/* Render input fields dynamically based on configuration */}
            {inputConfigs.map((input, index) => (
              <div key={input.key} className="space-y-2">
                <Label htmlFor={input.key}>{input.label}</Label>
                {renderInputComponent(input)}
              </div>
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
                    hookResult.reset();
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
