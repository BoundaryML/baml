import { encodeBase64, decodeBase64 } from '../lib/base64';

// Simple test to verify embed functionality
describe('Embed Functionality', () => {
  it('should encode and decode project data correctly', () => {
    // Test data
    const testProject = {
      id: 'test-project',
      name: 'Test Function',
      description: 'Test BAML function',
      files: [
        {
          path: '/main.baml',
          content: 'function test { args { text: string } returns string impl { // test } }',
          error: null,
        },
      ],
    };

    // Encode
    const jsonString = JSON.stringify(testProject);
    const encodedData = encodeBase64(jsonString);

    // Decode
    const decodedData = decodeBase64(encodedData);
    const decodedProject = JSON.parse(decodedData);

    // Verify
    expect(decodedProject.name).toBe(testProject.name);
    expect(decodedProject.files[0]?.content).toBe(testProject.files[0]?.content);
  });

  it('should handle special characters in BAML content', () => {
    const testProject = {
      id: 'test-project',
      name: 'Test Function',
      description: 'Test BAML function',
      files: [
        {
          path: '/main.baml',
          content: 'function test { args { text: string } returns string impl { // test with "quotes" and \'apostrophes\' } }',
          error: null,
        },
      ],
    };

    // Encode
    const jsonString = JSON.stringify(testProject);
    const encodedData = encodeBase64(jsonString);

    // Decode
    const decodedData = decodeBase64(encodedData);
    const decodedProject = JSON.parse(decodedData);

    // Verify
    expect(decodedProject.files[0]?.content).toBe(testProject.files[0]?.content);
  });

  it('should generate valid embed URLs', () => {
    const testProject = {
      id: 'test-project',
      name: 'Test Function',
      description: 'Test BAML function',
      files: [
        {
          path: '/main.baml',
          content: 'function test { args { text: string } returns string impl { // test } }',
          error: null,
        },
      ],
    };

    // Encode
    const jsonString = JSON.stringify(testProject);
    const encodedData = encodeBase64(jsonString);

    // Create URL
    const embedUrl = `http://localhost:3000/embed/${encodedData}`;

    // Verify URL format
    expect(embedUrl).toMatch(/^http:\/\/localhost:3000\/embed\/[A-Za-z0-9+/=]+$/);
  });
});