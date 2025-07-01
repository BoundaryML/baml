import type { InputFieldConfig } from '~/lib/store';
import { useEffect, useMemo, useState } from 'react';
import { type ImageSourceType, isValidImageUrl } from '../utils/imageUtils';

interface FormState {
  formValues: Record<string, string>;
  fileInputs: Record<string, File | null>;
  imageSourceTypes: Record<string, ImageSourceType>;
  imageUrlPreviews: Record<string, string>;
  isFormValid: boolean;
  handleTextInputChange: (key: string, value: string) => void;
  handleFileInputChange: (key: string, file: File | null) => void;
  handleImageUrlChange: (key: string, url: string) => void;
  toggleImageSourceType: (key: string) => void;
  resetForm: () => void;
}

export function useFormState(inputConfigs: InputFieldConfig[]): FormState {
  // State to hold form values for all inputs
  const [formValues, setFormValues] = useState<Record<string, string>>({});

  // State for file inputs
  const [fileInputs, setFileInputs] = useState<Record<string, File | null>>({});

  // State to track image source type (file or URL) for each image input
  const [imageSourceTypes, setImageSourceTypes] = useState<
    Record<string, ImageSourceType>
  >({});

  // State for tracking image URL previews for URL inputs
  const [imageUrlPreviews, setImageUrlPreviews] = useState<
    Record<string, string>
  >({});

  // Initialize default source types for image inputs when component mounts
  useEffect(() => {
    const defaultImageSourceTypes: Record<string, ImageSourceType> = {};
    for (const input of inputConfigs) {
      if (input.type === 'image') {
        defaultImageSourceTypes[input.key] = 'file';
      }
    }
    setImageSourceTypes(defaultImageSourceTypes);
  }, [inputConfigs]);

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

  const handleImageUrlChange = (key: string, url: string) => {
    // Update the form value with the URL
    handleTextInputChange(key, url);

    // Update the preview URL if it's a valid URL
    if (isValidImageUrl(url)) {
      setImageUrlPreviews((prev) => ({
        ...prev,
        [key]: url,
      }));
    } else {
      // Clear the preview if the URL is invalid
      setImageUrlPreviews((prev) => {
        const newPreviews = { ...prev };
        delete newPreviews[key];
        return newPreviews;
      });
    }
  };

  const toggleImageSourceType = (key: string) => {
    setImageSourceTypes((prev) => {
      const currentType = prev[key] || 'file';
      const newType: ImageSourceType = currentType === 'file' ? 'url' : 'file';

      // Clear the appropriate input when switching types
      if (newType === 'file') {
        // Clear URL input when switching to file
        handleTextInputChange(key, '');
        setImageUrlPreviews((prev) => {
          const newPreviews = { ...prev };
          delete newPreviews[key];
          return newPreviews;
        });
      } else {
        // Clear file input when switching to URL
        handleFileInputChange(key, null);
      }

      return {
        ...prev,
        [key]: newType,
      };
    });
  };

  // Check if form is valid (all required inputs have values)
  const isFormValid = useMemo(() => {
    return inputConfigs.every((input) => {
      if (!input.required) return true;

      if (input.type === 'image') {
        const sourceType = imageSourceTypes[input.key] || 'file';
        if (sourceType === 'file') {
          return !!fileInputs[input.key];
        }

        // For URL image sources, check if the URL is valid
        const url = formValues[input.key];
        return isValidImageUrl(url ?? '');
      }

      return !!formValues[input.key]?.trim();
    });
  }, [inputConfigs, formValues, fileInputs, imageSourceTypes]);

  const resetForm = () => {
    setFormValues({});
    setFileInputs({});
    setImageUrlPreviews({});

    // Reset image source types to defaults
    const defaultImageSourceTypes: Record<string, ImageSourceType> = {};
    for (const input of inputConfigs) {
      if (input.type === 'image') {
        defaultImageSourceTypes[input.key] = 'file';
      }
    }
    setImageSourceTypes(defaultImageSourceTypes);
  };

  return {
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
  };
}
