import { type LanguageInput } from 'shiki';
import bamlJinjaTextmateJsonString from './bamlJinjaTextmate.json';
import bamlTextmateJsonString from './bamlTextmate.json';

/**
 * Converts a Textmate grammar JSON object to a Shiki-compatible LanguageInput object.
 * - Converts capture keys from strings to numbers.
 * - Removes any 'comment' keys from patterns and repository items.
 *
 * @param textmateGrammar The Textmate grammar JSON object to convert.
 * @returns A LanguageInput object compatible with Shiki.
 */
export function convertTextmateToShiki(
  textmateGrammar: Record<string, any>,
  embeddedLangs: string[] = [],
): LanguageInput {
  const {
    fileTypes = [],
    name = '',
    patterns = [],
    repository = {},
    scopeName = '',
  } = textmateGrammar;

  /**
   * Converts string keys of captures to numeric keys.
   * @param captures The captures object with string keys.
   * @returns A captures object with numeric keys.
   */
  const convertCaptures = (
    captures: Record<string, any>,
  ): Record<number, any> => {
    const numericCaptures: Record<number, any> = {};
    for (const key in captures) {
      // eslint-disable-next-line no-prototype-builtins
      if (captures.hasOwnProperty(key) && /^\d+$/.test(key)) {
        numericCaptures[Number(key)] = captures[key];
      }
      // eslint-disable-next-line no-prototype-builtins
      if (captures.hasOwnProperty(key) && /^\d+$/.test(key)) {
        numericCaptures[Number(key)] = captures[key];
      }
      // Ignore non-numeric keys
    }
    return numericCaptures;
  };

  /**
   * Recursively processes patterns to ensure Shiki compatibility.
   * - Converts capture keys from strings to numbers.
   * - Removes any 'comment' keys.
   *
   * @param patterns Array of pattern objects.
   * @returns Processed array of patterns.
   */
  const processPatterns = (patterns: any[]): any[] => {
    return patterns.map((pattern) => {
      const processedPattern: Record<string, any> = { ...pattern };

      // Remove 'comment' key if it exists
      if (processedPattern.comment) {
        delete processedPattern.comment;
      }

      // Handle 'include' statements (Shiki supports them similarly to Textmate)
      if (pattern.include) {
        processedPattern.include = pattern.include;
      }

      // Convert capture keys from strings to numbers
      if (processedPattern.captures) {
        processedPattern.captures = convertCaptures(processedPattern.captures);
      }
      if (processedPattern.beginCaptures) {
        processedPattern.beginCaptures = convertCaptures(
          processedPattern.beginCaptures,
        );
      }
      if (processedPattern.endCaptures) {
        processedPattern.endCaptures = convertCaptures(
          processedPattern.endCaptures,
        );
      }

      // Recursively process nested 'patterns' arrays
      if (
        processedPattern.patterns &&
        Array.isArray(processedPattern.patterns)
      ) {
        processedPattern.patterns = processPatterns(processedPattern.patterns);
      }

      // Recursively process nested 'repository' references
      if (
        processedPattern.repository &&
        Array.isArray(processedPattern.repository)
      ) {
        processedPattern.repository = processPatterns(
          processedPattern.repository,
        );
      }

      return processedPattern;
    });
  };

  /**
   * Processes the repository by recursively processing its patterns.
   * - Converts capture keys from strings to numbers.
   * - Removes any 'comment' keys.
   *
   * @param repository The repository object from Textmate grammar.
   * @returns Processed repository object.
   */
  const processRepository = (
    repository: Record<string, any>,
  ): Record<string, any> => {
    const processedRepo: Record<string, any> = {};
    for (const key in repository) {
      // eslint-disable-next-line no-prototype-builtins
      if (repository.hasOwnProperty(key)) {
        const item = repository[key];
        processedRepo[key] = { ...item };

        // Remove 'comment' key if it exists
        if (processedRepo[key].comment) {
          delete processedRepo[key].comment;
        }

        // Convert capture keys from strings to numbers
        if (processedRepo[key].captures) {
          processedRepo[key].captures = convertCaptures(
            processedRepo[key].captures,
          );
        }
        if (processedRepo[key].beginCaptures) {
          processedRepo[key].beginCaptures = convertCaptures(
            processedRepo[key].beginCaptures,
          );
        }
        if (processedRepo[key].endCaptures) {
          processedRepo[key].endCaptures = convertCaptures(
            processedRepo[key].endCaptures,
          );
        }

        // Recursively process nested 'patterns' arrays
        if (item.patterns && Array.isArray(item.patterns)) {
          processedRepo[key].patterns = processPatterns(item.patterns);
        }

        // If the repository item has its own repository, process it recursively
        if (item.repository && typeof item.repository === 'object') {
          processedRepo[key].repository = processRepository(item.repository);
        }
      }
    }
    return processedRepo;
  };

  // Construct the LanguageInput object
  const shikiGrammar: LanguageInput = {
    fileTypes,
    name,
    embeddedLangs,
    scopeName,
    patterns: processPatterns(patterns),
    repository: processRepository(repository),
  };

  return shikiGrammar;
}

export const bamlTextmate = convertTextmateToShiki(bamlTextmateJsonString, [
  'baml-jinja',
]);
// the name of the lang is baml-jinja (make sure to change the json to match it)
export const bamlJinjaTextmate = convertTextmateToShiki(
  bamlJinjaTextmateJsonString,
  [],
);
