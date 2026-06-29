import { useState, useMemo, useRef, type FC } from 'react';
import {
  Eye,
  EyeOff,
  Trash2,
  Plus,
  Upload,
  AlertTriangle,
  Undo2,
  Terminal,
  ChevronDown,
  ChevronRight,
  Search,
} from 'lucide-react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from './ui/dialog';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Textarea } from './ui/textarea';
import { BOUNDARY_PROXY_URL_KEY } from '../proxy-config';

function apiKeyInputProps(key: string) {
  return {
    autoComplete: 'off',
    autoCorrect: 'off',
    autoCapitalize: 'off',
    spellCheck: false,
    'data-1p-ignore': 'true',
    'data-lpignore': 'true',
    'data-form-type': 'other',
    name: `baml-env-${key}`,
  } as const;
}

interface ApiKeysDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  envVars: Record<string, string>;
  /** Keys the project is known to need — accumulated from runtime requests. */
  requiredKeys: Set<string>;
  /** Original process env vars from the server's shell. */
  shellEnvVars: Record<string, string>;
  /** Shell keys that the user has manually overridden or deleted. */
  shellOverriddenKeys: Set<string>;
  /** Shell keys that the user has deleted. */
  shellDeletedKeys: Set<string>;
  onSetEnvVar: (key: string, value: string) => void;
  onDeleteEnvVar: (key: string) => void;
  onImportEnvVars: (vars: Record<string, string>) => void;
  /** Revert a key to its original shell value. */
  onRevertToShell: (key: string) => void;
  /**
   * Whether to surface the Boundary gateway toggle. promptfiddle shows it; the
   * VS Code extension and CLI playground hide it even if present in the env.
   */
  showProxyEnvVar?: boolean;
  /** Whether the gateway is on (BOUNDARY_PROXY_URL present in the BAML env). */
  proxyEnabled?: boolean;
  /** Flip the gateway on/off. */
  onToggleProxy?: (enabled: boolean) => void;
}

export const ApiKeysDialog: FC<ApiKeysDialogProps> = ({
  open,
  onOpenChange,
  envVars,
  requiredKeys,
  shellEnvVars,
  shellOverriddenKeys,
  shellDeletedKeys,
  onSetEnvVar,
  onDeleteEnvVar,
  onImportEnvVars,
  onRevertToShell,
  showProxyEnvVar = false,
  proxyEnabled = false,
  onToggleProxy,
}) => {
  const [showValues, setShowValues] = useState<Set<string>>(new Set());
  const [newKey, setNewKey] = useState('');
  const [newValue, setNewValue] = useState('');
  const [importMode, setImportMode] = useState(false);
  const [importText, setImportText] = useState('');
  const [shellExpanded, setShellExpanded] = useState(false);
  const [shellFilter, setShellFilter] = useState('');
  const debounceTimers = useRef<Map<string, ReturnType<typeof setTimeout>>>(
    new Map(),
  );

  const missingKeys = new Set([...requiredKeys].filter((k) => !envVars[k]));

  // Primary keys: required by BAML + manually added (not from shell) + overridden/deleted shell keys
  const primaryKeys = useMemo(() => {
    const keys = new Set<string>();
    for (const k of requiredKeys) keys.add(k);
    for (const k of shellDeletedKeys) keys.add(k);
    for (const k of shellOverriddenKeys) keys.add(k);
    // Manually added keys (in envVars but not from shell and not already required)
    for (const k of Object.keys(envVars)) {
      if (!(k in shellEnvVars)) keys.add(k);
    }
    // BOUNDARY_PROXY_URL never appears as a normal row: promptfiddle renders it
    // in its own toggle section, everywhere else it's hidden entirely.
    keys.delete(BOUNDARY_PROXY_URL_KEY);
    return [...keys].sort();
  }, [
    envVars,
    requiredKeys,
    shellEnvVars,
    shellDeletedKeys,
    shellOverriddenKeys,
  ]);

  // Shell keys not already shown in primary section. The proxy var is never
  // listed here — it's either pinned to the primary section or fully hidden.
  const shellOnlyKeys = useMemo(() => {
    const primarySet = new Set(primaryKeys);
    const keys = Object.keys(shellEnvVars).filter(
      (k) => !primarySet.has(k) && k !== BOUNDARY_PROXY_URL_KEY,
    );
    if (!shellFilter) return keys.sort();
    const lower = shellFilter.toLowerCase();
    return keys.filter((k) => k.toLowerCase().includes(lower)).sort();
  }, [shellEnvVars, primaryKeys, shellFilter]);

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
    debounceTimers.current.set(
      key,
      setTimeout(() => onSetEnvVar(key, value), 200),
    );
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
      if (
        (value.startsWith('"') && value.endsWith('"')) ||
        (value.startsWith("'") && value.endsWith("'"))
      ) {
        value = value.slice(1, -1);
      }
      if (key) vars[key] = value;
    }
    onImportEnvVars(vars);
    setImportText('');
    setImportMode(false);
  };

  const renderProxySection = () => (
    <div className="space-y-1.5 pb-3 border-b border-vsc-border">
      <div className="flex items-center gap-2">
        <span
          className={`flex-1 text-[12px] ${proxyEnabled ? 'text-vsc-text' : 'text-vsc-description'}`}
        >
          Use Boundary LLM gateway free tier
        </span>
        {/* Reserve the same trailing width as the two icon buttons (eye +
            trash) on normal rows so it lines up, and center the switch
            between those two columns. */}
        <div className="flex items-center justify-center shrink-0 w-16">
          <button
            type="button"
            role="switch"
            aria-checked={proxyEnabled}
            aria-label="Use Boundary LLM gateway free tier"
            onClick={() => onToggleProxy?.(!proxyEnabled)}
            className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring/50 ${proxyEnabled ? 'bg-primary' : 'bg-vsc-border'}`}
          >
            <span
              className={`inline-block h-4 w-4 rounded-full bg-white shadow-sm transition-transform ${proxyEnabled ? 'translate-x-[18px]' : 'translate-x-0.5'}`}
            />
          </button>
        </div>
      </div>
      <p className="text-[10px] text-vsc-description leading-snug">
        On, requests use Boundary&rsquo;s keys through our proxy. Off, they use
        the API keys below.
      </p>
    </div>
  );

  const renderRow = (key: string) => {
    const value = envVars[key] ?? '';
    const isMissing = missingKeys.has(key);
    const isFromShell = key in shellEnvVars;
    const isOverridden = shellOverriddenKeys.has(key);
    const isDeleted = shellDeletedKeys.has(key);
    return (
      <div
        key={key}
        className={`flex items-center gap-2 ${isDeleted ? 'opacity-50' : ''}`}
      >
        <div className="flex items-center gap-1 shrink-0 w-[160px]">
          {isMissing && !isDeleted && (
            <AlertTriangle size={12} className="text-yellow-400" />
          )}
          {isFromShell && !isMissing && (
            <Terminal size={10} className="text-vsc-description shrink-0" />
          )}
          <span
            className={`font-vsc-mono text-[11px] truncate ${isDeleted ? 'line-through text-vsc-description' : 'text-vsc-text'}`}
          >
            {key}
          </span>
          {isDeleted && (
            <span
              className="text-[9px] text-vsc-red shrink-0"
              title="Deleted (was from shell)"
            >
              deleted
            </span>
          )}
          {isFromShell && !isOverridden && !isDeleted && (
            <span
              className="text-[9px] text-vsc-description shrink-0"
              title="From shell environment"
            >
              shell
            </span>
          )}
          {isOverridden && !isDeleted && (
            <span
              className="text-[9px] text-yellow-500 shrink-0"
              title="Overridden (shell value differs)"
            >
              edited
            </span>
          )}
        </div>
        {isDeleted ? (
          <div className="flex-1 text-[11px] font-vsc-mono text-vsc-description italic px-2">
            removed
          </div>
        ) : (
          <Input
            type="text"
            defaultValue={value}
            key={`${key}-${isOverridden}-${isDeleted}`}
            onChange={(e) => handleInlineEdit(key, e.target.value)}
            placeholder={isMissing ? 'Required' : ''}
            className={`flex-1 text-[11px] font-vsc-mono ${showValues.has(key) ? '' : '[text-security:disc] [-webkit-text-security:disc]'}`}
            {...apiKeyInputProps(key)}
          />
        )}
        {!isDeleted && (
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => toggleShow(key)}
          >
            {showValues.has(key) ? <EyeOff size={12} /> : <Eye size={12} />}
          </Button>
        )}
        {isOverridden || isDeleted ? (
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-vsc-link"
            onClick={() => onRevertToShell(key)}
            title="Revert to shell value"
          >
            <Undo2 size={12} />
          </Button>
        ) : (
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-vsc-red"
            onClick={() => onDeleteEnvVar(key)}
          >
            <Trash2 size={12} />
          </Button>
        )}
      </div>
    );
  };

  const shellVarCount = Object.keys(shellEnvVars).length;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="w-[520px] max-h-[80vh] flex flex-col overflow-hidden">
        <DialogHeader>
          <DialogTitle>Environment Variables</DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-auto p-4 space-y-3">
          {/* Proxy toggle: pinned first, always visible (promptfiddle only) */}
          {showProxyEnvVar && renderProxySection()}

          {/* Primary section: required + manually added */}
          {primaryKeys.length > 0 && (
            <div className="space-y-2">{primaryKeys.map(renderRow)}</div>
          )}

          {primaryKeys.length === 0 && (
            <div className="text-[11px] text-vsc-description text-center py-2">
              No variables required yet. Run a function to see which env vars
              are needed.
            </div>
          )}

          {/* Add new key */}
          <div className="flex items-center gap-2 pt-2 border-t border-vsc-border">
            <Input
              placeholder="KEY"
              value={newKey}
              onChange={(e) => setNewKey(e.target.value)}
              className="w-[160px] text-[11px] font-vsc-mono"
              {...apiKeyInputProps('new-key')}
            />
            <Input
              type="text"
              placeholder="Value"
              value={newValue}
              onChange={(e) => setNewValue(e.target.value)}
              className="[text-security:disc] [-webkit-text-security:disc] flex-1 text-[11px] font-vsc-mono"
              {...apiKeyInputProps(newKey || 'new-value')}
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
                placeholder={
                  'Paste .env contents here...\nKEY=value\nANOTHER_KEY=value'
                }
                rows={5}
                className="text-[11px] font-vsc-mono resize-none"
              />
              <div className="flex gap-2">
                <Button variant="default" size="sm" onClick={handleImport}>
                  Import
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    setImportMode(false);
                    setImportText('');
                  }}
                >
                  Cancel
                </Button>
              </div>
            </div>
          ) : (
            <Button
              variant="link"
              size="sm"
              className="text-vsc-link text-[11px] gap-1 px-0"
              onClick={() => setImportMode(true)}
            >
              <Upload size={12} /> Import from .env
            </Button>
          )}

          {/* Shell environment section (collapsible) */}
          {shellVarCount > 0 && (
            <div className="pt-2 border-t border-vsc-border">
              <button
                onClick={() => setShellExpanded((prev) => !prev)}
                className="flex items-center gap-1 text-[11px] text-vsc-description hover:text-vsc-text w-full text-left py-1"
              >
                {shellExpanded ? (
                  <ChevronDown size={12} />
                ) : (
                  <ChevronRight size={12} />
                )}
                <Terminal size={10} />
                <span>All shell environment ({shellVarCount})</span>
              </button>

              {shellExpanded && (
                <div className="mt-2 space-y-2">
                  {/* Search filter */}
                  <div className="relative">
                    <Search
                      size={12}
                      className="absolute left-2 top-1/2 -translate-y-1/2 text-vsc-description"
                    />
                    <Input
                      placeholder="Filter variables..."
                      value={shellFilter}
                      onChange={(e) => setShellFilter(e.target.value)}
                      className="pl-7 text-[11px] font-vsc-mono"
                    />
                  </div>

                  <div className="space-y-1 max-h-[200px] overflow-auto">
                    {shellOnlyKeys.map((key) => (
                      <div key={key} className="flex items-center gap-2 py-0.5">
                        <span
                          className="font-vsc-mono text-[10px] text-vsc-description shrink-0 w-[160px] truncate"
                          title={key}
                        >
                          {key}
                        </span>
                        <Input
                          type="text"
                          defaultValue={shellEnvVars[key]}
                          onChange={(e) =>
                            handleInlineEdit(key, e.target.value)
                          }
                          className={`flex-1 text-[10px] font-vsc-mono h-6 ${showValues.has(key) ? '' : '[text-security:disc] [-webkit-text-security:disc]'}`}
                          {...apiKeyInputProps(key)}
                        />
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-5 w-5"
                          onClick={() => toggleShow(key)}
                        >
                          {showValues.has(key) ? (
                            <EyeOff size={10} />
                          ) : (
                            <Eye size={10} />
                          )}
                        </Button>
                      </div>
                    ))}
                    {shellOnlyKeys.length === 0 && shellFilter && (
                      <div className="text-[10px] text-vsc-description text-center py-2">
                        No matches
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
};
