import type { InputFieldConfig } from '~/lib/store';
import { Image as BamlImage } from '@boundaryml/baml/browser';
import { useEffect, useState } from 'react';

// Define a more specific type for the hook result
interface HookResult {
  isLoading: boolean;
  streamData?: unknown;
  data?: unknown;
  error?: unknown;
  reset: () => void;
  mutate: unknown; // Using 'unknown' because the actual type is complex and varies
}

interface UseFormSubmissionProps {
  inputConfigs: InputFieldConfig[];
  formValues: Record<string, string>;
  fileInputs: Record<string, File | null>;
  imageSourceTypes: Record<string, string>;
  isFormValid: boolean;
  mutate: unknown; // Using 'unknown' because we'll handle the typing internally
  hookResult: HookResult;
}

interface UseFormSubmissionResult {
  hasStarted: boolean;
  setHasStarted: React.Dispatch<React.SetStateAction<boolean>>;
  handleSubmit: (e: React.FormEvent<HTMLFormElement>) => Promise<void>;
}

export function useFormSubmission({
  inputConfigs,
  formValues,
  fileInputs,
  imageSourceTypes,
  isFormValid,
  mutate,
  hookResult,
}: UseFormSubmissionProps): UseFormSubmissionResult {
  const [hasStarted, setHasStarted] = useState(false);

  // Reset hasStarted when the request is complete or reset
  const { isLoading, streamData, data, error } = hookResult;

  useEffect(() => {
    if (!isLoading && !streamData && !data && !error && hasStarted) {
      setHasStarted(false);
    }
  }, [isLoading, streamData, data, error, hasStarted]);

  const handleSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!isFormValid) return;

    setHasStarted(true);

    // Prepare payload based on input configurations
    const payload: Record<string, unknown> = {};

    // Add text inputs
    for (const key of Object.keys(formValues)) {
      payload[key] = formValues[key];
    }

    // Process image inputs based on their source type
    for (const input of inputConfigs) {
      if (input.type === 'image') {
        const key = input.key;
        const sourceType = imageSourceTypes[key] || 'file';

        if (sourceType === 'file' && fileInputs[key]) {
          // Handle file inputs - convert to appropriate format
          try {
            // For image files, convert to base64 and create an Image object
            payload[key] = await BamlImage.fromFile(fileInputs[key] as File);
          } catch (error) {
            console.error('Error converting file:', error);
          }
        } else if (sourceType === 'url' && formValues[key]) {
          // Handle URL inputs
          const url = formValues[key];
          // Create an Image object from URL
          payload[key] = BamlImage.fromUrl(url);
        }
      }
    }

    // Debug the payload before submitting
    console.log('Submitting payload:', payload);

    try {
      // Extract the parameters in the correct order based on inputConfigs
      // and pass them as individual arguments to mutate
      if (inputConfigs.length === 1 && inputConfigs[0]?.type !== 'image') {
        // Simple case with single non-image parameter
        if (typeof mutate === 'function') {
          await (mutate as (arg: unknown) => Promise<void>)(
            formValues[inputConfigs[0]?.key ?? ''],
          );
        }
      } else {
        // Extract ordered parameters based on inputConfigs
        const orderedParams = inputConfigs
          .map((input) => payload[input.key])
          .filter((param) => param !== undefined);

        // Use safer approach to call the function
        if (typeof mutate === 'function') {
          // Use Function.prototype.apply with proper type casting
          await (Function.prototype.apply.call(
            mutate,
            null,
            orderedParams,
          ) as Promise<void>);
        }
      }
    } catch (error) {
      console.error('Error submitting form:', error);
    }
  };

  return {
    hasStarted,
    setHasStarted,
    handleSubmit,
  };
}
