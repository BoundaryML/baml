'use client';

import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@baml/ui/tooltip';
import { useAtomValue } from 'jotai';
import { AlertTriangle, CheckCircle, XCircle } from 'lucide-react';
import { useEffect, useState } from 'react';
import {
  bamlCliVersionAtom,
  numErrorsAtom,
  versionAtom,
} from '../baml_wasm_web/EventListener';
import { ErrorWarningDialog } from './ErrorWarningDialog';

const BreakpointBadge: React.FC = () => {
  const [breakpoint, setBreakpoint] = useState('xs');

  // Only show in development mode
  const isDev = process.env.NODE_ENV === 'development';

  useEffect(() => {
    if (!isDev) return;

    const updateBreakpoint = () => {
      const width = window.innerWidth;
      if (width >= 1536) {
        setBreakpoint('2xl');
      } else if (width >= 1280) {
        setBreakpoint('xl');
      } else if (width >= 1024) {
        setBreakpoint('lg');
      } else if (width >= 768) {
        setBreakpoint('md');
      } else if (width >= 640) {
        setBreakpoint('sm');
      } else {
        setBreakpoint('xs');
      }
    };

    updateBreakpoint();
    window.addEventListener('resize', updateBreakpoint);

    return () => window.removeEventListener('resize', updateBreakpoint);
  }, [isDev]);

  const breakpoints = [
    { name: '2xl', range: '1536px and up', min: 1536 },
    { name: 'xl', range: '1280px - 1535px', min: 1280, max: 1535 },
    { name: 'lg', range: '1024px - 1279px', min: 1024, max: 1279 },
    { name: 'md', range: '768px - 1023px', min: 768, max: 1023 },
    { name: 'sm', range: '640px - 767px', min: 640, max: 767 },
    { name: 'xs', range: 'below 640px', max: 639 },
  ];

  if (!isDev) return null;

  return (
    <TooltipProvider delayDuration={300}>
      <Tooltip>
        <TooltipTrigger asChild>
          <div className="px-2 py-1 bg-muted/50 rounded-md border text-[10px] font-mono text-muted-foreground">
            {breakpoint}
          </div>
        </TooltipTrigger>
        <TooltipContent side="top" className="p-4 w-48">
          <div className="space-y-3">
            <div className="text-xs font-medium text-foreground">
              Breakpoints
            </div>
            <div className="space-y-1.5">
              {breakpoints.map((bp) => (
                <div
                  key={bp.name}
                  className={`flex items-center gap-3 text-xs px-3 py-2 rounded ${
                    bp.name === breakpoint
                      ? 'bg-primary/10 text-primary border border-primary/20'
                      : 'text-muted-foreground'
                  }`}
                >
                  <div className="flex items-center gap-2 min-w-0 flex-1">
                    <span className="font-mono font-medium w-8 flex-shrink-0">
                      {bp.name}
                    </span>
                  </div>
                  <span className="text-muted-foreground text-right flex-shrink-0">
                    {bp.range}
                  </span>
                </div>
              ))}
            </div>
          </div>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
};

const ErrorCount: React.FC<{ onClick?: () => void }> = ({ onClick }) => {
  const { errors, warnings } = useAtomValue(numErrorsAtom);
  if (errors === 0 && warnings === 0) {
    return (
      <div className="flex flex-row gap-1 items-center text-green-600">
        <CheckCircle className="size-4" />
      </div>
    );
  }
  if (errors === 0) {
    return (
      <button
        type="button"
        onClick={onClick}
        className="flex flex-row gap-1 items-center text-yellow-600 hover:underline focus:outline-none"
        title="Show warnings"
      >
        {warnings} <AlertTriangle className="size-4" />
      </button>
    );
  }
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex flex-row gap-1 items-center text-red-600 hover:underline focus:outline-none"
      title="Show errors and warnings"
    >
      {errors} <XCircle className="size-4" /> {warnings}{' '}
      <AlertTriangle className="size-4" />
    </button>
  );
};

export const StatusBar: React.FC = () => {
  const bamlCliVersion = useAtomValue(bamlCliVersionAtom);
  const version = useAtomValue(versionAtom);
  const [showDialog, setShowDialog] = useState(false);

  return (
    <div className="w-full border-t bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60 flex-shrink-0">
      <div className="flex items-center justify-end px-4 py-2 text-xs gap-4">
        <BreakpointBadge />
        {bamlCliVersion && (
          <div className="text-muted-foreground">baml-cli {bamlCliVersion}</div>
        )}
        <div className="text-muted-foreground">VSCode Runtime: {version}</div>

        <ErrorCount onClick={() => setShowDialog(true)} />
        <ErrorWarningDialog open={showDialog} onOpenChange={setShowDialog} />
      </div>
    </div>
  );
};
