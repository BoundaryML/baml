import { useState, useRef, type FC } from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import { X, Eye, EyeOff, Trash2, Plus, Upload, AlertTriangle } from 'lucide-react';

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
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/40 z-50" />
        <Dialog.Content className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[480px] max-h-[80vh] bg-vsc-bg border border-vsc-border rounded-lg shadow-xl z-50 flex flex-col overflow-hidden">
          <div className="flex items-center justify-between px-4 py-3 border-b border-vsc-border">
            <Dialog.Title className="text-sm font-semibold text-vsc-text">API Keys</Dialog.Title>
            <Dialog.Close className="p-1 rounded hover:bg-vsc-hover text-vsc-text-muted">
              <X size={14} />
            </Dialog.Close>
          </div>

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
                  <input
                    type={showValues.has(key) ? 'text' : 'password'}
                    defaultValue={value}
                    onChange={(e) => handleInlineEdit(key, e.target.value)}
                    placeholder={isMissing ? 'Required' : ''}
                    className="flex-1 px-2 py-1 text-[11px] font-vsc-mono rounded border border-vsc-input-border bg-vsc-input-bg text-vsc-input-fg outline-none"
                  />
                  <button onClick={() => toggleShow(key)} className="p-1 text-vsc-text-muted hover:text-vsc-text">
                    {showValues.has(key) ? <EyeOff size={12} /> : <Eye size={12} />}
                  </button>
                  <button onClick={() => onDeleteEnvVar(key)} className="p-1 text-vsc-text-muted hover:text-vsc-error">
                    <Trash2 size={12} />
                  </button>
                </div>
              );
            })}

            {/* Add new key */}
            <div className="flex items-center gap-2 pt-2 border-t border-vsc-border">
              <input
                placeholder="KEY"
                value={newKey}
                onChange={(e) => setNewKey(e.target.value)}
                className="w-[140px] px-2 py-1 text-[11px] font-vsc-mono rounded border border-vsc-input-border bg-vsc-input-bg text-vsc-input-fg outline-none"
              />
              <input
                placeholder="Value"
                value={newValue}
                onChange={(e) => setNewValue(e.target.value)}
                className="flex-1 px-2 py-1 text-[11px] font-vsc-mono rounded border border-vsc-input-border bg-vsc-input-bg text-vsc-input-fg outline-none"
              />
              <button onClick={handleAdd} className="p-1 rounded bg-vsc-accent text-vsc-accent-fg hover:opacity-80">
                <Plus size={12} />
              </button>
            </div>

            {/* .env Import */}
            {importMode ? (
              <div className="space-y-2 pt-2">
                <textarea
                  value={importText}
                  onChange={(e) => setImportText(e.target.value)}
                  placeholder={"Paste .env contents here...\nKEY=value\nANOTHER_KEY=value"}
                  rows={5}
                  className="w-full px-2 py-1.5 text-[11px] font-vsc-mono rounded border border-vsc-input-border bg-vsc-input-bg text-vsc-input-fg outline-none resize-none"
                />
                <div className="flex gap-2">
                  <button onClick={handleImport} className="px-3 py-1 text-[11px] rounded bg-vsc-accent text-vsc-accent-fg hover:opacity-80">
                    Import
                  </button>
                  <button onClick={() => { setImportMode(false); setImportText(''); }} className="px-3 py-1 text-[11px] rounded text-vsc-text-muted hover:bg-vsc-hover">
                    Cancel
                  </button>
                </div>
              </div>
            ) : (
              <button onClick={() => setImportMode(true)} className="flex items-center gap-1 text-[11px] text-vsc-link hover:underline">
                <Upload size={12} /> Import from .env
              </button>
            )}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
};
