'use client';
import { MoonIcon, SunIcon } from '@radix-ui/react-icons';
import { useTheme } from 'next-themes';

import { Button } from '../components/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../components/dropdown-menu';

interface ThemeToggleProps {
  mode?: 'dropdown' | 'toggle';
  className?: string;
}

function ThemeToggle({ mode = 'dropdown', className }: ThemeToggleProps) {
  const { setTheme, theme } = useTheme();

  const toggleTheme = () => {
    setTheme(theme === 'dark' ? 'light' : 'dark');
  };

  if (mode === 'toggle') {
    return (
      <Button
        variant="outline"
        size="icon"
        onClick={toggleTheme}
        className={className}
      >
        <SunIcon className="dark:-rotate-90 size-5 rotate-0 scale-100 transition-all dark:scale-0" />
        <MoonIcon className="absolute size-5 rotate-90 scale-0 transition-all dark:rotate-0 dark:scale-100" />
        <span className="sr-only">Toggle theme</span>
      </Button>
    );
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="icon" className={className}>
          <SunIcon className="dark:-rotate-90 size-5 rotate-0 scale-100 transition-all dark:scale-0" />
          <MoonIcon className="absolute size-5 rotate-90 scale-0 transition-all dark:rotate-0 dark:scale-100" />
          <span className="sr-only">Toggle theme</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={() => setTheme('light')}>
          Light
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => setTheme('dark')}>
          Dark
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => setTheme('system')}>
          System
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export { ThemeToggle };

export { ThemeProvider } from 'next-themes';
