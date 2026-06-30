'use client';

import { ArrowRight, Check, Copy } from 'lucide-react';
import { motion } from 'motion/react';
import { useTheme } from 'next-themes';
import { type HTMLAttributes, useState } from 'react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

// Omit the DOM `onCopy` (ClipboardEventHandler) so our command-string `onCopy`
// callback below doesn't conflict with it.
interface ScriptCopyBtnProps extends Omit<HTMLAttributes<HTMLDivElement>, 'onCopy'> {
  showMultiplePackageOptions?: boolean;
  codeLanguage: string;
  lightTheme: string;
  darkTheme: string;
  commandMap: Record<string, string>;
  className?: string;
  linkHref?: string;
  onCopy?: (command: string) => void;
}

export function ScriptCopyBtn({
  showMultiplePackageOptions = true,
  codeLanguage,
  lightTheme,
  darkTheme,
  commandMap,
  className,
  linkHref,
  onCopy,
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
    onCopy?.(command);
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
          <div className="min-w-0 font-mono">
            <button
              type="button"
              aria-label={
                linkHref ? 'Open docs' : copied ? 'Copied' : 'Copy to clipboard'
              }
              onClick={() => {
                if (linkHref) {
                  window.open(linkHref, '_blank', 'noopener,noreferrer');
                } else {
                  copyToClipboard();
                }
              }}
              className="h-10 w-fit max-w-full rounded border border-border bg-background px-3 text-[11px] sm:text-xs flex items-center cursor-pointer text-left hover:bg-muted/40 transition-colors"
            >
              <span className="block overflow-x-auto whitespace-pre">
                {command}
              </span>
            </button>
          </div>
          <Button
            aria-label={
              linkHref ? 'Open docs' : copied ? 'Copied' : 'Copy to clipboard'
            }
            className="relative ml-2 hidden rounded-md md:block cursor-pointer"
            onClick={() => {
              if (linkHref) {
                window.open(linkHref, '_blank', 'noopener,noreferrer');
              } else {
                copyToClipboard();
              }
            }}
            size="icon"
            variant="outline"
          >
            <span className="sr-only">
              {linkHref ? 'Open docs' : copied ? 'Copied' : 'Copy'}
            </span>
            <Copy
              className={`m-auto h-4 w-4 inset-0 transition-all duration-700 ${
                linkHref || copied ? 'scale-0 -rotate-90' : 'scale-100 rotate-0'
              }`}
            />
            <Check
              className={`absolute inset-0 m-auto h-4 w-4 transition-all duration-700 ${
                !linkHref && copied ? 'scale-100' : 'scale-0'
              }`}
            />
            <ArrowRight
              className={`absolute inset-0 m-auto h-4 w-4 transition-all duration-700 ${
                linkHref ? 'scale-100 rotate-0' : 'scale-0 rotate-90'
              }`}
            />
          </Button>
        </div>
      </div>
    </div>
  );
}
