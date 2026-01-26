import { beforeAll, describe, expect, it } from 'vitest';

import { BamlRuntime } from '../runtime/BamlRuntime';
import { DEBUG_BAML_FILES } from '../debugFixtures';

let runtime: BamlRuntime;

beforeAll(async () => {
  const { runtime: realRuntime } = await BamlRuntime.create(
    DEBUG_BAML_FILES,
    {},
    []
  );
  runtime = realRuntime;
});

describe('WASM graph generation', () => {
  it('produces workflow structure for SimpleWorkflow', () => {
    const functions = runtime.getFunctions();
    const simpleWorkflow = functions.find((fn) => fn.name === 'SimpleWorkflow');

    expect(simpleWorkflow).toBeTruthy();
    expect(simpleWorkflow?.type).toBe('workflow');

    const idByLogFilter = new Map(
      (simpleWorkflow?.nodes ?? []).map((node) => [node.metadata?.logFilterKey, node.id])
    );
    // IDs are numeric strings, logFilterKey kept as metadata
    expect([...idByLogFilter.values()].every((id) => /^\d+$/.test(id ?? ''))).toBe(true);
    expect(idByLogFilter.get('SimpleWorkflow|root:0')).toBeDefined();
    expect(idByLogFilter.get('SimpleWorkflow|root:0|hdr:gather-applicant-context:0')).toBeDefined();
    expect(idByLogFilter.get('SimpleWorkflow|root:0|hdr:normalize-profile-signals:1')).toBeDefined();

    const edgeTargets = new Set(simpleWorkflow?.edges.map((edge) => edge.target));
    const normalizeId = idByLogFilter.get('SimpleWorkflow|root:0|hdr:normalize-profile-signals:1');
    const persistId = idByLogFilter.get('SimpleWorkflow|root:0|hdr:persist-summarized-profile:2');
    expect(normalizeId && edgeTargets.has(normalizeId)).toBe(true);
    expect(persistId && edgeTargets.has(persistId)).toBe(true);
  });

  it('produces branch structure for ConditionalWorkflow', () => {
    const functions = runtime.getFunctions();
    const conditionalWorkflow = functions.find((fn) => fn.name === 'ConditionalWorkflow');

    expect(conditionalWorkflow).toBeTruthy();
    expect(conditionalWorkflow?.type).toBe('workflow');

    const idByLogFilter = new Map(
      (conditionalWorkflow?.nodes ?? []).map((node) => [node.metadata?.logFilterKey, node.id])
    );
    expect(idByLogFilter.get('ConditionalWorkflow|root:0')).toBeDefined();
    const checkSummaryId = idByLogFilter.get('ConditionalWorkflow|root:0|hdr:check-summary-confidence:1');
    const branchGroupId = idByLogFilter.get(
      'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1|bg:if-checkcondition-validation-summary:0'
    );
    expect(checkSummaryId).toBeDefined();
    expect(branchGroupId).toBeDefined();

    const branchChildren = conditionalWorkflow?.nodes.filter(
      (node) => node.parent === checkSummaryId
    );
    expect(branchChildren && branchChildren.length).toBeGreaterThan(0);
  });
});
