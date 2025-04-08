import { Label } from '@/components/ui/label';
import type { InputFieldConfig } from '@/lib/store';
import type { ImageSourceType } from '../utils/imageUtils';
import { ImageInput } from './ImageInput';
import { TextInput } from './TextInput';

interface InputFieldProps {
  input: InputFieldConfig;
  formValues: Record<string, string>;
  fileInputs: Record<string, File | null>;
  imageSourceTypes: Record<string, ImageSourceType>;
  imageUrlPreviews: Record<string, string>;
  onTextChange: (key: string, value: string) => void;
  onFileChange: (key: string, file: File | null) => void;
  onImageUrlChange: (key: string, url: string) => void;
  onToggleImageSourceType: (key: string) => void;
  disabled?: boolean;
}

export function InputField({
  input,
  formValues,
  fileInputs,
  imageSourceTypes,
  imageUrlPreviews,
  onTextChange,
  onFileChange,
  onImageUrlChange,
  onToggleImageSourceType,
  disabled = false,
}: InputFieldProps) {
  const renderInputComponent = () => {
    switch (input.type) {
      case 'text':
      case 'textarea':
        return (
          <TextInput
            id={input.key}
            type={input.type}
            value={formValues[input.key] || ''}
            onChange={onTextChange}
            placeholder={input.placeholder}
            disabled={disabled}
          />
        );
      case 'image': {
        const sourceType = imageSourceTypes[input.key] || 'file';
        return (
          <ImageInput
            id={input.key}
            accept={input.accept}
            sourceType={sourceType}
            fileValue={fileInputs[input.key] || null}
            urlValue={formValues[input.key] || ''}
            urlPreview={imageUrlPreviews[input.key]}
            onFileChange={onFileChange}
            onUrlChange={onImageUrlChange}
            onToggleSourceType={onToggleImageSourceType}
            disabled={disabled}
          />
        );
      }
      default:
        return null;
    }
  };

  return (
    <div className="space-y-2">
      <Label htmlFor={input.key}>{input.label}</Label>
      {renderInputComponent()}
    </div>
  );
}
