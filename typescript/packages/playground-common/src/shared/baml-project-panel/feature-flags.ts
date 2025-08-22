import { atom } from 'jotai';
import { atomWithStorage } from 'jotai/utils';

// Feature flags atom for standalone playground
export const standaloneFeatureFlagsAtom = atomWithStorage<string[]>('baml-feature-flags', []);

// Beta feature flag convenience atom for standalone use
export const betaFeatureEnabledAtom = atom(
  (get) => get(standaloneFeatureFlagsAtom).includes('beta'),
  (get, set, enabled: boolean) => {
    const currentFlags = get(standaloneFeatureFlagsAtom);
    const updatedFlags = enabled 
      ? [...currentFlags.filter(flag => flag !== 'beta'), 'beta']
      : currentFlags.filter(flag => flag !== 'beta');
    set(standaloneFeatureFlagsAtom, updatedFlags);
  }
);

// Check if we're in a VSCode environment
export const isVSCodeEnvironment = () => {
  if (typeof window === 'undefined') return false;
  return 'acquireVsCodeApi' in window;
};