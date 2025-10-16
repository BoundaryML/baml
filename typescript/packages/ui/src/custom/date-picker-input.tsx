import { add, format } from 'date-fns';
import type React from 'react';
import { useCallback } from 'react';

import { Button } from '../components/button';
import { DatePicker } from './date-picker';

interface DatePickerInputProps {
  date?: Date;
  onChange: (date?: Date) => void;
  quickFillDates?: (Date | string)[];
}

export const DatePickerInput: React.FC<DatePickerInputProps> = ({
  date,
  onChange,
  quickFillDates,
}) => {
  const parseRelativeDate = useCallback((relativeDate: string | Date): Date => {
    if (relativeDate instanceof Date) return relativeDate;

    const now = new Date();
    const [amount, unit, ago] = relativeDate.split(' ');
    const numericAmount = Number.parseInt(amount ?? '0', 10);
    const isPast = ago === 'ago';

    switch (unit) {
      case 'day':
      case 'days': {
        return add(now, { days: isPast ? -numericAmount : numericAmount });
      }
      case 'week':
      case 'weeks': {
        return add(now, { weeks: isPast ? -numericAmount : numericAmount });
      }
      case 'months':
      case 'month': {
        return add(now, { months: isPast ? -numericAmount : numericAmount });
      }
      case 'year':
      case 'years': {
        return add(now, { years: isPast ? -numericAmount : numericAmount });
      }
      default: {
        return now;
      }
    }
  }, []);

  const handleQuickFill = useCallback(
    (fillDate: Date | string) => {
      const parsedDate = parseRelativeDate(fillDate);
      onChange(parsedDate);
    },
    [onChange, parseRelativeDate],
  );

  return (
    <div className="w-full">
      <DatePicker date={date} setDate={onChange} />
      {quickFillDates && quickFillDates.length > 0 && (
        <div className="mt-2 grid grid-cols-4 gap-2">
          {quickFillDates.map((fillDate) => (
            <Button
              key={`quick-fill-${fillDate.toString()}`}
              size="sm"
              variant="outline"
              onClick={() => handleQuickFill(fillDate)}
              type="button"
            >
              {typeof fillDate === 'string'
                ? fillDate
                : format(fillDate, 'MMM d, yyyy')}
            </Button>
          ))}
        </div>
      )}
    </div>
  );
};
