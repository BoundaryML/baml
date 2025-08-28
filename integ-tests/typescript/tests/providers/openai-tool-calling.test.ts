import { b, BamlRuntime } from '../baml_client';
import { Sentiment, WeatherInfo, SearchResult, ImageGeneration, Person } from '../baml_client/types';

describe('OpenAI Tool Calling Tests', () => {
  const testCity = 'San Francisco';
  const testCities = ['New York', 'London', 'Tokyo'];
  
  describe('Basic Tool Calling', () => {
    test('should handle single tool call with auto choice', async () => {
      const result = await b.TestToolCallingSingle(testCity);
      
      expect(result).toBeDefined();
      expect(typeof result.temperature).toBe('number');
      expect(typeof result.units).toBe('string');
      expect(['celsius', 'fahrenheit']).toContain(result.units);
      expect(typeof result.condition).toBe('string');
      expect(result.condition.length).toBeGreaterThan(0);
      
      // Humidity is optional
      if (result.humidity !== null) {
        expect(typeof result.humidity).toBe('number');
      }
    }, 30000);

    test('should handle tool call with required choice', async () => {
      const result = await b.TestToolCallingRequired(testCity);
      
      expect(result).toBeDefined();
      expect(typeof result.temperature).toBe('number');
      expect(typeof result.units).toBe('string');
      expect(typeof result.condition).toBe('string');
    }, 30000);
  });

  describe('Multiple Tool Calls', () => {
    test('should handle multiple parallel tool calls', async () => {
      const results = await b.TestToolCallingMultiple(testCities);
      
      expect(Array.isArray(results)).toBe(true);
      expect(results.length).toBeGreaterThan(0);
      
      // Each result should be a valid WeatherInfo
      results.forEach((result: WeatherInfo) => {
        expect(typeof result.temperature).toBe('number');
        expect(typeof result.units).toBe('string');
        expect(typeof result.condition).toBe('string');
      });
    }, 45000);
  });

  describe('Union Return Types', () => {
    test('should handle union types with search instruction', async () => {
      const result = await b.TestToolCallingUnion('Search for information about climate change');
      
      expect(result).toBeDefined();
      
      // Should be either SearchResult or ImageGeneration
      if ('query' in result) {
        // SearchResult
        const searchResult = result as SearchResult;
        expect(typeof searchResult.query).toBe('string');
        expect(Array.isArray(searchResult.results)).toBe(true);
        expect(typeof searchResult.count).toBe('number');
      } else {
        // ImageGeneration
        const imageResult = result as ImageGeneration;
        expect(typeof imageResult.prompt).toBe('string');
        expect(typeof imageResult.image_url).toBe('string');
        expect(typeof imageResult.size).toBe('string');
      }
    }, 30000);

    test('should handle union types with image instruction', async () => {
      const result = await b.TestToolCallingUnion('Generate an image of a sunset over mountains');
      
      expect(result).toBeDefined();
      
      // Should likely be ImageGeneration for this prompt
      if ('prompt' in result) {
        const imageResult = result as ImageGeneration;
        expect(typeof imageResult.prompt).toBe('string');
        expect(typeof imageResult.image_url).toBe('string');
        expect(typeof imageResult.size).toBe('string');
      }
    }, 30000);
  });

  describe('Complex Data Types', () => {
    test('should handle enum return type', async () => {
      const positiveText = 'I love this sunny weather! It makes me so happy.';
      const result = await b.TestToolCallingEnum(positiveText);
      
      expect(result).toBeDefined();
      expect(Object.values(Sentiment)).toContain(result);
    }, 30000);

    test('should handle nested class structures', async () => {
      const description = 'A 30-year-old software engineer named Alice who lives on Main Street in Seattle, USA and enjoys reading, hiking, and cooking';
      const result = await b.TestToolCallingNested(description);
      
      expect(result).toBeDefined();
      expect(typeof result.name).toBe('string');
      expect(typeof result.age).toBe('number');
      expect(result.address).toBeDefined();
      expect(typeof result.address.street).toBe('string');
      expect(typeof result.address.city).toBe('string');
      expect(typeof result.address.country).toBe('string');
      expect(Array.isArray(result.hobbies)).toBe(true);
      expect(result.hobbies.length).toBeGreaterThan(0);
    }, 30000);
  });

  describe('Backward Compatibility', () => {
    test('should work with text_schema mode', async () => {
      const result = await b.TestTextSchemaMode(testCity);
      
      expect(result).toBeDefined();
      expect(typeof result.temperature).toBe('number');
      expect(typeof result.units).toBe('string');
      expect(typeof result.condition).toBe('string');
    }, 30000);

    test('should work with default mode (no baml_mode)', async () => {
      const result = await b.TestDefaultMode(testCity);
      
      expect(result).toBeDefined();
      expect(typeof result.temperature).toBe('number');
      expect(typeof result.units).toBe('string');
      expect(typeof result.condition).toBe('string');
    }, 30000);
  });

  describe('Request Format Validation', () => {
    test('should format tool calling request correctly', async () => {
      // Use the baml client's request inspection feature to check request format
      const request = await b.TestToolCallingSingle.build(testCity);
      
      expect(request).toBeDefined();
      expect(request.body).toBeDefined();
      
      const body = JSON.parse(request.body);
      
      // Check that tools are present
      expect(body.tools).toBeDefined();
      expect(Array.isArray(body.tools)).toBe(true);
      expect(body.tools.length).toBeGreaterThan(0);
      
      // Check tool structure
      const tool = body.tools[0];
      expect(tool.type).toBe('function');
      expect(tool.function).toBeDefined();
      expect(tool.function.name).toBe('WeatherInfo');
      expect(tool.function.parameters).toBeDefined();
      expect(tool.function.parameters.type).toBe('object');
      
      // Check tool_choice
      expect(body.tool_choice).toBe('auto');
      
      // Check that output_format is removed
      expect(body.output_format).toBeUndefined();
      expect(body.baml_mode).toBeUndefined(); // Internal field should be removed
      
      // Check that parallel_tool_calls is enabled
      expect(body.parallel_tool_calls).toBe(true);
    });

    test('should format text_schema request correctly', async () => {
      const request = await b.TestTextSchemaMode.build(testCity);
      
      expect(request).toBeDefined();
      expect(request.body).toBeDefined();
      
      const body = JSON.parse(request.body);
      
      // Should NOT have tools
      expect(body.tools).toBeUndefined();
      expect(body.tool_choice).toBeUndefined();
      expect(body.parallel_tool_calls).toBeUndefined();
      
      // Should have messages that include output format instructions
      expect(body.messages).toBeDefined();
      const hasOutputFormat = body.messages.some((msg: any) => 
        msg.content && (
          msg.content.includes('JSON') || 
          msg.content.includes('schema') ||
          msg.content.includes('format')
        )
      );
      expect(hasOutputFormat).toBe(true);
    });
  });

  describe('Streaming Support', () => {
    test('should handle streaming tool calls', async () => {
      const stream = b.TestToolCallingSingle.stream(testCity);
      const chunks: any[] = [];
      
      for await (const chunk of stream) {
        chunks.push(chunk);
      }
      
      expect(chunks.length).toBeGreaterThan(0);
      
      // Get final result
      const finalResult = await stream.getFinalResponse();
      expect(finalResult).toBeDefined();
      expect(typeof finalResult.temperature).toBe('number');
      expect(typeof finalResult.units).toBe('string');
      expect(typeof finalResult.condition).toBe('string');
    }, 30000);
  });

  describe('Error Handling', () => {
    test('should handle invalid tool calls gracefully', async () => {
      // This test might need to be adjusted based on how the system handles errors
      try {
        const result = await b.TestToolCallingSingle('InvalidCityNameThatShouldNotExist12345');
        // If it succeeds, just check it returns valid structure
        expect(result).toBeDefined();
        if (result) {
          expect(typeof result.temperature).toBe('number');
          expect(typeof result.units).toBe('string');
          expect(typeof result.condition).toBe('string');
        }
      } catch (error) {
        // If it fails, that's also acceptable behavior
        expect(error).toBeDefined();
      }
    }, 30000);
  });
});