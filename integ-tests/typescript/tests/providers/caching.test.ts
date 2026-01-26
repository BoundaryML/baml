import { b } from "../test-setup";
import { Collector } from "@boundaryml/baml";

describe("Provider Caching Tests", () => {


  it("should handle OpenAI streaming with cached token tracking", async () => {
    const collector = new Collector("openai-streaming-caching");

    const stream = b.stream.TestOpenAIGPT4oMini("Stream with caching support", { collector });
    
    const chunks = [];
    for await (const chunk of stream) {
      chunks.push(chunk);
    }

    const result = await stream.getFinalResponse();
    expect(result.length).toBeGreaterThan(0);

    const logs = collector.logs;
    expect(logs.length).toBe(1);

    const log = logs[0];
    expect(log.logType).toBe("stream");

    // Verify cached tokens are tracked for streaming
    expect(log.usage.cachedInputTokens).toBeDefined();
    expect(log.calls[0].usage?.cachedInputTokens).toBeDefined();

    console.log("OpenAI streaming cached tokens:", log.usage.cachedInputTokens);
  });

  it("should aggregate cached tokens across multiple OpenAI calls", async () => {
    const collector = new Collector("openai-aggregation");

    // Make multiple calls
    await b.TestOpenAIGPT4oMini("First OpenAI call", { collector });
    await b.TestOpenAIGPT4oMini("Second OpenAI call", { collector });

    const logs = collector.logs;
    expect(logs.length).toBe(2);

    // Calculate expected total cached tokens
    const totalCachedTokens = 
      (logs[0].usage.cachedInputTokens || 0) +
      (logs[1].usage.cachedInputTokens || 0);

    // Verify collector aggregates cached tokens correctly
    expect(collector.usage.cachedInputTokens).toBe(totalCachedTokens);

    console.log("Total OpenAI cached tokens:", collector.usage.cachedInputTokens);
  });

  it("should verify OpenAI caching with streaming", async () => {
    const collector = new Collector("openai-large-content-caching");

    // Create substantial content (2048+ tokens) to ensure caching has opportunity to trigger
    const largeContent = `
    The comprehensive analysis of artificial intelligence systems requires deep understanding of multiple domains including machine learning algorithms, neural network architectures, data preprocessing techniques, model optimization strategies, performance evaluation metrics, ethical considerations, and deployment challenges. Modern AI applications span across various industries from healthcare and finance to autonomous vehicles and natural language processing systems.

    In healthcare, AI technologies are revolutionizing medical diagnosis through computer vision systems that can analyze medical imagery with unprecedented accuracy. These systems utilize convolutional neural networks trained on massive datasets of X-rays, CT scans, MRIs, and other medical images to detect patterns that might be missed by human practitioners. The integration of electronic health records with predictive analytics enables early intervention strategies and personalized treatment recommendations.

    The financial sector has embraced AI for fraud detection, algorithmic trading, credit risk assessment, and customer service automation. Machine learning models analyze transaction patterns in real-time to identify suspicious activities, while natural language processing systems handle customer inquiries through chatbots and virtual assistants. Robo-advisors use sophisticated algorithms to provide investment recommendations based on individual risk profiles and market conditions.

    Autonomous vehicles represent one of the most complex AI applications, requiring the integration of computer vision, sensor fusion, path planning algorithms, and real-time decision-making systems. These vehicles must navigate dynamic environments while ensuring passenger safety and complying with traffic regulations. The development of self-driving cars involves extensive simulation testing and validation in controlled environments before deployment on public roads.
    `.repeat(5);

    // First call - might establish patterns
    const stream = b.stream.TestOpenAIGPT4oMini(largeContent, { collector });

    const chunks = [];
    for await (const chunk of stream) {
      chunks.push(chunk);
    }

    const result = await stream.getFinalResponse();
    expect(result.length).toBeGreaterThan(0);

    // Second call with identical content - should potentially use caching if supported
    const stream2 = b.stream.TestOpenAIGPT4oMini(largeContent, { collector });

    const chunks2 = [];
    for await (const chunk of stream2) {
      chunks2.push(chunk);
    }

    const result2 = await stream2.getFinalResponse();
    expect(result2.length).toBeGreaterThan(0);
    
    // Third call to increase chances of seeing cached behavior
    const stream3 = b.stream.TestOpenAIGPT4oMini(largeContent, { collector });

    const chunks3 = [];
    for await (const chunk of stream3) {
      chunks3.push(chunk);
    }

    const result3 = await stream3.getFinalResponse();
    expect(result3.length).toBeGreaterThan(0);

    const logs = collector.logs;
    expect(logs.length).toBe(3);

    // Verify all calls have cached tokens fields defined
    logs.forEach((log, index) => {
      // expect(log.usage.cachedInputTokens).toBeDefined();
      // expect(log.calls[0].usage?.cachedInputTokens).toBeDefined();
      console.log(`OpenAI large content call ${index + 1} cached tokens:`, log.usage.cachedInputTokens);
    });

    // Calculate total cached tokens
    const totalCachedTokens = logs.reduce((sum, log) => sum + (log.usage.cachedInputTokens || 0), 0);
    expect(collector.usage.cachedInputTokens).toBe(totalCachedTokens);
    expect(collector.usage.cachedInputTokens).toBeGreaterThan(0);

    console.log("Large content length:", largeContent.length, "characters");
    console.log("Total OpenAI cached tokens from repeated calls:", collector.usage.cachedInputTokens);
  });

  it("should verify Google/Vertex caching with repeated large content", async () => {
    const collector = new Collector("gemini-large-content-caching");

    // Create substantial content (2048+ tokens) for Gemini caching
    const largeContent = `
    Quantum computing represents a paradigm shift in computational power, leveraging the principles of quantum mechanics to process information in ways that classical computers cannot. Unlike traditional bits that exist in definite states of 0 or 1, quantum bits (qubits) can exist in superposition, allowing them to represent multiple states simultaneously. This property, combined with quantum entanglement and interference, enables quantum computers to solve certain types of problems exponentially faster than classical computers.

    The development of quantum algorithms has opened new possibilities in cryptography, optimization, machine learning, and simulation of quantum systems. Shor's algorithm, for instance, can factor large integers efficiently, potentially breaking current RSA encryption methods. Grover's algorithm provides a quadratic speedup for searching unsorted databases, while quantum machine learning algorithms promise to accelerate pattern recognition and data analysis tasks.

    Current quantum computers face significant challenges including quantum decoherence, where quantum states are destroyed by environmental interference, and the need for extremely low temperatures to maintain quantum coherence. Error correction in quantum systems requires sophisticated techniques due to the no-cloning theorem, which prevents the direct copying of quantum states for redundancy.

    Major technology companies and research institutions are investing heavily in quantum computing research, developing different approaches including superconducting circuits, trapped ions, topological qubits, and photonic systems. Each approach has its own advantages and challenges in terms of scalability, error rates, and operational requirements.
    `.repeat(8);

    // First call - establishes potential caching patterns
    await b.TestGemini(largeContent, { collector });
    
    // Second call with identical content - should potentially use content caching
    await b.TestGemini(largeContent, { collector });
    
    // Third call to maximize chances of cached behavior
    await b.TestGemini(largeContent, { collector });

    const logs = collector.logs;
    expect(logs.length).toBe(3);

    // Verify all calls have cached tokens fields defined
    logs.forEach((log, index) => {
      expect(log.usage.cachedInputTokens).toBeDefined();
      expect(log.calls[0].usage?.cachedInputTokens).toBeDefined();
      console.log(`Gemini large content call ${index + 1} cached tokens:`, log.usage.cachedInputTokens);
    });

    // Calculate total cached tokens
    const totalCachedTokens = logs.reduce((sum, log) => sum + (log.usage.cachedInputTokens || 0), 0);
    expect(collector.usage.cachedInputTokens).toBe(totalCachedTokens);

    console.log("Large content length:", largeContent.length, "characters");
    console.log("Total Gemini cached tokens from repeated calls:", collector.usage.cachedInputTokens);
  });

  it("should handle Vertex AI streaming with cached token tracking", async () => {
    const collector = new Collector("vertex-streaming-caching");

    const stream = b.stream.TestVertex("Stream with Vertex caching support", { collector });
    
    const chunks = [];
    for await (const chunk of stream) {
      chunks.push(chunk);
    }

    const result = await stream.getFinalResponse();
    expect(result.length).toBeGreaterThan(0);

    const logs = collector.logs;
    expect(logs.length).toBe(1);

    const log = logs[0];
    expect(log.logType).toBe("stream");

    // Verify cached tokens are tracked for streaming
    expect(log.usage.cachedInputTokens).toBeDefined();
    expect(log.calls[0].usage?.cachedInputTokens).toBeDefined();

    console.log("Vertex streaming cached tokens:", log.usage.cachedInputTokens);
  });

  it("should verify AWS provider returns null for cached tokens", async () => {
    const collector = new Collector("aws-no-caching");

    // AWS Bedrock doesn't support caching, so cached tokens should be null
    await b.TestAws("Test AWS without caching support", { collector });

    const logs = collector.logs;
    expect(logs.length).toBe(1);

    const log = logs[0];
    expect(log.functionName).toBe("TestAws");

    // AWS should return null for cached tokens since it doesn't support caching
    expect(log.usage.cachedInputTokens).toBeNull();
    expect(log.calls[0].usage?.cachedInputTokens).toBeNull();
    expect(collector.usage.cachedInputTokens).toBeNull();

    console.log("AWS cached tokens (should be null):", log.usage.cachedInputTokens);
  });

});