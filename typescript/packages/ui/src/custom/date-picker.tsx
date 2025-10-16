'use client';
import { format } from 'date-fns';
import { Calendar as CalendarIcon } from 'lucide-react';

import { cn } from '@baml/ui/lib/utils';

import { Button } from '../components/button';
import { Calendar } from '../components/calendar';
import { Popover, PopoverContent, PopoverTrigger } from '../components/popover';

export const DatePicker = function DatePickerCmp({
  date,
  setDate,
}: {
  date?: Date;
  setDate: (date?: Date) => void;
}) {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant={'outline'}
          className={cn(
            'w-full justify-start text-left font-normal',
            !date && 'text-muted-foreground',
          )}
        >
          <CalendarIcon className="mr-2 h-4 w-4" />
          {date ? format(date, 'PPP') : <span>Pick a date</span>}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-auto p-0">
        <Calendar
          mode="single"
          selected={date}
          onSelect={setDate}
          // initialFocus
        />
      </PopoverContent>
    </Popover>
  );
};
