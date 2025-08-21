import { atom } from 'jotai';
import { atomWithStorage } from 'jotai/utils';
import { vscodeSettingsAtom } from './atoms';

// Local feature flags atom for standalone playground or temporary overrides
export const localFeatureFlagsAtom = atomWithStorage<string[]>('baml-feature-flags', []);

// Combined feature flags atom that considers both VSCode settings and local overrides
export const effectiveFeatureFlagsAtom = atom(
  (get) => {
    const localFlags = get(localFeatureFlagsAtom);
    try {
      const vscodeSettings = get(vscodeSettingsAtom);
      const isInVSCode = !!vscodeSettings;
      
      let result;
      if (isInVSCode) {
        // In VSCode: start with VSCode settings as default, allow local overrides
        const vscodeFlags = vscodeSettings.featureFlags || [];
        // Merge VSCode flags with local overrides
        const mergedFlags = [...new Set([...vscodeFlags, ...localFlags])];
        result = mergedFlags;
      } else {
        // Standalone fiddle: local flags are the source of truth
        result = localFlags;
      }
      return result;
    } catch (e) {
      // If VSCode settings fail to load, use local flags
      return localFlags;
    }
  },
  (get, set, newFlags: string[]) => {
    set(localFeatureFlagsAtom, newFlags);
  }
);

// Beta feature flag convenience atom
export const betaFeatureEnabledAtom = atom(
  (get) => get(effectiveFeatureFlagsAtom).includes('beta'),
  (get, set, enabled: boolean) => {
    const currentFlags = get(effectiveFeatureFlagsAtom);
    const updatedFlags = enabled 
      ? [...currentFlags.filter(flag => flag !== 'beta'), 'beta']
      : currentFlags.filter(flag => flag !== 'beta');
    set(effectiveFeatureFlagsAtom, updatedFlags);
  }
);

// Check if we're in a VSCode environment
export const isVSCodeEnvironment = () => {
  if (typeof window === 'undefined') return false;
  return 'acquireVsCodeApi' in window;
};