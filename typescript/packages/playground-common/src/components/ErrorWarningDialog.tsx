import React from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogTrigger,
  DialogClose,
  DialogFooter,
} from '@baml/ui/dialog';
import { AlertTriangle, XCircle, Info } from 'lucide-react';
import { useAtomValue } from 'jotai';
import { diagnosticsAtom, filesAtom, type DiagnosticError } from '../sdk/atoms/core.atoms';

function locationOf(d: DiagnosticError, files: Record<string, string>): string | null {
  if (!d.filePath) return null;
  const content = files[d.filePath];
  if (!content) return d.filePath;
  let line = 1;
  let col = 1;
  for (let i = 0; i < d.start_ch && i < content.length; i++) {
    if (content[i] === '\n') {
      line++;
      col = 1;
    } else {
      col++;
    }
  }
  return `${d.filePath}:${line}:${col}`;
}

export const ErrorWarningDialog: React.FC<{
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  trigger?: React.ReactNode;
}> = ({ open, onOpenChange, trigger }) => {
  const diagnostics = useAtomValue(diagnosticsAtom);
  const files = useAtomValue(filesAtom);
  const errors = diagnostics.filter((d) => d.type === 'error');
  const warnings = diagnostics.filter((d) => d.type === 'warning');

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {trigger ? <DialogTrigger asChild>{trigger}</DialogTrigger> : null}
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <XCircle className="text-red-500" size={20} />
            Errors & Warnings
          </DialogTitle>
          <DialogDescription>
            Review the following issues in your project. Errors must be fixed to proceed. Warnings are recommended to address.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 mt-2 max-h-[70vh] overflow-y-auto">
          {errors.length > 0 && (
            <div>
              <div className="flex items-center gap-2 text-red-600 font-semibold mb-1">
                <XCircle size={16} /> {errors.length} Error{errors.length > 1 ? 's' : ''}
              </div>
              <ul className="space-y-3">
                {errors.map((err, i) => (
                  <li key={i} className="bg-accent border border-l-chart-5 rounded p-2 text-sm">
                    <div className="font-medium">{err.message}</div>
                    {locationOf(err, files) && (
                      <div className="text-xs text-muted-foreground mt-1">
                        <span className="font-mono">{locationOf(err, files)}</span>
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}
          {warnings.length > 0 && (
            <div>
              <div className="flex items-center gap-2 text-chart-4 font-semibold mb-1">
                <AlertTriangle size={16} /> {warnings.length} Warning{warnings.length > 1 ? 's' : ''}
              </div>
              <ul className="space-y-3">
                {warnings.map((warn, i) => (
                  <li key={i} className="bg-accent border border-l-chart-4 rounded p-2 text-sm">
                    <div className="font-medium">{warn.message}</div>
                    {locationOf(warn, files) && (
                      <div className="text-xs text-muted-foreground mt-1">
                        <span className="font-mono">{locationOf(warn, files)}</span>
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}
          {errors.length === 0 && warnings.length === 0 && (
            <div className="flex items-center gap-2 text-chart-2">
              <Info size={18} /> No errors or warnings!
            </div>
          )}
        </div>
        <DialogFooter>
          <DialogClose asChild>
            <button className="px-4 py-2 rounded bg-muted-foreground/10 hover:bg-muted-foreground/20 transition-colors">Close</button>
          </DialogClose>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};