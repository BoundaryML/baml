import { useAtomValue, useSetAtom } from 'jotai';
import type React from 'react';
import { displaySettingsAtom } from '../preview-toolbar';
import { showTokensAtom } from './render-text';

export const PromptStats: React.FC<{ text: string }> = ({ text }) => {
  const showTokenCounts = useAtomValue(showTokensAtom);
  const setDisplaySettings = useSetAtom(displaySettingsAtom);
  const numberFormatter = new Intl.NumberFormat();

  return (
    <div className="flex flex-row sm:gap-4 justify-between items-stretch px-2 py-2 text-xs border border-border bg-muted text-muted-foreground rounded-b w-full">
      <div className="flex flex-wrap gap-y-2 gap-x-5 sm:gap-x-4 w-full sm:w-auto">
        <div className="flex flex-col items-start min-w-[60px]">
          <span className="text-muted-foreground/60">Characters</span>
          <span className="font-medium">
            {numberFormatter.format(text.length)}
          </span>
        </div>
        <div className="flex flex-col items-start min-w-[60px]">
          <span className="text-muted-foreground/60">Words</span>
          <span className="font-medium">
            {numberFormatter.format(text.split(/\s+/).filter(Boolean).length)}
          </span>
        </div>
        <div className="flex flex-col items-start min-w-[60px]">
          <span className="text-muted-foreground/60">Lines</span>
          <span className="font-medium">
            {numberFormatter.format(text.split('\n').length)}
          </span>
        </div>
        <div className="flex flex-col items-start min-w-[80px]">
          <span className="text-muted-foreground/60">Tokens (est.)</span>
          <span className="font-medium">
            {numberFormatter.format(Math.ceil(text.length / 4))}
          </span>
        </div>
      </div>
      <button
        className="mt-2 sm:mt-0 sm:ml-4 px-2 py-1 rounded bg-card border border-border text-xs hover:bg-accent transition-colors self-end sm:self-center shrink-0 whitespace-nowrap"
        type="button"
        onClick={() =>
          setDisplaySettings((prev) => ({
            ...prev,
            showTokens: !prev.showTokens,
          }))
        }
      >
        {showTokenCounts ? 'Hide Tokens' : 'Show Tokens'}
      </button>
    </div>
  );
};
