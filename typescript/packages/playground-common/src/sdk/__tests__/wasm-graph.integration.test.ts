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

    const nodeIds = new Set(simpleWorkflow?.nodes.map((node) => node.id));
    // Root node should have lexical_id format with |root:0
    expect(nodeIds.has('SimpleWorkflow|root:0')).toBe(true);
    expect(
      nodeIds.has('SimpleWorkflow|root:0|hdr:gather-applicant-context:0')
    ).toBe(true);
    expect(
      nodeIds.has('SimpleWorkflow|root:0|hdr:normalize-profile-signals:1')
    ).toBe(true);

    const edgeTargets = new Set(simpleWorkflow?.edges.map((edge) => edge.target));
    expect(
      edgeTargets.has('SimpleWorkflow|root:0|hdr:normalize-profile-signals:1')
    ).toBe(true);
    expect(
      edgeTargets.has('SimpleWorkflow|root:0|hdr:persist-summarized-profile:2')
    ).toBe(true);
  });

  it('produces branch structure for ConditionalWorkflow', () => {
    const functions = runtime.getFunctions();
    const conditionalWorkflow = functions.find((fn) => fn.name === 'ConditionalWorkflow');

    expect(conditionalWorkflow).toBeTruthy();
    expect(conditionalWorkflow?.type).toBe('workflow');

    const nodeIds = new Set(conditionalWorkflow?.nodes.map((node) => node.id));
    // Root node should have lexical_id format with |root:0
    expect(nodeIds.has('ConditionalWorkflow|root:0')).toBe(true);
    expect(
      nodeIds.has('ConditionalWorkflow|root:0|hdr:check-summary-confidence:1')
    ).toBe(true);
    expect(
      nodeIds.has(
        'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1|bg:if-checkcondition-validation-summary:0'
      )
    ).toBe(true);

    const branchChildren = conditionalWorkflow?.nodes.filter(
      (node) =>
        node.parent === 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1'
    );
    expect(branchChildren && branchChildren.length).toBeGreaterThan(0);
  });
});
