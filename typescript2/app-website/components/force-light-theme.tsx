'use client';

import { useTheme } from 'next-themes';
import { useEffect } from 'react';

export function ForceLightTheme() {
  const { setTheme } = useTheme();
  useEffect(() => {
    setTheme('light');
  }, [setTheme]);
  return null;
}
