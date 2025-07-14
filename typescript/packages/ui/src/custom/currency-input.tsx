import type React from 'react';
import { useCallback, useEffect, useState } from 'react';

import { Button } from '../components/button';
import { Input } from '../components/input';
import { formatCurrency } from '../lib/format-currency';

interface CurrencyInputProps
  extends Omit<React.ComponentProps<'input'>, 'onChange'> {
  onChange?: (value: number) => void;
  currency?: string;
  locale?: string;
  quickFillValues?: number[];
}

export const CurrencyInput: React.FC<CurrencyInputProps> = ({
  onChange,
  currency = 'USD',
  locale = 'en-US',
  quickFillValues,
  ...props
}) => {
  const [value, setValue] = useState<string>('');

  const formatWithCommas = useCallback((value_: string) => {
    const parts = value_.split('.');
    if (parts[0]) {
      parts[0] = parts[0].replaceAll(/\B(?=(\d{3})+(?!\d))/g, ',');
    }
    return parts.join('.');
  }, []);

  const handleChange = useCallback(
    (newValue: string | number) => {
      const rawValue = newValue.toString().replaceAll(/[^\d.]/g, '');
      const formattedValue = formatWithCommas(rawValue);
      setValue(formattedValue);
      if (onChange) {
        onChange(Number.parseFloat(rawValue) || 0);
      }
    },
    [formatWithCommas, onChange],
  );

  useEffect(() => {
    if (props.value !== undefined) {
      handleChange(props.value.toString());
    }
  }, [props.value, handleChange]);

  const handleInputChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    handleChange(event.target.value);
  };

  const handleQuickFill = (fillValue: number) => {
    const formattedValue = formatWithCommas(fillValue.toString());
    setValue(formattedValue);
    if (onChange) {
      onChange(fillValue);
    }
  };

  return (
    <div className="w-full">
      <div className="relative">
        <Input
          {...props}
          type="text"
          value={value}
          onChange={handleInputChange}
          onFocus={(event) => event.target.select()}
          className="pl-8" // Add left padding for the currency icon
        />
        <span className="-translate-y-1/2 absolute top-1/2 left-3 text-gray-500">
          {currency === 'USD' ? '$' : currency}
        </span>
      </div>
      {quickFillValues && quickFillValues.length > 0 && (
        <div className="mt-2 grid grid-cols-4 gap-2">
          {quickFillValues.map((value) => (
            <Button
              key={`quick-fill-${value.toString()}`}
              size="sm"
              variant="outline"
              onClick={() => handleQuickFill(value)}
              type="button"
            >
              {formatCurrency({
                amount: value,
                compact: true,
                currency,
                locale,
              })}
            </Button>
          ))}
        </div>
      )}
    </div>
  );
};
