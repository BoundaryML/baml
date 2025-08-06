import { type ChildProcess, spawn } from 'node:child_process';
import { Sema } from 'async-sema';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { SitemapGenerator, slugify } from './sitemap';

const TEST_DOCS_PATH = '/Users/sam/baml2/fern/docs.yml';

const MAX_CONCURRENT_REQUESTS = 20;
const FETCH_TIMEOUT_MS = 5_000;

describe('slugify', () => {
  const SLUGIFY_TESTS = [
    { input: '_.role', expectedSlug: 'role' },
    { input: '@@dynamic', expectedSlug: 'dynamic' },
    { input: '@assert', expectedSlug: 'assert' },
    { input: '@check', expectedSlug: 'check' },
    { input: '@skip', expectedSlug: 'skip' },
    { input: 'Action Item Extraction', expectedSlug: 'action-item-extraction' },
    { input: 'Anthropic', expectedSlug: 'anthropic' },
    { input: 'array (list)', expectedSlug: 'array-list' },
    { input: 'Attributes', expectedSlug: 'attributes' },
    { input: 'AWS', expectedSlug: 'aws' },
    { input: 'AWS Bedrock', expectedSlug: 'aws-bedrock' },
    { input: 'BAML Advanced', expectedSlug: 'baml-advanced' },
    { input: 'BAML Basics', expectedSlug: 'baml-basics' },
    { input: 'BAML vs Marvin', expectedSlug: 'baml-vs-marvin' },
    { input: 'BAML vs Pydantic', expectedSlug: 'baml-vs-pydantic' },
    { input: 'baml-cli', expectedSlug: 'baml-cli' },
    { input: 'baml.cliPath', expectedSlug: 'baml-cli-path' },
    {
      input: 'baml.enablePlaygroundProxy',
      expectedSlug: 'baml-enable-playground-proxy',
    },
    {
      input: 'baml.generateCodeOnSave',
      expectedSlug: 'baml-generate-code-on-save',
    },
    {
      input: 'baml.syncExtensionToGeneratorVersion',
      expectedSlug: 'baml-sync-extension-to-generator-version',
    },
    {
      input: 'BamlClientFinishReasonError',
      expectedSlug: 'baml-client-finish-reason-error',
    },
    { input: 'BamlValidationError', expectedSlug: 'baml-validation-error' },
    { input: 'bool', expectedSlug: 'bool' },
    { input: 'Boundary Cloud', expectedSlug: 'boundary-cloud' },
    { input: 'Building a Chatbot', expectedSlug: 'building-a-chatbot' },
    { input: 'Chain of Thought', expectedSlug: 'chain-of-thought' },
    { input: 'Changelog', expectedSlug: 'changelog' },
    { input: 'Chat', expectedSlug: 'chat' },
    { input: 'Checks and Asserts', expectedSlug: 'checks-and-asserts' },
    { input: 'class', expectedSlug: 'class' },
    { input: 'Classification', expectedSlug: 'classification' },
    { input: 'client<llm>', expectedSlug: 'client-llm' },
    { input: 'ClientRegistry', expectedSlug: 'client-registry' },
    {
      input: 'Collector (track tokens)',
      expectedSlug: 'collector-track-tokens',
    },
    { input: 'comments', expectedSlug: 'comments' },
    { input: 'Comparisons', expectedSlug: 'comparisons' },
    { input: 'Concurrent Calls', expectedSlug: 'concurrent-calls' },
    { input: 'Conditionals', expectedSlug: 'conditionals' },
    { input: 'Contact', expectedSlug: 'contact' },
    { input: 'ctx.client', expectedSlug: 'ctx-client' },
    { input: 'ctx.output_format', expectedSlug: 'ctx-output-format' },
    { input: 'Cursor Extension', expectedSlug: 'cursor-extension' },
    { input: 'Deploying', expectedSlug: 'deploying' },
    { input: 'dev', expectedSlug: 'dev' },
    { input: 'Development', expectedSlug: 'development' },
    { input: 'Docker', expectedSlug: 'docker' },
    { input: 'Docker (REST API)', expectedSlug: 'docker-rest-api' },
    {
      input: 'Editor Extension Settings',
      expectedSlug: 'editor-extension-settings',
    },
    { input: 'enum', expectedSlug: 'enum' },
    { input: 'Environment Variables', expectedSlug: 'environment-variables' },
    { input: 'Error Handling', expectedSlug: 'error-handling' },
    { input: 'Errors', expectedSlug: 'errors' },
    { input: 'Fallback', expectedSlug: 'fallback' },
    { input: 'fmt', expectedSlug: 'fmt' },
    { input: 'Framework Integration', expectedSlug: 'framework-integration' },
    { input: 'function', expectedSlug: 'function' },
    { input: 'General BAML Syntax', expectedSlug: 'general-baml-syntax' },
    { input: 'generate', expectedSlug: 'generate' },
    { input: 'generator', expectedSlug: 'generator' },
    { input: 'Google AI: Gemini', expectedSlug: 'google-ai-gemini' },
    { input: 'Google: Vertex', expectedSlug: 'google-vertex' },
    { input: 'Hello World', expectedSlug: 'hello-world' },
    { input: 'HookData', expectedSlug: 'hook-data' },
    { input: 'HookInput', expectedSlug: 'hook-input' },
    { input: 'HookOutput', expectedSlug: 'hook-output' },
    { input: 'init', expectedSlug: 'init' },
    { input: 'Installation: Editors', expectedSlug: 'installation-editors' },
    { input: 'Installation: Language', expectedSlug: 'installation-language' },
    { input: 'int / float', expectedSlug: 'int-float' },
    { input: 'Interactive Examples', expectedSlug: 'interactive-examples' },
    { input: 'Introduction', expectedSlug: 'introduction' },
    { input: 'Jinja in Attributes', expectedSlug: 'jinja-in-attributes' },
    { input: 'LLM Client Providers', expectedSlug: 'llm-client-providers' },
    { input: 'LLM Client Registry', expectedSlug: 'llm-client-registry' },
    { input: 'LLM Client Strategies', expectedSlug: 'llm-client-strategies' },
    { input: 'Loops', expectedSlug: 'loops' },
    { input: 'map (dictionary)', expectedSlug: 'map-dictionary' },
    { input: 'Modular API', expectedSlug: 'modular-api' },
    {
      input: 'Multi-Modal (Images / Audio)',
      expectedSlug: 'multi-modal-images-audio',
    },
    { input: 'Observability', expectedSlug: 'observability' },
    { input: 'OpenAI', expectedSlug: 'open-ai' },
    { input: 'OpenAI from Azure', expectedSlug: 'open-ai-from-azure' },
    { input: 'OpenAI Responses API', expectedSlug: 'open-ai-responses-api' },
    { input: 'openai-generic', expectedSlug: 'openai-generic' },
    { input: 'Others', expectedSlug: 'others' },
    { input: 'Overview', expectedSlug: 'overview' },
    {
      input: 'PII Data Extraction / Scrubbing',
      expectedSlug: 'pii-data-extraction-scrubbing',
    },
    {
      input: 'Prompt Caching / Message Role Metadata',
      expectedSlug: 'prompt-caching-message-role-metadata',
    },
    { input: 'Prompt Engineering', expectedSlug: 'prompt-engineering' },
    { input: 'Prompt Syntax', expectedSlug: 'prompt-syntax' },
    { input: 'Prompting with BAML', expectedSlug: 'prompting-with-baml' },
    { input: 'Python', expectedSlug: 'python' },
    { input: 'Quick Start', expectedSlug: 'quick-start' },
    { input: 'React/Next.js', expectedSlug: 'react-next-js' },
    {
      input: 'Reducing Hallucinations',
      expectedSlug: 'reducing-hallucinations',
    },
    {
      input: 'REST API (other languages)',
      expectedSlug: 'rest-api-other-languages',
    },
    {
      input: 'Retrieval Augmented Generation',
      expectedSlug: 'retrieval-augmented-generation',
    },
    { input: 'Retry Policy', expectedSlug: 'retry-policy' },
    {
      input: 'Reusing Prompt Snippets',
      expectedSlug: 'reusing-prompt-snippets',
    },
    { input: 'Round Robin', expectedSlug: 'round-robin' },
    { input: 'Ruby', expectedSlug: 'ruby' },
    { input: 'serve', expectedSlug: 'serve' },
    { input: 'Streaming', expectedSlug: 'streaming' },
    { input: 'string', expectedSlug: 'string' },
    { input: 'Symbol Tuning', expectedSlug: 'symbol-tuning' },
    { input: 'template_string', expectedSlug: 'template-string' },
    { input: 'Terminal Logs', expectedSlug: 'terminal-logs' },
    { input: 'test', expectedSlug: 'test' },
    { input: 'Testing functions', expectedSlug: 'testing-functions' },
    {
      input: 'Tools / Function Calling',
      expectedSlug: 'tools-function-calling',
    },
    { input: 'Tracking Usage', expectedSlug: 'tracking-usage' },
    { input: 'TypeBuilder', expectedSlug: 'type-builder' },
    { input: 'Types', expectedSlug: 'types' },
    { input: 'Typescript', expectedSlug: 'typescript' },
    { input: 'Upgrade BAML versions', expectedSlug: 'upgrade-baml-versions' },
    { input: 'use{FunctionName} Hook', expectedSlug: 'use-function-name-hook' },
    { input: 'Variables', expectedSlug: 'variables' },
    { input: 'VSCode Extension', expectedSlug: 'vs-code-extension' },
    { input: 'Welcome', expectedSlug: 'welcome' },
    { input: 'What are attributes?', expectedSlug: 'what-are-attributes' },
    { input: 'What is BAML?', expectedSlug: 'what-is-baml' },
    { input: 'What is jinja?', expectedSlug: 'what-is-jinja' },
    { input: 'with_options(..)', expectedSlug: 'with-options' },
  ];

  it('should have at least 10 unique tests', () => {
    const uniqueTests = Array.from(
      new Map(SLUGIFY_TESTS.map((test) => [test.input, test])).values(),
    )
      .sort((a, b) => a.input.localeCompare(b.input))
      .map((input) => JSON.stringify(input));
    expect(uniqueTests.length).toBeGreaterThanOrEqual(10);
  });

  SLUGIFY_TESTS.map(({ input, expectedSlug }) =>
    it(`should slugify "${input}" correctly`, () => {
      expect(slugify(input)).toBe(expectedSlug);
    }),
  );
});

describe('Sitemap Generation and Validation', () => {
  const concurrentRequestsSemaphore = new Sema(MAX_CONCURRENT_REQUESTS);

  let fernProcess: ChildProcess;
  let fernUrl: string;

  beforeAll(async () => {
    fernProcess = spawn('npx', ['fern', 'docs', 'dev'], {
      cwd: '/Users/sam/baml2/fern',
      stdio: 'pipe',
    });

    // Wait for the server to be ready and extract the base URL
    await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error('Fern server failed to start within timeout'));
      }, 30000);

      fernProcess.stdout?.on('data', (data: Buffer) => {
        const output = data.toString();
        console.log('Fern stdout:', output); // Debug output

        // Look for the specific ready message
        const readyMatch = output.match(/Development server ready on (http:\/\/localhost:\d+)/);
        if (readyMatch) {
          fernUrl = readyMatch[1];
          console.log(`✅ Fern server ready at: ${fernUrl}`);
          clearTimeout(timeout);
          // Give the server a moment to fully initialize
          setTimeout(() => resolve(void 0), 2000);
        }
      });

      fernProcess.stderr?.on('data', (data: Buffer) => {
        console.error('Fern stderr:', data.toString());
      });

      fernProcess.on('error', (error: Error) => {
        clearTimeout(timeout);
        reject(error);
      });
    });
  }, 15_000); // 10s timeout for setup

  afterAll(() => {
    if (fernProcess) {
      fernProcess.kill();
    }
  });

  it('should build sitemap and validate all links', async () => {
    const generator = new SitemapGenerator(TEST_DOCS_PATH);
    const entries = await generator.generateSitemap({
      includeBlogPosts: false,
    });

    const fetches = [];

    for (const entry of entries) {
      if (entry.type !== 'fern') {
        continue;
      }

      fetches.push(
        (async () => {
          // Clean URL construction to avoid double slashes
          const cleanHref = entry.href.startsWith('/') ? entry.href.slice(1) : entry.href;
          const url = `${fernUrl}/${cleanHref}`;
          try {
            await concurrentRequestsSemaphore.acquire();
            const response = await fetch(url, {
              method: 'GET',
              signal: AbortSignal.timeout(FETCH_TIMEOUT_MS), // 5 second timeout
            });
            if (!response.ok) {
              throw new Error(`Failed to fetch ${url}: ${response.statusText}`);
            }
            return { entry, success: true };
          } catch (error) {
            return { entry, success: false, error };
          } finally {
            concurrentRequestsSemaphore.release();
          }
        })(),
      );
    }

    const results = (await Promise.allSettled(fetches)).filter(
      (result) => result.status === 'fulfilled',
    );
    expect(results.length).toBeGreaterThan(100);

    const failed = results.filter((result) => result.value.success === false);
    if (failed.length > 0) {
      console.error(
        `❌ Sitemap generated ${failed.length} invalid links:`,
        failed.map((result) => result.value.entry),
      );
      throw new Error(`Sitemap generated invalid links: ${failed.length}`);
    }

    console.info(`✅ Sitemap generated ${results.length} valid links`);
  }, 30_000);
});
