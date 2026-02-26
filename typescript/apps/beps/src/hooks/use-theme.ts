"use client";

import { useEffect, useState } from "react";

/**
 * Returns true when the app is in dark mode (html has class "dark").
 * Subscribes to class changes on document.documentElement so it updates
 * when the user toggles the theme.
 */
export function useIsDark(): boolean {
  const [isDark, setIsDark] = useState(false);

  useEffect(() => {
    const root = document.documentElement;

    const check = () => setIsDark(root.classList.contains("dark"));

    check();

    const observer = new MutationObserver(check);
    observer.observe(root, {
      attributes: true,
      attributeFilter: ["class"],
    });

    return () => observer.disconnect();
  }, []);

  return isDark;
}
