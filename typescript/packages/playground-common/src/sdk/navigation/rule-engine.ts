/**
 * Rule Engine
 *
 * Applies navigation rules to determine target state
 */

import type { SelectionState } from '../atoms/core.atoms';
import type { NavigationRule, EnrichedTarget, NavigationContext } from './types';

export class NavigationError extends Error {
  constructor(
    message: string,
    public context: { target: EnrichedTarget; current: SelectionState }
  ) {
    super(message);
    this.name = 'NavigationError';
  }
}

export class RuleEngine {
  constructor(private rules: NavigationRule[]) {}

  /**
   * Determine target state by applying rules
   *
   * Returns the new selection state and the rule that was applied
   */
  decide(
    target: EnrichedTarget,
    current: SelectionState,
    context?: NavigationContext
  ): { state: SelectionState; rule: string } {
    // Sort by priority (ascending - lower number = higher priority)
    const sorted = [...this.rules].sort((a, b) => a.priority - b.priority);

    // Find first matching rule
    for (const rule of sorted) {
      if (rule.matches(target, current)) {
        const state = rule.resolve(target, current, context);

        return {
          state,
          rule: rule.id,
        };
      }
    }

    throw new NavigationError('No rule matched', { target, current });
  }

  /**
   * Get explanation for why a rule matched
   */
  explain(target: EnrichedTarget, current: SelectionState): string {
    const sorted = [...this.rules].sort((a, b) => a.priority - b.priority);

    for (const rule of sorted) {
      if (rule.matches(target, current)) {
        return rule.explain?.(target, current) || `Matched rule: ${rule.id}`;
      }
    }

    return 'No rule matched';
  }
}
