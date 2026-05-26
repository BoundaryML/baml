// test_collector.test.ts — mirrors bridge_python/tests/test_collector.py

import { Collector } from '../index';

describe('Collector', () => {
    test('constructor with default name', () => {
        const c = new Collector();
        expect(c.name).toBe('default');
    });

    test('constructor with custom name', () => {
        const c = new Collector('my-collector');
        expect(c.name).toBe('my-collector');
    });

    test('logs starts empty', () => {
        const c = new Collector();
        expect(c.logs).toEqual([]);
    });

    test('last returns null when empty', () => {
        const c = new Collector();
        expect(c.last).toBeNull();
    });

    test('clear returns 0 when empty', () => {
        const c = new Collector();
        expect(c.clear()).toBe(0);
    });
});
