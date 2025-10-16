'use client';

import { CalendarIcon } from '@radix-ui/react-icons';
import { addDays, format } from 'date-fns';
import { useSearchParams } from 'next/navigation';
import * as React from 'react';
import type { DateRange } from 'react-day-picker';

import { cn } from '@baml/ui/lib/utils';

import type { VariantProps } from 'class-variance-authority';
import { Button, type buttonVariants } from '../components/button';
import { Calendar } from '../components/calendar';
import { Popover, PopoverContent, PopoverTrigger } from '../components/popover';

interface DateRangePickerProps
  extends React.ComponentPropsWithoutRef<typeof PopoverContent> {
  /**
   * The selected date range.
   * @default undefined
   * @type DateRange
   * @example { from: new Date(), to: new Date() }
   */
  dateRange?: DateRange;

  /**
   * The number of days to display in the date range picker.
   * @default undefined
   * @type number
   * @example 7
   */
  dayCount?: number;

  /**
   * The placeholder text of the calendar trigger button.
   * @default "Pick a date"
   * @type string | undefined
   */
  placeholder?: string;

  /**
   * The variant of the calendar trigger button.
   * @default "outline"
   * @type "default" | "outline" | "secondary" | "ghost"
   */
  triggerVariant?: VariantProps<typeof buttonVariants>['variant'];

  /**
   * The size of the calendar trigger button.
   * @default "default"
   * @type "default" | "sm" | "lg"
   */
  triggerSize?: VariantProps<typeof buttonVariants>['size'];

  /**
   * The class name of the calendar trigger button.
   * @default undefined
   * @type string
   */
  triggerClassName?: string;
}

export function DateRangePicker({
  dateRange,
  dayCount,
  placeholder = 'Pick a date',
  triggerVariant = 'outline',
  triggerSize = 'default',
  triggerClassName,
  className,
  ...props
}: DateRangePickerProps) {
  const searchParams = useSearchParams();

  const [date, setDate] = React.useState<DateRange | undefined>(() => {
    const fromParam = searchParams.get('from');
    const toParam = searchParams.get('to');

    let fromDay: Date | undefined;
    let toDay: Date | undefined;

    if (dateRange) {
      fromDay = dateRange.from;
      toDay = dateRange.to;
    } else if (dayCount) {
      toDay = new Date();
      fromDay = addDays(toDay, -dayCount);
    }

    return {
      from: fromParam ? new Date(fromParam) : fromDay,
      to: toParam ? new Date(toParam) : toDay,
    };
  });

  return (
    <div className="grid gap-2">
      <Popover>
        <PopoverTrigger asChild>
          <Button
            variant={triggerVariant}
            size={triggerSize}
            className={cn(
              'w-full justify-start truncate text-left font-normal',
              !date && 'text-muted-foreground',
              triggerClassName,
            )}
          >
            <CalendarIcon className="mr-2 size-4" />
            {date?.from ? (
              date.to ? (
                <>
                  {format(date.from, 'LLL dd, y')} -{' '}
                  {format(date.to, 'LLL dd, y')}
                </>
              ) : (
                format(date.from, 'LLL dd, y')
              )
            ) : (
              <span>{placeholder}</span>
            )}
          </Button>
        </PopoverTrigger>
        <PopoverContent className={cn('w-auto p-0', className)} {...props}>
          <Calendar
            // initialFocus
            mode="range"
            defaultMonth={date?.from}
            selected={date}
            onSelect={setDate}
            numberOfMonths={2}
          />
        </PopoverContent>
      </Popover>
    </div>
  );
}
