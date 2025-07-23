import { b } from "./test-setup";
import { AbortError } from "@boundaryml/baml";

describe("AbortController", () => {
  // Helper function to wait for a specified time
  const wait = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

  describe("Direct Abort Method", () => {
    it("should abort streaming using stream.abort()", async () => {
      // Start a streaming operation
      const stream = b.stream.TestOpenAIGPT4oMini("Count from 1 to 100 slowly");
      
      const results: string[] = [];
      let abortCalled = false;
      
      // Set a timeout to abort after collecting some results
      setTimeout(() => {
        abortCalled = true;
        stream.abort();
      }, 1000);
      
      try {
        // Process stream until aborted
        for await (const chunk of stream) {
          results.push(chunk);
          
          // If we've collected enough results, wait for the abort to happen
          if (results.length >= 5) {
            await wait(1500); // Wait longer than the abort timeout
          }
        }
        
        // This should not be reached if aborted
        fail("Expected AbortError was not thrown");
      } catch (error) {
        // Verify we got an AbortError
        expect(error).toBeInstanceOf(AbortError);
        expect(error.name).toBe("AbortError");
        expect(abortCalled).toBe(true);
      }
      
      // Verify we collected some results before aborting
      expect(results.length).toBeGreaterThan(0);
      
      // Verify the stream is marked as aborted
      expect(stream.isAborted).toBe(true);
    });
    
    it("should throw AbortError when calling getFinalResponse after abort", async () => {
      const stream = b.stream.TestOpenAIGPT4oMini("Generate a short story");
      
      // Abort the stream
      stream.abort();
      
      // Attempt to get final response should throw
      await expect(stream.getFinalResponse()).rejects.toThrow(AbortError);
    });
  });
  
  describe("External AbortController", () => {
    it("should abort streaming using external AbortController", async () => {
      // Create an AbortController
      const controller = new AbortController();
      
      // Start a streaming operation with the signal
      const stream = b.stream.TestOpenAIGPT4oMini("Count from 1 to 100 slowly", { 
        signal: controller.signal 
      });
      
      const results: string[] = [];
      
      // Set a timeout to abort after collecting some results
      setTimeout(() => {
        controller.abort();
      }, 1000);
      
      try {
        // Process stream until aborted
        for await (const chunk of stream) {
          results.push(chunk);
          
          // If we've collected enough results, wait for the abort to happen
          if (results.length >= 5) {
            await wait(1500); // Wait longer than the abort timeout
          }
        }
        
        // This should not be reached if aborted
        fail("Expected AbortError was not thrown");
      } catch (error) {
        // Verify we got an AbortError
        expect(error).toBeInstanceOf(AbortError);
        expect(error.name).toBe("AbortError");
      }
      
      // Verify we collected some results before aborting
      expect(results.length).toBeGreaterThan(0);
      
      // Verify the stream is marked as aborted
      expect(stream.isAborted).toBe(true);
    });
    
    it("should abort immediately if signal is already aborted", async () => {
      // Create an AbortController and abort it immediately
      const controller = new AbortController();
      controller.abort();
      
      // Start a streaming operation with the already aborted signal
      const stream = b.stream.TestOpenAIGPT4oMini("Generate a short story", { 
        signal: controller.signal 
      });
      
      // Verify the stream is marked as aborted immediately
      expect(stream.isAborted).toBe(true);
      
      // Attempt to iterate should throw
      await expect(async () => {
        for await (const chunk of stream) {
          // This should not be reached
          fail("Expected AbortError was not thrown");
        }
      }).rejects.toThrow(AbortError);
      
      // Attempt to get final response should throw
      await expect(stream.getFinalResponse()).rejects.toThrow(AbortError);
    });
  });
  
  describe("Stream Signal", () => {
    it("should expose AbortSignal via stream.signal", async () => {
      const stream = b.stream.TestOpenAIGPT4oMini("Generate a short story");
      
      // Verify signal is available
      expect(stream.signal).toBeDefined();
      expect(stream.signal.aborted).toBe(false);
      
      // Set up a listener
      let signalAborted = false;
      stream.signal.addEventListener("abort", () => {
        signalAborted = true;
      }, { once: true });
      
      // Abort the stream
      stream.abort();
      
      // Verify signal was aborted
      expect(stream.signal.aborted).toBe(true);
      expect(signalAborted).toBe(true);
    });
  });
  
  describe("ReadableStream Integration", () => {
    it("should abort the ReadableStream when stream is aborted", async () => {
      const stream = b.stream.TestOpenAIGPT4oMini("Generate a short story");
      
      // Convert to ReadableStream
      const readableStream = stream.toStreamable();
      
      // Set up reader
      const reader = readableStream.getReader();
      
      // Read first chunk
      const { value: firstChunk } = await reader.read();
      expect(firstChunk).toBeDefined();
      
      // Abort the stream
      stream.abort();
      
      // Next read should complete with done=true
      const { done } = await reader.read();
      expect(done).toBe(true);
    });
  });
});
