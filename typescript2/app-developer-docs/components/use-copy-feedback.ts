'use client';

import { useEffect, useRef, useState } from 'react';

const COPY_FEEDBACK_DURATION_MS = 1600;

export function useCopyFeedback<Action extends string>() {
  const [copiedAction, setCopiedAction] = useState<Action | null>(null);
  const resetTimer = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (resetTimer.current !== null) {
        window.clearTimeout(resetTimer.current);
      }
    },
    [],
  );

  const copy = async (action: Action, value: string): Promise<boolean> => {
    try {
      await navigator.clipboard.writeText(value);
    } catch {
      return false;
    }

    setCopiedAction(action);
    if (resetTimer.current !== null) {
      window.clearTimeout(resetTimer.current);
    }
    resetTimer.current = window.setTimeout(
      () => setCopiedAction(null),
      COPY_FEEDBACK_DURATION_MS,
    );
    return true;
  };

  return { copiedAction, copy };
}
