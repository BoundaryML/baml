/**
 * Navigation System Type Definitions
 *
 * Based on NAVIGATION_DESIGN_V2.md
 */

import type { SelectionState } from '../atoms/core.atoms';
import type { FunctionWithCallGraph } from '../interface';
import type { BAMLTest, BAMLFile } from '../types';

// ============================================================================
// Navigation Input (what the user clicked)
// ============================================================================

export type NavigationSource =
  | 'cursor'
  | 'debug-panel'
  | 'graph'
  | 'test-panel'
  | 'toolbar'
  | 'sidebar'
  | 'api';

export type NavigationInput = {
  kind: 'function' | 'test' | 'node';
  source: NavigationSource;
  timestamp: number;

  // Function click
  functionName?: string;
  functionType?: string;

  // Test click
  testName?: string;

  // Node click (has workflow context)
  workflowId?: string;
  nodeId?: string;

  // Cursor position (for IDE integration)
  cursorPosition?: {
    filePath: string;
    line: number;
    column: number;
  };
};

// ============================================================================
// Enriched Target (input + context)
// ============================================================================

export interface WorkflowMembership {
  workflowId: string;
  nodeId: string;
  nodeLabel: string;
  calledFunctions: string[]; // Functions called by this node (can be multiple)
}

export interface EnrichedTarget {
  // What was clicked
  name: string;
  kind: 'function' | 'test' | 'node';
  exists: boolean;

  // Where is it used?
  workflowMemberships: WorkflowMembership[];

  // Related data
  availableTests: string[];
  functionType?: string;

  // Additional metadata
  functionName?: string;
  testName?: string;
  workflowId?: string;
  nodeId?: string;
}

// ============================================================================
// Navigation Rules
// ============================================================================

export interface NavigationRule {
  id: string;
  priority: number; // Lower = higher priority

  // When does this rule apply?
  matches: (target: EnrichedTarget, current: SelectionState) => boolean;

  // What should we do?
  resolve: (target: EnrichedTarget, current: SelectionState, context?: NavigationContext) => SelectionState;

  // Optional: Why did this rule match? (for debugging)
  explain?: (target: EnrichedTarget, current: SelectionState) => string;
}

// ============================================================================
// Side Effects
// ============================================================================

export type SideEffect =
  | { type: 'switch-tab'; tab: 'preview' | 'curl' | 'graph' }
  | { type: 'pan-to-node'; workflowId: string; nodeId: string }
  | { type: 'open-panel' }
  | { type: 'close-panel' }
  | { type: 'select-test'; testName: string }
  | { type: 'clear-test' }
  | { type: 'jump-to-file'; span: { filePath: string; startLine: number; startColumn: number } };

// ============================================================================
// Navigation Context (for enrichment)
// ============================================================================

export interface NavigationContext {
  workflows: FunctionWithCallGraph[];
  functions: FunctionWithCallGraph[];
  bamlFiles: BAMLFile[];
  tests: BAMLTest[];
}

// ============================================================================
// Navigation Log Entry
// ============================================================================

export interface NavigationLogEntry {
  input: NavigationInput;
  target: EnrichedTarget;
  from: SelectionState;
  to: SelectionState;
  rule: string;
  effects: SideEffect[];
  duration: number;
  timestamp: number;
}

// ============================================================================
// Jotai Types
// ============================================================================

import type { Getter, Setter } from 'jotai';

export type JotaiGet = Getter;
export type JotaiSet = Setter;
