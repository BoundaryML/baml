import { b, b_sync } from './test-setup';

describe('Workflow Tests', () => {
  it('should run workflows', async () => {
    const workflow = await b.LLMEcho("Hello, world!");
    expect(workflow).toBe("Hello, world!");
  });
});