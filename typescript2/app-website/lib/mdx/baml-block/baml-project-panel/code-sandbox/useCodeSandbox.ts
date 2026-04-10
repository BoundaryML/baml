import { type OutputLine, runCode } from '@/app/actions/baml-sandbox';
import { readStreamableValue } from 'ai/rsc';
import { useState } from 'react';
import { type ICodeBlock } from '../types';

export function useCodeSandbox() {
  const [isLoading, setIsLoading] = useState(false);
  const [output, setOutput] = useState<OutputLine[]>([]);

  const execute = async (
    code: ICodeBlock,
    files: { path: string; content: string }[],
    callbacks?: {
      onOutput?: (line: OutputLine) => void;
    },
  ) => {
    setIsLoading(true);
    setOutput([]);

    try {
      const { object } = await runCode(code, files);
      const asyncIterable = readStreamableValue(object);

      for await (const line of asyncIterable) {
        if (!line) continue;

        setOutput((prev) => [...prev, line]);
        callbacks?.onOutput?.(line);
      }
    } catch (err) {
      console.error('Code execution error:', err);
    } finally {
      setIsLoading(false);
    }
  };

  return {
    output,
    isLoading,
    execute,
  };
}
