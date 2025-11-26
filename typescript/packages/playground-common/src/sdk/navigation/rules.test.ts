/**
 * Navigation Rules Tests
 */

import { describe, it, expect } from 'vitest';
import { RuleEngine } from './rule-engine';
import { NAVIGATION_RULES } from './rules';
import type { EnrichedTarget } from './types';
import type { SelectionState } from '../atoms/core.atoms';

describe('Navigation Rules', () => {
  const ruleEngine = new RuleEngine(NAVIGATION_RULES);

  describe('direct-node-click rule', () => {
    it('should preserve testName when clicking the same workflow node twice', () => {
      // Setup: User is in a workflow with a test selected (NOT the first one in the list)
      const currentState: SelectionState = {
        mode: 'workflow',
        workflowId: 'SimpleWorkflow',
        selectedNodeId: 'SimpleWorkflow|root:0',
        functionName: 'SimpleWorkflow',
        testName: 'test_simple_failure', // <-- Second test is selected
      };

      // Action: User clicks on the same root node again (double-click scenario)
      const target: EnrichedTarget = {
        name: 'SimpleWorkflow',
        kind: 'node',
        exists: true,
        workflowMemberships: [{
          workflowId: 'SimpleWorkflow',
          nodeId: 'SimpleWorkflow|root:0',
          nodeLabel: 'SimpleWorkflow',
          calledFunctions: ['SimpleWorkflow'],
        }],
        availableTests: ['test_simple_success', 'test_simple_failure'],
        workflowId: 'SimpleWorkflow',
        nodeId: 'SimpleWorkflow|root:0',
      };

      const result = ruleEngine.decide(target, currentState);

      // The testName should be preserved, not reset to the first available test
      expect(result.rule).toBe('direct-node-click');
      expect(result.state.mode).toBe('workflow');
      if (result.state.mode === 'workflow') {
        expect(result.state.testName).toBe('test_simple_failure');
      }
    });

    it('should preserve testName when clicking a different node in the same workflow', () => {
      // Setup: User is in a workflow with a test selected (NOT the first in the list)
      const currentState: SelectionState = {
        mode: 'workflow',
        workflowId: 'SimpleWorkflow',
        selectedNodeId: 'SimpleWorkflow|root:0',
        functionName: 'SimpleWorkflow',
        testName: 'test_simple_failure', // <-- Second test is selected
      };

      // Action: User clicks a different node in the same workflow
      const target: EnrichedTarget = {
        name: 'ProcessStep',
        kind: 'node',
        exists: true,
        workflowMemberships: [{
          workflowId: 'SimpleWorkflow',
          nodeId: 'SimpleWorkflow|step1:0',
          nodeLabel: 'ProcessStep',
          calledFunctions: ['ProcessStep'],
        }],
        availableTests: ['test_simple_success', 'test_simple_failure'],
        workflowId: 'SimpleWorkflow',
        nodeId: 'SimpleWorkflow|step1:0',
      };

      const result = ruleEngine.decide(target, currentState);

      expect(result.rule).toBe('direct-node-click');
      expect(result.state.mode).toBe('workflow');
      if (result.state.mode === 'workflow') {
        expect(result.state.testName).toBe('test_simple_failure');
      }
    });

    it('should select first available test when no test is currently selected', () => {
      // Setup: User is in a workflow with no test selected
      const currentState: SelectionState = {
        mode: 'workflow',
        workflowId: 'SimpleWorkflow',
        selectedNodeId: 'SimpleWorkflow|root:0',
        functionName: 'SimpleWorkflow',
        testName: null,
      };

      // Action: User clicks a node
      const target: EnrichedTarget = {
        name: 'SimpleWorkflow',
        kind: 'node',
        exists: true,
        workflowMemberships: [{
          workflowId: 'SimpleWorkflow',
          nodeId: 'SimpleWorkflow|root:0',
          nodeLabel: 'SimpleWorkflow',
          calledFunctions: ['SimpleWorkflow'],
        }],
        availableTests: ['test_simple_success', 'test_simple_failure'],
        workflowId: 'SimpleWorkflow',
        nodeId: 'SimpleWorkflow|root:0',
      };

      const result = ruleEngine.decide(target, currentState);

      expect(result.rule).toBe('direct-node-click');
      expect(result.state.mode).toBe('workflow');
      if (result.state.mode === 'workflow') {
        expect(result.state.testName).toBe('test_simple_success');
      }
    });

    it('should select first test when current test is not in available tests', () => {
      // Setup: User is in a workflow with a test that doesn't exist for this node
      const currentState: SelectionState = {
        mode: 'workflow',
        workflowId: 'SimpleWorkflow',
        selectedNodeId: 'SimpleWorkflow|root:0',
        functionName: 'SimpleWorkflow',
        testName: 'nonexistent_test',
      };

      // Action: User clicks a node with different available tests
      const target: EnrichedTarget = {
        name: 'SimpleWorkflow',
        kind: 'node',
        exists: true,
        workflowMemberships: [{
          workflowId: 'SimpleWorkflow',
          nodeId: 'SimpleWorkflow|root:0',
          nodeLabel: 'SimpleWorkflow',
          calledFunctions: ['SimpleWorkflow'],
        }],
        availableTests: ['test_simple_success', 'test_simple_failure'],
        workflowId: 'SimpleWorkflow',
        nodeId: 'SimpleWorkflow|root:0',
      };

      const result = ruleEngine.decide(target, currentState);

      expect(result.rule).toBe('direct-node-click');
      expect(result.state.mode).toBe('workflow');
      if (result.state.mode === 'workflow') {
        // Should fall back to first available test
        expect(result.state.testName).toBe('test_simple_success');
      }
    });
  });
});
