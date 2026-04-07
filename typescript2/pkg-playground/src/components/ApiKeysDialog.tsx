import { useState, useRef, type FC } from 'react';
import { Eye, EyeOff, Trash2, Plus, Upload, AlertTriangle } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Textarea } from './ui/textarea';

interface ApiKeysDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  envVars: Record<string, string>;
  /** Keys the project is known to need — accumulated from runtime requests. */
  requiredKeys: Set<string>;
  onSetEnvVar: (key: string, value: string) => void;
  onDeleteEnvVar: (key: string) => void;
  onImportEnvVars: (vars: Record<string, string>) => void;
}

export const ApiKeysDialog: FC<ApiKeysDialogProps> = ({
  open, onOpenChange, envVars, requiredKeys, onSetEnvVar, onDeleteEnvVar, onImportEnvVars,
}) => {
  const [showValues, setShowValues] = useState<Set<string>>(new Set());
  const [newKey, setNewKey] = useState('');
  const [newValue, setNewValue] = useState('');
  const [importMode, setImportMode] = useState(false);
  const [importText, setImportText] = useState('');
  const debounceTimers = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  const missingKeys = new Set([...requiredKeys].filter((k) => !envVars[k]));

  const toggleShow = (key: string) => {
    setShowValues((prev) => {
      const next = new Set(prev);
      next.has(key) ? next.delete(key) : next.add(key);
      return next;
    });
  };

  const handleInlineEdit = (key: string, value: string) => {
    const existing = debounceTimers.current.get(key);
    if (existing) clearTimeout(existing);
    debounceTimers.current.set(key, setTimeout(() => onSetEnvVar(key, value), 200));
  };

  const handleAdd = () => {
    if (!newKey.trim()) return;
    onSetEnvVar(newKey.trim(), newValue);
    setNewKey('');
    setNewValue('');
  };

  const handleImport = () => {
    const vars: Record<string, string> = {};
    for (const line of importText.split('\n')) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('#')) continue;
      const eqIdx = trimmed.indexOf('=');
      if (eqIdx === -1) continue;
      const key = trimmed.slice(0, eqIdx).trim();
      let value = trimmed.slice(eqIdx + 1).trim();
      if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) {
        value = value.slice(1, -1);
      }
      if (key) vars[key] = value;
    }
    onImportEnvVars(vars);
    setImportText('');
    setImportMode(false);
  };

  const allKeys = [...new Set([...Object.keys(envVars), ...requiredKeys])].sort();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="w-[480px] max-h-[80vh] flex flex-col overflow-hidden" data-1p-ignore data-lpignore="true">
        <DialogHeader>
          <DialogTitle>API Keys</DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-auto p-4 space-y-3">
          {allKeys.map((key) => {
            const value = envVars[key] ?? '';
            const isMissing = missingKeys.has(key);
            return (
              <div key={key} className="flex items-center gap-2">
                <div className="flex items-center gap-1 shrink-0 w-[140px]">
                  {isMissing && <AlertTriangle size={12} className="text-yellow-400" />}
                  <span className="font-vsc-mono text-[11px] text-vsc-text truncate">{key}</span>
                </div>
                <Input
                  type={showValues.has(key) ? 'text' : 'password'}
                  defaultValue={value}
                  onChange={(e) => handleInlineEdit(key, e.target.value)}
                  placeholder={isMissing ? 'Required' : ''}
                  className="flex-1 text-[11px] font-vsc-mono"
                  autoComplete="off"
                  data-1p-ignore
                  data-lpignore="true"
                  data-form-type="other"
                />
                <Button variant="ghost" size="icon" className="h-7 w-7" onClick={() => toggleShow(key)}>
                  {showValues.has(key) ? <EyeOff size={12} /> : <Eye size={12} />}
                </Button>
                <Button variant="ghost" size="icon" className="h-7 w-7 text-vsc-red" onClick={() => onDeleteEnvVar(key)}>
                  <Trash2 size={12} />
                </Button>
              </div>
            );
          })}

          {/* Add new key */}
          <div className="flex items-center gap-2 pt-2 border-t border-vsc-border">
            <Input
              placeholder="KEY"
              value={newKey}
              onChange={(e) => setNewKey(e.target.value)}
              className="w-[140px] text-[11px] font-vsc-mono"
              autoComplete="off"
              data-1p-ignore
              data-lpignore="true"
              data-form-type="other"
            />
            <Input
              placeholder="Value"
              value={newValue}
              onChange={(e) => setNewValue(e.target.value)}
              className="flex-1 text-[11px] font-vsc-mono"
              autoComplete="off"
              data-1p-ignore
              data-lpignore="true"
              data-form-type="other"
            />
            <Button variant="default" size="sm" onClick={handleAdd}>
              <Plus size={12} />
            </Button>
          </div>

          {/* .env Import */}
          {importMode ? (
            <div className="space-y-2 pt-2">
              <Textarea
                value={importText}
                onChange={(e) => setImportText(e.target.value)}
                placeholder={"Paste .env contents here...\nKEY=value\nANOTHER_KEY=value"}
                rows={5}
                className="text-[11px] font-vsc-mono resize-none"
              />
              <div className="flex gap-2">
                <Button variant="default" size="sm" onClick={handleImport}>
                  Import
                </Button>
                <Button variant="ghost" size="sm" onClick={() => { setImportMode(false); setImportText(''); }}>
                  Cancel
                </Button>
              </div>
            </div>
          ) : (
            <Button variant="link" size="sm" className="text-vsc-link text-[11px] gap-1 px-0" onClick={() => setImportMode(true)}>
              <Upload size={12} /> Import from .env
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
};
