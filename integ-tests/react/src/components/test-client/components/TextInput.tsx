import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import type React from 'react';

interface TextInputProps {
  id: string;
  type: 'text' | 'textarea';
  value: string;
  onChange: (key: string, value: string) => void;
  placeholder?: string;
  disabled?: boolean;
}

export function TextInput({
  id,
  type,
  value,
  onChange,
  placeholder,
  disabled = false,
}: TextInputProps) {
  if (type === 'textarea') {
    return (
      <Textarea
        id={id}
        value={value}
        onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) =>
          onChange(id, e.target.value)
        }
        placeholder={placeholder}
        disabled={disabled}
      />
    );
  }

  return (
    <Input
      id={id}
      type="text"
      value={value}
      onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
        onChange(id, e.target.value)
      }
      placeholder={placeholder}
      disabled={disabled}
    />
  );
}
