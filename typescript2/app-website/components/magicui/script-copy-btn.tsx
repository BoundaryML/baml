'use client';

import { Check, Copy } from 'lucide-react';
import { motion } from 'motion/react';
import { useTheme } from 'next-themes';
import { type HTMLAttributes, useState } from 'react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

interface ScriptCopyBtnProps extends HTMLAttributes<HTMLDivElement> {
  showMultiplePackageOptions?: boolean;
  codeLanguage: string;
  lightTheme: string;
  darkTheme: string;
  commandMap: Record<string, string>;
  className?: string;
}

export function ScriptCopyBtn({
  showMultiplePackageOptions = true,
  codeLanguage,
  lightTheme,
  darkTheme,
  commandMap,
  className,
}: ScriptCopyBtnProps) {
  const packageManagers = Object.keys(commandMap);
  const [packageManager, setPackageManager] = useState<string>(
    packageManagers[0] ?? '',
  );
  const [copied, setCopied] = useState(false);
  const { theme } = useTheme();
  const command = commandMap[packageManager] ?? '';

  const copyToClipboard = () => {
    navigator.clipboard.writeText(command);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div
      className={cn(
        'flex w-full max-w-lg items-center justify-center',
        className,
      )}
    >
      <div className="w-full space-y-2">
        <div className="mb-2 flex items-center">
          {showMultiplePackageOptions && (
            <div className="relative w-full">
              <div className="inline-flex w-full overflow-x-auto rounded-md border border-border text-xs scrollbar-hide">
                {packageManagers.map((pm, index) => (
                  <div className="flex items-center flex-shrink-0" key={pm}>
                    {index > 0 && (
                      <div aria-hidden="true" className="h-4 w-px bg-border" />
                    )}
                    <Button
                      className={`relative rounded-none bg-background px-2 py-1 hover:bg-background whitespace-nowrap ${
                        packageManager === pm
                          ? 'text-primary'
                          : 'text-muted-foreground'
                      }`}
                      onClick={() => setPackageManager(pm)}
                      size="sm"
                      variant="ghost"
                    >
                      {pm}
                      {packageManager === pm && (
                        <motion.div
                          className="absolute inset-x-0 bottom-[1px] mx-auto h-0.5 w-[90%] bg-primary"
                          initial={false}
                          layoutId="activeTab"
                          transition={{
                            damping: 30,
                            stiffness: 500,
                            type: 'spring',
                          }}
                        />
                      )}
                    </Button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
        <div className="relative flex items-center">
          <div className="w-full grow font-mono">
            <div className="h-10 rounded border border-border bg-background px-3 text-[11px] sm:text-xs flex items-center">
              <span className="block w-full overflow-x-auto whitespace-pre">
                {command}
              </span>
            </div>
          </div>
          <Button
            aria-label={copied ? 'Copied' : 'Copy to clipboard'}
            className="relative ml-2 hidden rounded-md md:block"
            onClick={copyToClipboard}
            size="icon"
            variant="outline"
          >
            <span className="sr-only">{copied ? 'Copied' : 'Copy'}</span>
            <Copy
              className={`m-auto h-4 w-4 inset-0 transition-all duration-300 ${
                copied ? 'scale-0' : 'scale-100'
              }`}
            />
            <Check
              className={`absolute inset-0 m-auto h-4 w-4 transition-all duration-300 ${
                copied ? 'scale-100' : 'scale-0'
              }`}
            />
          </Button>
        </div>
      </div>
    </div>
  );
}
