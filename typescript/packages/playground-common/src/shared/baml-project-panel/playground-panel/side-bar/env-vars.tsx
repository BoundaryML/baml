'use client';

import React from 'react';
import { Button } from '@baml/ui/button';
import { Checkbox } from '@baml/ui/checkbox';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@baml/ui/dialog';
import { Input } from '@baml/ui/input';
import { Label } from '@baml/ui/label';
import { toast } from '@baml/ui/sonner';
import { Textarea } from '@baml/ui/textarea';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@baml/ui/tooltip';
import { QuestionMarkCircledIcon } from '@radix-ui/react-icons';
import { parse as parseDotenv } from 'dotenv';
import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai';
import { sortBy } from 'lodash';
import { Save } from 'lucide-react';
import {
  AlertTriangle,
  Check,
  Eye,
  EyeOff,
  PlusCircle,
  Settings2,
  Trash2,
} from 'lucide-react';
import { FileText } from 'lucide-react';
import { useState, useCallback, useTransition } from 'react';
import {
  type BamlConfigAtom,
  bamlConfig,
} from '../../../../baml_wasm_web/bamlConfig';
import {
  proxyUrlAtom,
  requiredEnvVarsAtom,
  userEnvVarsAtom,
} from '../../atoms';
import { vscode } from '../../vscode';

const envVarVisibilityAtom = atom<Record<string, boolean>>({});

const REQUIRED_ENV_VAR_UNSET_WARNING =
  'Your BAML clients may fail if this is not set';

interface EnvVarEntry {
  key: string;
  value: string | undefined;
  required: boolean;
  hidden: boolean;
}

const renderedEnvVarsAtom = atom<EnvVarEntry[]>((get) => {
  const userEnvVars = get(userEnvVarsAtom) as Record<string, string>;
  const requiredEnvVars = get(requiredEnvVarsAtom);
  const visibility = get(envVarVisibilityAtom);

  const vars: EnvVarEntry[] = Object.entries(userEnvVars).map(
    ([key, value]) => ({
      key,
      value,
      required: requiredEnvVars.includes(key),
      hidden: visibility[key] !== false, // hidden by default unless explicitly set to false
    }),
  );

  const missingVars = requiredEnvVars.filter(
    (envVar) => !(envVar in userEnvVars),
  );

  vars.push(
    ...missingVars.map((envVar) => ({
      key: envVar,
      value: undefined,
      required: true,
      hidden: visibility[envVar] !== false,
    })),
  );

  return sortBy(vars, [(v) => v.key]);
});

const escapeValue = (value: string): string => {
  return value.replace(/[\n\r\t]/g, (match) => {
    switch (match) {
      case '\n':
        return '\\n';
      case '\r':
        return '\\r';
      case '\t':
        return '\\t';
      default:
        return match;
    }
  });
};

const unescapeValue = (value: string): string => {
  return value.replace(/\\[nrt]/g, (match) => {
    switch (match) {
      case '\\n':
        return '\n';
      case '\\r':
        return '\r';
      case '\\t':
        return '\t';
      default:
        return match;
    }
  });
};

function EnvVarStatus({
  value,
  required,
}: { value?: string; required: boolean }) {
  if (!value || value === '') {
    return (
      <TooltipProvider delayDuration={300}>
        <Tooltip>
          <TooltipTrigger asChild>
            <AlertTriangle className="h-4 w-4 text-orange-500 flex-shrink-0" />
          </TooltipTrigger>
          <TooltipContent side="top" className="text-xs">
            {REQUIRED_ENV_VAR_UNSET_WARNING}
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  if (required) {
    return (
      <TooltipProvider delayDuration={300}>
        <Tooltip>
          <TooltipTrigger asChild>
            <Check className="h-4 w-4 text-green-500 flex-shrink-0" />
          </TooltipTrigger>
          <TooltipContent side="top" className="text-xs">
            Used by one of your BAML clients
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  return <div />;
}

// Memoized component for individual environment variable row
const EnvVarRow = React.memo(({
  env,
  onUpdate,
  onDelete,
  onToggleVisibility,
}: {
  env: EnvVarEntry;
  onUpdate: (key: string, value: string) => void;
  onDelete: (key: string) => void;
  onToggleVisibility: (key: string) => void;
}) => {
  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    onUpdate(env.key, unescapeValue(e.target.value));
  }, [env.key, onUpdate]);

  return (
    <tr className="relative hover:bg-accent/50 rounded-md">
      <td className="pl-2 pr-0.5 py-0.5">
        <div className="flex items-center gap-2 justify-between">
          <code className="font-mono text-xs text-muted-foreground">
            {env.key}
          </code>
          <EnvVarStatus value={env.value} required={env.required} />
        </div>
      </td>
      <td className="px-0.5 py-0.5">
        <Input
          type={env.hidden ? 'password' : 'text'}
          value={typeof env.value === 'string' ? escapeValue(env.value) : ''}
          onChange={handleChange}
          className="h-6 text-xs font-mono placeholder:font-sans min-w-32"
          placeholder={
            env.required && !env.value ? '<unset>' : undefined
          }
          autoComplete="off"
          data-1p-ignore
        />
      </td>
      <td className="pl-0.5 pr-2 py-0.5 text-right">
        <div className="flex gap-1 justify-end">
          <Button
            variant="ghost"
            size="sm"
            className="p-0.5 w-5 h-5"
            onClick={() => onToggleVisibility(env.key)}
          >
            {env.hidden ? (
              <EyeOff className="w-4 h-4 text-muted-foreground hover:text-primary" />
            ) : (
              <Eye className="w-4 h-4 text-muted-foreground hover:text-primary" />
            )}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="p-0.5 w-5 h-5"
            onClick={() => onDelete(env.key)}
          >
            <Trash2 className="w-4 h-4 text-muted-foreground hover:text-destructive" />
          </Button>
        </div>
      </td>
    </tr>
  );
});

export const EnvironmentVariablesPanel: React.FC = () => {
  const [userEnvVars, setUserEnvVars] = useAtom(userEnvVarsAtom);
  const requiredEnvVars = useAtomValue(requiredEnvVarsAtom);
  const visibility = useAtomValue(envVarVisibilityAtom);
  const setVisibility = useSetAtom(envVarVisibilityAtom);
  const proxySettings = useAtomValue(proxyUrlAtom);
  const setBamlConfig = useSetAtom(bamlConfig);
  const [, startTransition] = useTransition();

  // Local state for all environment variables to avoid triggering runtime updates on every change
  const [localEnvVars, setLocalEnvVars] = useState<Record<string, string>>({});
  const [hasLocalChanges, setHasLocalChanges] = useState(false);

  // Initialize local state from global state
  React.useEffect(() => {
    if (!hasLocalChanges) {
      setLocalEnvVars(userEnvVars);
    }
  }, [userEnvVars, hasLocalChanges]);

  // Compute rendered env vars locally to avoid atom recalculation
  const envVars = React.useMemo(() => {
    const vars: EnvVarEntry[] = Object.entries(localEnvVars).map(
      ([key, value]) => ({
        key,
        value,
        required: requiredEnvVars.includes(key),
        hidden: visibility[key] !== false,
      }),
    );

    const missingVars = requiredEnvVars.filter(
      (envVar) => !(envVar in localEnvVars),
    );

    vars.push(
      ...missingVars.map((envVar) => ({
        key: envVar,
        value: undefined,
        required: true,
        hidden: visibility[envVar] !== false,
      })),
    );

    return sortBy(vars, [(v) => v.key]);
  }, [localEnvVars, requiredEnvVars, visibility]);

  const [newKey, setNewKey] = useState('');
  const [newValue, setNewValue] = useState('');
  const [envFileContent, setEnvFileContent] = useState('');

  // Memoize callbacks to prevent unnecessary re-renders
  const toggleVisibility = useCallback((key: string) => {
    setVisibility((prev) => ({
      ...prev,
      [key]: !prev[key],
    }));
  }, [setVisibility]);

  const updateEnvVar = useCallback((key: string, value: string) => {
    setLocalEnvVars(prev => ({
      ...prev,
      [key]: value,
    }));
    setHasLocalChanges(true);
  }, []);

  const deleteEnvVar = useCallback((key: string) => {
    setLocalEnvVars(prev => {
      const newVars = { ...prev };
      delete newVars[key];
      return newVars;
    });
    setHasLocalChanges(true);
  }, []);

  const addEnvVar = useCallback(() => {
    if (newKey.trim() === '') return;

    setLocalEnvVars(prev => ({
      ...prev,
      [newKey]: newValue,
    }));
    setHasLocalChanges(true);

    // Reset form
    setNewKey('');
    setNewValue('');
  }, [newKey, newValue]);

  const saveChanges = useCallback(() => {
    startTransition(() => {
      setUserEnvVars(localEnvVars);
      setHasLocalChanges(false);
    });
  }, [localEnvVars, setUserEnvVars]);

  const parseAndSaveEnvFile = useCallback(() => {
    try {
      const parsed = parseDotenv(envFileContent);
      setLocalEnvVars(prev => ({
        ...prev,
        ...parsed,
      }));
      setHasLocalChanges(true);
      setEnvFileContent('');
      toast.success(
        `Successfully imported ${Object.keys(parsed).length} variables`,
      );
    } catch (error) {
      toast.error('Error parsing .env file', {
        description: 'Please check the format of your .env file',
      });
    }
  }, [envFileContent]);

  return (
    <div className="p-2 space-y-2 text-sm">
      <div className="flex justify-between items-center">
        <h3 className="flex gap-2 items-center font-medium text-muted-foreground">
          <Settings2 className="w-4 h-4" />
          Environment Variables
        </h3>
        {hasLocalChanges && (
          <Button 
            size="sm" 
            variant="outline" 
            onClick={saveChanges}
            className="h-7"
          >
            <Save className="w-3 h-3 mr-1" />
            Save Changes
          </Button>
        )}
      </div>
      <div className="text-left text-muted-foreground">
        <p>
          Set your own API Keys here.&nbsp;
          <a
            href="https://docs.boundaryml.com/ref/llm-client-providers/overview#fields"
            target="_blank"
            rel="noopener noreferrer"
            className="text-blue-500 hover:underline"
          >
            See supported LLMs
          </a>
        </p>
      </div>
      <div className="text-left text-muted-foreground">
        <div className="flex gap-2 items-center">
          <div className="flex gap-2 items-center">
            <TooltipProvider delayDuration={300}>
              <Tooltip>
                <TooltipTrigger asChild>
                  <QuestionMarkCircledIcon className="w-4 h-4" />
                </TooltipTrigger>
                <TooltipContent side="top" className="text-xs w-80">
                  The BAML playground directly calls the LLM provider's API.
                  Some providers make it difficult for browsers to call their
                  API due to CORS restrictions.
                  <br />
                  <br />
                  To get around this, the BAML VSCode extension includes a{' '}
                  <b>localhost proxy</b> that sits between your browser and the
                  LLM provider's API.
                  <br />
                  <br />
                  <b>
                    BAML MAKES NO NETWORK CALLS BEYOND THE LLM PROVIDER'S API
                    YOU SPECIFY.
                  </b>
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
            <p>
              VSCode proxy is{' '}
              <b>{proxySettings.proxyEnabled ? 'enabled' : 'disabled'}</b>
            </p>
            <Checkbox
              checked={proxySettings.proxyEnabled}
              onCheckedChange={async (checked) => {
                try {
                  await vscode.setProxySettings(!!checked);
                  // Update local config to reflect the change immediately
                  setBamlConfig((prev: BamlConfigAtom) => ({
                    ...prev,
                    config: {
                      ...prev.config,
                    },
                  }));
                } catch (error) {
                  console.error('Failed to update proxy settings:', error);
                  toast.error('Error updating proxy settings', {
                    description: 'Please try again',
                  });
                }
              }}
            />
          </div>
          <p>{proxySettings.proxyUrl}</p>
        </div>
      </div>

      <div className="space-y-1">
        <table className="w-full">
          <tbody>
            {envVars
              .filter(({ key }) => key !== 'BOUNDARY_PROXY_URL')
              .map((env) => (
                <EnvVarRow
                  key={env.key}
                  env={env}
                  onUpdate={updateEnvVar}
                  onDelete={deleteEnvVar}
                  onToggleVisibility={toggleVisibility}
                />
              ))}
            <tr className="rounded-md">
              <td className="pl-2 pr-0.5 py-0.5">
                <Input
                  value={newKey}
                  onChange={(e) => setNewKey(e.target.value)}
                  placeholder="New environment variable"
                  className="h-6 text-xs font-mono placeholder:font-sans"
                />
              </td>
              <td className="px-0.5 py-0.5">
                <Input
                  value={newValue}
                  onChange={(e) => setNewValue(e.target.value)}
                  placeholder="Value"
                  className="h-6 text-xs font-mono placeholder:font-sans"
                />
              </td>
              <td className="pl-0.5 pr-0.5 py-0.5">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={addEnvVar}
                  className="h-8"
                >
                  <PlusCircle className="mr-2 w-4 h-4" />
                  Add
                </Button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <Dialog>
        <DialogTrigger asChild>
          <Button variant="outline" size="sm" className="w-full mt-2">
            <FileText className="h-4 w-4 mr-2" />
            Import from .env
          </Button>
        </DialogTrigger>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Import from .env file</DialogTitle>
          </DialogHeader>
          <div className="py-4">
            <Label htmlFor="env-file">
              Paste your .env file content below:
            </Label>
            <Textarea
              id="env-file"
              className="min-h-[200px] mt-2 font-mono text-xs"
              placeholder="KEY=value"
              value={envFileContent}
              onChange={(e) => setEnvFileContent(e.target.value)}
            />
          </div>
          <DialogFooter>
            <DialogClose asChild>
              <Button variant="outline">Cancel</Button>
            </DialogClose>
            <DialogClose asChild>
              <Button onClick={parseAndSaveEnvFile}>Import</Button>
            </DialogClose>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
};

export const EnvironmentVariablesDialog: React.FC<{
  showEnvDialog: boolean;
  setShowEnvDialog: (show: boolean) => void;
}> = ({ showEnvDialog, setShowEnvDialog }) => {
  return (
    <Dialog open={showEnvDialog} onOpenChange={setShowEnvDialog}>
      <DialogContent className="mt-12 max-h-[80vh] overflow-y-auto sm:max-w-none w-fit">
        {/* DialogContent requires DialogTitle error  */}
        <DialogTitle className="sr-only">Environment Variables</DialogTitle>
        <EnvironmentVariablesPanel />
      </DialogContent>
    </Dialog>
  );
};
