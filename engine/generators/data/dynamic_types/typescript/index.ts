// E2E Tests for TypeBuilder and dynamic types
// Mirrors the Rust e2e tests in ../rust/main.rs

import { describe, it, expect } from 'vitest';
import TypeBuilder from '../baml_client/type_builder';
import { b } from '../baml_client';

describe('TypeBuilder E2E Tests', () => {
  describe('Dynamic Class Property', () => {
    it('should add property to dynamic class and call LLM', async () => {
      const tb = new TypeBuilder();

      // Add dynamic property "occupation" to Person class
      tb.Person.addProperty('occupation', tb.string())
        .description('The person\'s job or profession');

      // Call function with TypeBuilder
      const result = await b.GetPerson(
        'A software engineer named Alice who is 30 years old and works as a backend developer',
        { tb }
      );

      // Verify static fields
      expect(result.name).toBeTruthy();
      expect(result.age).toBeGreaterThan(0);

      // Verify dynamic field
      expect(result['occupation']).toBeTruthy();
      console.log(`Got person: ${result.name} (age ${result.age}), occupation: ${result['occupation']}`);
    });
  });

  describe('Multiple Dynamic Properties', () => {
    it('should add multiple properties with different types', async () => {
      const tb = new TypeBuilder();

      tb.Person.addProperty('email', tb.string())
        .description('Email address');
      tb.Person.addProperty('is_employed', tb.bool())
        .description('Whether currently employed');
      tb.Person.addProperty('years_experience', tb.int())
        .description('Years of work experience');

      const result = await b.GetPerson(
        'Bob Smith, age 35, email bob@example.com, currently employed with 10 years experience',
        { tb }
      );

      expect(result.name).toBeTruthy();
      expect(result.age).toBeGreaterThan(0);
      expect(result['email']).toBeTruthy();
      expect(result['is_employed']).toBeDefined();
      expect(result['years_experience']).toBeDefined();

      console.log(`Person: ${result.name}, email: ${result['email']}, employed: ${result['is_employed']}, years: ${result['years_experience']}`);
    });
  });

  describe('Dynamic Enum Value', () => {
    it('should add enum values and classify correctly', async () => {
      const tb = new TypeBuilder();

      // Add new enum values to Category
      tb.Category.addValue('Sports').description('Sports and athletics news');
      tb.Category.addValue('Politics').description('Political news and government');
      tb.Category.addValue('Entertainment').description('Movies, TV, celebrities');

      const result = await b.ClassifyArticle(
        'The Lakers won the championship last night with a stunning 3-pointer in overtime',
        { tb }
      );

      const categoryStr = String(result);
      console.log(`Category: ${categoryStr}`);

      // Should be one of our categories
      expect(['Sports', 'Technology', 'Science', 'Arts', 'Politics', 'Entertainment']).toContain(categoryStr);
    });
  });

  describe('Nested Dynamic Types', () => {
    it('should handle nested dynamic types', async () => {
      const tb = new TypeBuilder();

      // Add dynamic property to Person (nested in Article)
      tb.Person.addProperty('bio', tb.string());

      // Add dynamic properties to Article
      tb.Article.addProperty('word_count', tb.int());
      tb.Article.addProperty('published', tb.bool());

      // Add new category value
      tb.Category.addValue('Business');

      const result = await b.CreateArticle(
        'A 500-word published article about tech startups by John Doe, a tech journalist',
        { tb }
      );

      // Verify static fields
      expect(result.title).toBeTruthy();
      expect(result.author.name).toBeTruthy();

      // Verify dynamic fields on Article
      expect(result['word_count']).toBeDefined();
      expect(result['published']).toBeDefined();

      // Verify dynamic field on nested Person
      expect(result.author['bio']).toBeDefined();

      console.log(`Article: ${result.title} by ${result.author.name} (category: ${result.category})`);
    });
  });

  describe('Complex Dynamic Types', () => {
    it('should handle lists and optionals in dynamic properties', async () => {
      const tb = new TypeBuilder();

      // Add list of strings
      tb.Person.addProperty('skills', tb.string().list());

      // Add optional string
      tb.Person.addProperty('nickname', tb.string().optional())
        .description('The person\'s nickname (if explicitly provided)');

      const result = await b.GetPerson(
        'Alice Johnson, 28, skills: Rust, Python, Go. Nickname: AJ',
        { tb }
      );

      console.log(`Person: ${result.name} (age ${result.age})`);

      // Check skills list
      expect(Array.isArray(result['skills'])).toBe(true);
      expect((result['skills'] as string[]).length).toBe(3);

      // Check optional nickname
      expect(result['nickname']).toBe('AJ');
    });
  });

  describe('Alias for Enum Values', () => {
    it('should use alias for better LLM matching', async () => {
      const tb = new TypeBuilder();

      // Add a category with an alias
      tb.Category.addValue('AI')
        .alias('Artificial Intelligence')
        .description('Artificial intelligence and machine learning');

      const result = await b.ClassifyArticle(
        'GPT-5 achieves human-level reasoning in new benchmarks, researchers claim',
        { tb }
      );

      const categoryStr = String(result);
      console.log(`Category for AI article: ${categoryStr}`);

      // Should be AI or Technology
      expect(['AI', 'Technology']).toContain(categoryStr);
    });
  });

  describe('Fully Dynamic Class', () => {
    it('should create completely new class at runtime', async () => {
      const tb = new TypeBuilder();

      // Create a completely new class at runtime
      const productClass = tb.addClass('Product');
      productClass.addProperty('name', tb.string());
      productClass.addProperty('price', tb.float());
      productClass.addProperty('in_stock', tb.bool());

      // Verify the type is registered
      const productType = productClass.type();
      console.log(`Created dynamic Product type`);

      // Note: Can't call a function that returns Product directly
      // because it's not in the schema, but we verify the type exists
      expect(productType).toBeDefined();
    });
  });
});
