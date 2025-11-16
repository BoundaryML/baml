/**
 * Navigation System Entry Point
 *
 * Exports all navigation-related types and functions
 */

// Export types
export type {
  NavigationInput,
  NavigationSource,
  EnrichedTarget,
  WorkflowMembership,
  NavigationRule,
  SideEffect,
  NavigationContext,
  NavigationLogEntry,
} from './types';

// Export coordinator (for advanced usage)
export { NavigationCoordinator, createNavigationCoordinator } from './coordinator';

// Export logger (for debugging)
export { navLogger, NavigationLogger } from './logger';

// Export rules (for customization)
export { NAVIGATION_RULES } from './rules';
