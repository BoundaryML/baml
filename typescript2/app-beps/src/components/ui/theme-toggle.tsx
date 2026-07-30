"use client";

import { useEffect, useState } from "react";
import { Monitor, Moon, Sun } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { THEME_STORAGE_KEY } from "@/lib/theme";

type ThemeMode = "light" | "dark" | "system";

function isThemeMode(value: string | null): value is ThemeMode {
  return value === "light" || value === "dark" || value === "system";
}

function resolveTheme(theme: ThemeMode): "light" | "dark" {
  if (theme === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }

  return theme;
}

function applyTheme(theme: ThemeMode): void {
  const root = document.documentElement;
  const resolvedTheme = resolveTheme(theme);

  root.classList.remove("light", "dark");
  root.classList.add(resolvedTheme);
  root.style.colorScheme = resolvedTheme;
}

function getStoredTheme(): ThemeMode {
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    return isThemeMode(stored) ? stored : "system";
  } catch {
    return "system";
  }
}

export function ThemeToggle() {
  const [theme, setTheme] = useState<ThemeMode>(() => {
    if (typeof window === "undefined") return "system";
    return getStoredTheme();
  });

  useEffect(() => {
    applyTheme(theme);

    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const handleMediaChange = () => {
      if (getStoredTheme() === "system") {
        applyTheme("system");
      }
    };

    if (typeof media.addEventListener === "function") {
      media.addEventListener("change", handleMediaChange);
      return () => media.removeEventListener("change", handleMediaChange);
    }

    media.addListener(handleMediaChange);
    return () => media.removeListener(handleMediaChange);
  }, [theme]);

  const handleThemeChange = (value: string) => {
    if (!isThemeMode(value)) return;

    setTheme(value);
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, value);
    } catch {}
    applyTheme(value);
  };

  return (
    <div className="fixed bottom-4 right-4 z-50">
      <Select value={theme} onValueChange={handleThemeChange}>
        <SelectTrigger
          aria-label="Select color theme"
          className="h-8 w-[130px] bg-background/90 text-xs backdrop-blur supports-[backdrop-filter]:bg-background/75"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent align="end">
          <SelectItem value="light">
            <span className="inline-flex items-center gap-2">
              <Sun className="h-3.5 w-3.5" />
              Light
            </span>
          </SelectItem>
          <SelectItem value="dark">
            <span className="inline-flex items-center gap-2">
              <Moon className="h-3.5 w-3.5" />
              Dark
            </span>
          </SelectItem>
          <SelectItem value="system">
            <span className="inline-flex items-center gap-2">
              <Monitor className="h-3.5 w-3.5" />
              System
            </span>
          </SelectItem>
        </SelectContent>
      </Select>
    </div>
  );
}
