'use client';

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@baml/ui/select';
import {
  exampleParser,
  getExampleDisplayName,
  hookConfigMap,
} from '~/lib/store';
import { useQueryState } from 'nuqs';
import { useCallback } from 'react';
import type { FunctionNames } from '../../baml_client/react/hooks';
import { TabConfigMenu } from './test-client/tab-config-menu';

export function ExampleSelector() {
  const [selectedExample, setSelectedExample] = useQueryState(
    'example',
    exampleParser,
  );

  const handleExampleChange = useCallback(
    (value: string) => {
      setSelectedExample(value as FunctionNames);
    },
    [setSelectedExample],
  );

  return (
    <div className="space-y-4 text-center">
      <h1 className="font-bold text-4xl tracking-tight">
        BAML + Next.js Integration
      </h1>
      <p className="text-lg text-muted-foreground">
        Select an example below to get started.
      </p>
      <div className="mx-auto flex w-[350px] items-center gap-2">
        <Select value={selectedExample} onValueChange={handleExampleChange}>
          <SelectTrigger>
            <SelectValue placeholder="Select an example">
              {selectedExample &&
                getExampleDisplayName(selectedExample as FunctionNames)}
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {Object.keys(hookConfigMap).map((exampleKey, index) => (
              <div key={exampleKey}>
                {index > 0 && <div className="mx-2 my-1 h-px bg-muted" />}
                <SelectItem value={exampleKey}>
                  <div className="flex flex-col gap-1">
                    <span className="font-medium">
                      {getExampleDisplayName(exampleKey as FunctionNames)}
                    </span>
                    <span className="text-muted-foreground text-xs">
                      {hookConfigMap[exampleKey as FunctionNames]?.description}
                    </span>
                  </div>
                </SelectItem>
              </div>
            ))}
          </SelectContent>
        </Select>
        <TabConfigMenu />
      </div>
    </div>
  );
}
