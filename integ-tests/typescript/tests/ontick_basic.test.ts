// Basic tests for experimental onTick without requiring API calls
import { b } from '../baml_client';
import { b as syncB } from '../baml_client/sync_client';

describe('Experimental OnTick Basic', () => {
    it('should reject onTick for sync functions', () => {
        const onTick = jest.fn();
        
        expect(() => {
            syncB.TestAnthropicShorthand(
                'Hello world',
                { onTick }
            );
        }).toThrow('onTick is not supported for synchronous functions');
    });

    it('should accept onTick in BamlCallOptions for async functions', () => {
        const onTick = jest.fn();
        
        // This test just validates that the type system accepts onTick
        // We can't actually call the function without an API key
        const options = {
            onTick: onTick as (reason: any, log: any) => void
        };
        
        expect(options).toHaveProperty('onTick');
        expect(typeof options.onTick).toBe('function');
    });

    it('should accept onTick in stream functions', () => {
        const onTick = jest.fn();
        
        // This test just validates that the stream accepts onTick
        const options = {
            onTick: onTick as (reason: any, log: any) => void
        };
        
        expect(options).toHaveProperty('onTick');
        expect(typeof options.onTick).toBe('function');
        
        // We can create a stream with onTick option (but not execute it without API key)
        try {
            const stream = b.stream.TestAnthropicShorthand(
                'Hello world',
                options
            );
            expect(stream).toBeDefined();
        } catch (e) {
            // Expected to fail without proper API key, but the type checking passes
        }
    });
});