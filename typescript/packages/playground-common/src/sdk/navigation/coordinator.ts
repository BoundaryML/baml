/**
 * Navigation Coordinator
 *
 * Orchestrates the entire navigation flow:
 * 1. Enrich target with context
 * 2. Apply rules to decide where to go
 * 3. Build transaction
 * 4. Apply state changes
 * 5. Log the result
 */

import type { SelectionState } from '../atoms/core.atoms';
import type { NavigationInput, JotaiGet, JotaiSet } from './types';
import { TargetEnricher } from './target-enricher';
import { RuleEngine } from './rule-engine';
import { StateManager } from './state-manager';
import { NavigationLogger } from './logger';
import { NAVIGATION_RULES } from './rules';
import { unifiedSelectionStateAtom } from '../atoms/core.atoms';

export class NavigationCoordinator {
  constructor(
    private enricher: TargetEnricher,
    private engine: RuleEngine,
    private stateManager: StateManager,
    private logger: NavigationLogger
  ) { }

  /**
   * Main entry point for navigation
   *
   * This is the only public method - all navigation goes through here
   */
  async navigate(
    input: NavigationInput,
    atomGet: JotaiGet,
    atomSet: JotaiSet
  ): Promise<void> {
    const startTime = performance.now();

    try {
      // 1. Enrich target with context
      const target = this.enricher.enrich(input);

      // 2. Get current state
      const current = atomGet(unifiedSelectionStateAtom);
      // 3. Decide where to go (pass context so rules can look up workflows)
      const context = this.enricher.getContext();
      const { state: targetState, rule } = this.engine.decide(target, current, context);
      console.log('[NavigationCoordinator] targetState:', targetState);
      console.log('[NavigationCoordinator] rule:', rule);
      // 4. Build transaction (pass input and context for side effects)
      const effects = this.stateManager.buildTransaction(
        targetState,
        current,
        input,
        this.enricher.getContext()
      );

      // 5. Apply transaction
      await this.stateManager.apply(targetState, effects, atomSet);

      // 6. Log
      this.logger.log({
        input,
        target,
        from: current,
        to: targetState,
        rule,
        effects,
        duration: performance.now() - startTime,
        timestamp: Date.now(),
      });
    } catch (error) {
      this.logger.error(input, error as Error);
      throw error;
    }
  }

  /**
   * Update the enricher's context (when workflows/functions change)
   */
  updateContext(context: Parameters<TargetEnricher['updateContext']>[0]): void {
    this.enricher.updateContext(context);
  }
}

/**
 * Factory function to create a navigation coordinator
 */
export function createNavigationCoordinator(
  context: ConstructorParameters<typeof TargetEnricher>[0]
): NavigationCoordinator {
  return new NavigationCoordinator(
    new TargetEnricher(context),
    new RuleEngine(NAVIGATION_RULES),
    new StateManager(),
    new NavigationLogger()
  );
}
