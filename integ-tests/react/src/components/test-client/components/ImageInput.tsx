import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Link, Upload } from 'lucide-react';
import type React from 'react';
import type { ImageSourceType } from '../utils/imageUtils';

interface ImageInputProps {
  id: string;
  accept?: string;
  sourceType: ImageSourceType;
  fileValue: File | null;
  urlValue: string;
  urlPreview: string | undefined;
  onFileChange: (key: string, file: File | null) => void;
  onUrlChange: (key: string, url: string) => void;
  onToggleSourceType: (key: string) => void;
  disabled?: boolean;
}

export function ImageInput({
  id,
  accept = 'image/*',
  sourceType,
  fileValue,
  urlValue,
  urlPreview,
  onFileChange,
  onUrlChange,
  onToggleSourceType,
  disabled = false,
}: ImageInputProps) {
  return (
    <div className="flex flex-col space-y-4">
      {/* Toggle between file upload and URL input */}
      <div className="flex items-center space-x-2">
        <Button
          type="button"
          variant={sourceType === 'file' ? 'default' : 'outline'}
          size="sm"
          onClick={() => {
            if (sourceType !== 'file') onToggleSourceType(id);
          }}
          disabled={disabled}
        >
          <Upload className="mr-2 h-4 w-4" />
          File Upload
        </Button>
        <Button
          type="button"
          variant={sourceType === 'url' ? 'default' : 'outline'}
          size="sm"
          onClick={() => {
            if (sourceType !== 'url') onToggleSourceType(id);
          }}
          disabled={disabled}
        >
          <Link className="mr-2 h-4 w-4" />
          URL
        </Button>
      </div>

      {/* File upload input */}
      {sourceType === 'file' && (
        <div className="flex flex-col space-y-2">
          <div className="flex items-center gap-2">
            <Input
              id={id}
              type="file"
              accept={accept}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                const file = e.target.files?.[0] || null;
                onFileChange(id, file);
              }}
              disabled={disabled}
              className="hidden"
            />
            <Button
              type="button"
              variant="outline"
              onClick={() => document.getElementById(id)?.click()}
              disabled={disabled}
            >
              <Upload className="mr-2 h-4 w-4" />
              {fileValue ? 'Change Image' : 'Upload Image'}
            </Button>
            {fileValue && (
              <span className="text-gray-500 text-sm">{fileValue.name}</span>
            )}
          </div>
          {fileValue && (
            <div className="mt-2 max-w-sm">
              <img
                src={URL.createObjectURL(fileValue)}
                alt="Preview"
                className="max-h-40 rounded-md object-contain"
              />
            </div>
          )}
        </div>
      )}

      {/* URL input */}
      {sourceType === 'url' && (
        <div className="flex flex-col space-y-2">
          <Input
            type="url"
            placeholder="Enter image URL..."
            value={urlValue}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
              onUrlChange(id, e.target.value)
            }
            disabled={disabled}
          />
          {urlPreview && (
            <div className="mt-2 max-w-sm">
              <img
                src={urlPreview}
                alt="Preview"
                className="max-h-40 rounded-md object-contain"
                onError={(e) => {
                  // Hide the image if it fails to load
                  (e.target as HTMLImageElement).style.display = 'none';
                }}
              />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
