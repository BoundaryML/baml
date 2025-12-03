import { atom } from 'jotai';
import { atomWithStorage } from 'jotai/utils';
import { vscodeLocalStorageStore } from '../../shared/baml-project-panel/Jotai';
import { vscode } from '../../shared/baml-project-panel/vscode';
import { proxyUrlAtom, runtimeAtom } from '../../shared/baml-project-panel/atoms';

export const apiKeyVisibilityAtom = atom<Record<string, boolean>>({});

export interface ApiKeyEntry {
  key: string;
  value: string | undefined;
  required: boolean;
  hidden: boolean;
}

const hasShownApiKeyDialogAtom = atomWithStorage(
  'has-closed-env-vars-dialog',
  false,
  vscodeLocalStorageStore,
);

const apiKeyDialogOpenAtom = atom(false);

export const showApiKeyDialogAtom = atom(
  (get) => {
    const apiKeyDialogOpen = get(apiKeyDialogOpenAtom)
    if (apiKeyDialogOpen) return true

    const requiredVars = get(requiredApiKeysAtom)
    const envVars = get(apiKeysAtom)

    // Check if ALL required vars are missing
    const hasMissingVars =
      requiredVars.length > 0 && requiredVars.every((key: string) => !envVars[key])

    const hasShownDialog = get(hasShownApiKeyDialogAtom)
    if (hasShownDialog) return apiKeyDialogOpen

    // if we are in vscode, we don't want to show the dialog
    if (!vscode.isVscode()) {
      return false
    }

    return hasMissingVars
  },
  (get, set, value: boolean) => {
    if (!value) {
      set(hasShownApiKeyDialogAtom, true)
    }
    set(apiKeyDialogOpenAtom, value)
  },
)

// --- ENV VAR ATOMS MOVED FROM shared/baml-project-panel/atoms.ts ---

export const resetEnvKeyValuesAtom = atom(null, (get, set) => {
  set(envKeyValueStorage, []);
});

export const envKeyValuesAtom = atom(
  (get) => {
    const envKeyValues = get(envKeyValueStorage);
    return envKeyValues.map(([k, v], idx): [string, string, number] => [
      k,
      v,
      idx,
    ]);
  },
  (
    get,
    set,
    update: // Update value
      | { itemIndex: number; value: string }
      // Update key
      | { itemIndex: number; newKey: string }
      // Remove key
      | { itemIndex: number; remove: true }
      // Insert key
      | {
        itemIndex: null;
        key: string;
        value?: string;
      },
  ) => {
    if (update.itemIndex !== null) {
      const keyValues = [...get(envKeyValueStorage)];
      const targetItem = keyValues[update.itemIndex];
      if (targetItem) {
        if ('value' in update) {
          targetItem[1] = update.value ?? '';
        } else if ('newKey' in update) {
          targetItem[0] = update.newKey;
        } else if ('remove' in update) {
          keyValues.splice(update.itemIndex, 1);
        }
      }
      set(envKeyValueStorage, keyValues);
    } else {
      set(envKeyValueStorage, (prev) => [
        ...prev,
        [update.key, update.value ?? ''],
      ]);
    }
  },
);

export const userApiKeysAtom = atom(
  (get): Record<string, string> => {
    const envKeyValues = get(envKeyValuesAtom);
    const result = Object.fromEntries(
      envKeyValues
        .map(([k, v]) => [k, v])
        .filter(([k]) => k !== 'BOUNDARY_PROXY_URL'),
    );
    return result;
  },
  (get, set, newEnvVars: Record<string, string>) => {

    // Get current envKeyValues to preserve BOUNDARY_PROXY_URL if it exists
    const currentEnvKeyValues = get(envKeyValuesAtom);
    const boundaryProxyEntry = currentEnvKeyValues
      .find(([k]) => k === 'BOUNDARY_PROXY_URL');

    const envKeyValues = Object.entries(newEnvVars);

    // If BOUNDARY_PROXY_URL existed before, preserve it
    if (boundaryProxyEntry) {
      // Only take the key and value, not the index
      envKeyValues.push([boundaryProxyEntry[0], boundaryProxyEntry[1]]);
    }

    set(envKeyValueStorage, envKeyValues);
  },
);

// Computed atom that includes proxy logic (for runtime usage)
export const apiKeysAtom = atom(
  (get) => {
    if (typeof window === 'undefined') {
      return {};
    }

    // Check for Next.js environment
    const isNextJs = !!(window as any).next?.version;

    if (isNextJs) {
      // NextJS environment - check proxy settings but use Next.js specific proxy URL
      const { proxyEnabled } = get(proxyUrlAtom);
      const userEnvVars = get(userApiKeysAtom);

      if (!proxyEnabled) {
        return userEnvVars;
      }

      // Proxy enabled - use Next.js specific proxy URL
      const nextJsProxyUrl = window?.location?.origin?.includes('localhost')
        ? 'https://fiddle-proxy.fly.dev' // localhost development
        : 'https://fiddle-proxy.fly.dev'; // production

      return {
        ...userEnvVars,
        BOUNDARY_PROXY_URL: nextJsProxyUrl,
      };
    }

    const { proxyEnabled, proxyUrl } = get(proxyUrlAtom);
    const userEnvVarsUnescaped = get(userApiKeysAtom);


    // escape env vars that may have \n,\t in them
    // we don't replace \" because its a bit trickier, but if users report bugs, we should fix this.
    const userEnvVars = Object.fromEntries(
      Object.entries(userEnvVarsUnescaped).map(([key, value]) => [key, value.replaceAll('\n', '\\n').replaceAll('\t', '\\t')]),
    );

    if (!proxyEnabled) {
      // if proxy is not enabled, just return user vars without BOUNDARY_PROXY_URL
      console.log('[apiKeysAtom] Proxy disabled, returning user vars without BOUNDARY_PROXY_URL');
      return userEnvVars;
    }

    if (proxyUrl === undefined) {
      console.log('[apiKeysAtom] Proxy URL undefined, returning user vars');
      return userEnvVars;
    }

    // Add or update BOUNDARY_PROXY_URL based on current proxy settings
    console.log('[apiKeysAtom] Adding BOUNDARY_PROXY_URL:', proxyUrl);
    return {
      ...userEnvVars,
      BOUNDARY_PROXY_URL: proxyUrl,
    };
  },
  // Delegate writes to userEnvVarsAtom to avoid interference
  (get, set, newEnvVars: Record<string, string>) => {
    const { BOUNDARY_PROXY_URL, ...userVars } = newEnvVars;
    set(userApiKeysAtom, userVars);
  },
);

export const requiredApiKeysAtom = atom((get) => {
  const { rt, lastValidRt } = get(runtimeAtom);
  const runtime = rt || lastValidRt;

  if (!runtime) {
    return [];
  }

  const requiredEnvVars = runtime.required_env_vars();
  const defaultEnvVars = ['OPENAI_API_KEY', 'ANTHROPIC_API_KEY'];
  for (const e of defaultEnvVars) {
    if (!requiredEnvVars.find((envVar: string) => e === envVar)) {
      requiredEnvVars.push(e);
    }
  }

  return requiredEnvVars;
});

const defaultEnvKeyValues: [string, string][] = (() => {
  if (typeof window === 'undefined') {
    return [];
  }

  const baseDefaults: [string, string][] = [];

  // Add proxy URL defaults based on environment
  if ((window as any).next?.version) {
    const domain = window?.location?.origin || '';
    if (domain.includes('localhost')) {
      // we can do somehting fancier here later if we want to test locally.
      baseDefaults.push(['BOUNDARY_PROXY_URL', 'https://fiddle-proxy.fly.dev']);
    } else {
      baseDefaults.push(['BOUNDARY_PROXY_URL', 'https://fiddle-proxy.fly.dev']);
    }
  } else {
    // VSCode environment
    baseDefaults.push(['BOUNDARY_PROXY_URL', 'http://localhost:0000']);

    // Add placeholder API keys for new VSCode users
    // Check if this is a new user (no existing storage)
    if (typeof window !== 'undefined' && window.localStorage) {
      const storedValue = window.localStorage.getItem('env-key-values');
      if (!storedValue || storedValue === '[]') {
        // New user - add placeholder keys
        baseDefaults.push(['OPENAI_API_KEY', 'PLACEHOLDER_OPENAI_KEY']);
        baseDefaults.push(['ANTHROPIC_API_KEY', 'PLACEHOLDER_ANTHROPIC_KEY']);
        console.debug('New VSCode user detected - adding placeholder API keys');
      }
    }
  }

  console.debug('Default environment values:', baseDefaults);
  return baseDefaults;
})();


// Check what's currently in localStorage
if (typeof window !== 'undefined' && window.localStorage) {
  const storedValue = window.localStorage.getItem('env-key-values');
  if (storedValue) {
    try {
      const parsed = JSON.parse(storedValue);
    } catch (e) {
      console.error('Failed to parse localStorage value:', e);
    }
  }
}

export const envKeyValueStorage = atomWithStorage<[string, string][]>(
  'env-key-values',
  defaultEnvKeyValues,
  vscodeLocalStorageStore,
  {
    getOnInit: true,
  }
);

// Atom for pending (unsaved) API key rows in the add form
export const pendingApiKeyRowsAtom = atom<Array<{ key: string; value: string }>>([
  { key: '', value: '' },
]);

export const areApiKeysMissingAtom = atom((get) => {
  const requiredVars = get(requiredApiKeysAtom)
  const isVscode = vscode.isVscode()
  if (!isVscode) return false
  const envVars = get(apiKeysAtom)
  return requiredVars.length > 0 && requiredVars.some((key: string) => !envVars[key])
})

// Local state atoms for API key management
export const localApiKeysAtom = atom<Record<string, string>>({});
export const hasLocalChangesAtom = atom(false);
export const isSavingAtom = atom(false);
export const recentlyAddedKeysAtom = atom<Set<string>>(new Set<string>());
export const justSavedAtom = atom(false);
export const hasInitializedAtom = atom(false);

// Initialize local API keys from user API keys
export const initializeLocalApiKeysAtom = atom(
  null,
  (get, set) => {
    const userApiKeys = get(userApiKeysAtom);
    set(localApiKeysAtom, userApiKeys);
    set(hasInitializedAtom, true);
  }
);

// Computed atom for rendered API keys
export const renderedApiKeysAtom = atom((get) => {
  const localApiKeys = get(localApiKeysAtom);
  const requiredApiKeys = get(requiredApiKeysAtom);
  const visibility = get(apiKeyVisibilityAtom);
  const recentlyAddedKeys = get(recentlyAddedKeysAtom);

  const vars: ApiKeyEntry[] = Object.entries(localApiKeys).map(
    ([key, value]) => ({
      key,
      value,
      required: requiredApiKeys.includes(key),
      hidden: visibility[key] !== true,
    }),
  );

  const missingVars = requiredApiKeys.filter(
    (apiKey: string) => !(apiKey in localApiKeys),
  );

  vars.push(
    ...missingVars.map((apiKey: string) => ({
      key: apiKey,
      value: undefined,
      required: true,
      hidden: visibility[apiKey] !== true,
    })),
  );

  // Sort with recently added keys at the top, then alphabetically
  const sorted = vars.sort((a, b) => {
    const aIsRecent = recentlyAddedKeys.has(a.key);
    const bIsRecent = recentlyAddedKeys.has(b.key);

    if (aIsRecent && !bIsRecent) return -1;
    if (!aIsRecent && bIsRecent) return 1;

    // If both are recent or both are not, sort alphabetically
    return a.key.localeCompare(b.key);
  });

  return sorted;
});

// Action atoms for API key operations
export const updateApiKeyAtom = atom(
  null,
  (get, set, { key, value }: { key: string; value: string }) => {
    set(localApiKeysAtom, (prev) => ({
      ...prev,
      [key]: value,
    }));
    set(hasLocalChangesAtom, true);
  }
);

export const deleteApiKeyAtom = atom(
  null,
  (get, set, key: string) => {
    set(localApiKeysAtom, (prev: Record<string, string>) => {
      const newVars = { ...prev };
      delete newVars[key];
      return newVars;
    });
    set(hasLocalChangesAtom, true);
    // Remove from recently added keys if it was there
    set(recentlyAddedKeysAtom, (prev: Set<string>) => {
      const newSet = new Set(prev);
      newSet.delete(key);
      return newSet;
    });
  }
);

export const addApiKeyAtom = atom(
  null,
  (get, set, { key, value }: { key: string; value: string }) => {
    set(localApiKeysAtom, (prev: Record<string, string>) => {
      const newState = {
        ...prev,
        [key]: value,
      };
      return newState;
    });
    set(hasLocalChangesAtom, true);
    // Mark this key as recently added
    set(recentlyAddedKeysAtom, (prev: Set<string>) => new Set([...prev, key]));
  }
);

export const importApiKeysAtom = atom(
  null,
  (get, set, vars: Record<string, string>) => {
    const newKeys = Object.keys(vars);
    set(localApiKeysAtom, (prev) => ({
      ...prev,
      ...vars,
    }));
    set(hasLocalChangesAtom, true);
    // Mark imported keys as recently added
    set(recentlyAddedKeysAtom, (prev) => new Set([...prev, ...newKeys]));

    const localApiKeys = get(localApiKeysAtom);
    // Set this otherwise changes won't take effect.
    set(userApiKeysAtom, localApiKeys);
  }
);

export const saveApiKeyChangesAtom = atom(
  null,
  async (get, set) => {
    const localApiKeys = get(localApiKeysAtom);
    set(isSavingAtom, true);

    // Small delay to ensure UI updates immediately
    await new Promise(resolve => setTimeout(resolve, 0));

    set(userApiKeysAtom, localApiKeys);

    set(hasLocalChangesAtom, false);
    set(justSavedAtom, true); // Set flag to prevent immediate re-initialization
    // Clear recently added keys after saving
    set(recentlyAddedKeysAtom, new Set());

    // Reset saving state after a small delay to show completion
    setTimeout(() => {
      set(isSavingAtom, false);
    }, 200);
  }
);

// Sync atom to handle synchronization between local and user API keys
export const syncLocalApiKeysAtom = atom(
  null,
  (get, set) => {
    const hasLocalChanges = get(hasLocalChangesAtom);
    const justSaved = get(justSavedAtom);
    const hasInitialized = get(hasInitializedAtom);
    const userApiKeys = get(userApiKeysAtom);

    // Always initialize on first render
    if (!hasInitialized) {
      set(localApiKeysAtom, userApiKeys);
      set(hasInitializedAtom, true);
      return;
    }

    // Sync when no local changes and not just saved
    if (!hasLocalChanges && !justSaved) {
      set(localApiKeysAtom, userApiKeys);
    } else if (justSaved) {
      // Reset the flag after the effect runs once
      set(justSavedAtom, false);
    }
  }
);

