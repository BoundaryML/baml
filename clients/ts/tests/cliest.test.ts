import { trace, initTracer, shutdownTracer } from "baml-client-lib";

initTracer();

const fx = trace((a: number, b: number) => a + b, 'add', [{ name: 'a', type: 'number' }, { name: 'b', type: 'number' }], false, 'number');

describe('Client', () => {
  it('should add', () => {
    expect(fx(1, 2)).toBe(3);
  });
})