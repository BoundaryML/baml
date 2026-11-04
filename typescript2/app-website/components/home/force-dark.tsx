'use client';

import { useEffect } from 'react';

/** This version is dark only, so the board demo and the page edges follow it whatever the saved theme is. */
export default function ForceDark() {
  useEffect(() => {
    const root = document.documentElement;
    const previous = root.dataset.theme;
    root.dataset.theme = 'dark';
    return () => {
      if (previous) root.dataset.theme = previous;
    };
  }, []);
  return null;
}
