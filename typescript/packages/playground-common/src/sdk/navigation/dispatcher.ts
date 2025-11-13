/**
 * Navigation Dispatcher Atom
 *
 * This is the main entry point for triggering navigation from components.
 * All navigation events should go through this dispatcher.
 */

import { atom } from 'jotai';
import type { NavigationInput, NavigationContext } from './types';
import { createNavigationCoordinator } from './coordinator';
import {
  runtimeInstanceAtom,
  workflowsAtom,
  bamlFilesAtom,
} from '../atoms/core.atoms';
import type { FunctionWithCallGraph } from '../interface';
import type { BAMLFile, BAMLTest } from '../types';

/**
 * Lazily create the coordinator instance
 */
let coordinatorInstance: ReturnType<typeof createNavigationCoordinator> | null = null;

function getCoordinator(get: any) {
  const runtime = get(runtimeInstanceAtom);
  const workflows = get(workflowsAtom) as FunctionWithCallGraph[];
  const bamlFiles = get(bamlFilesAtom) as BAMLFile[] | null;

  const functions = runtime?.getFunctions() || [];
  const tests: BAMLTest[] =
    bamlFiles?.flatMap((file) => file.tests || []) || [];

  const context: NavigationContext = {
    workflows,
    functions,
    bamlFiles: bamlFiles || [],
    tests,
  };

  // If we already have an instance, update its context and return it
  if (coordinatorInstance) {
    coordinatorInstance.updateContext(context);
    return coordinatorInstance;
  }

  // Create new coordinator
  coordinatorInstance = createNavigationCoordinator(context);

  return coordinatorInstance;
}

/**
 * Navigation dispatcher atom
 *
 * Usage:
 * ```tsx
 * const dispatchNavigation = useSetAtom(navigationDispatcherAtom);
 *
 * dispatchNavigation({
 *   kind: 'function',
 *   functionName: 'MyFunction',
 *   source: 'debug-panel',
 *   timestamp: Date.now(),
 * });
 * ```
 */
export const navigationDispatcherAtom = atom(
  null,
  async (get, set, input: NavigationInput) => {
    const coordinator = getCoordinator(get);
    await coordinator.navigate(input, get, set);
  }
);
