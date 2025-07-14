import 'jotai-devtools/styles.css';
import { createStore, useAtomValue } from 'jotai';
import { Provider as JotaiProvider } from 'jotai';
import { ThemeProvider } from 'next-themes';
import '../App.css';
import { ApiKeysDialog } from '@baml/playground-common/components/api-keys-dialog/dialog';
import { apiKeysAtom } from '@baml/playground-common/components/api-keys-dialog/atoms';

interface JotaiProviderProps {
  apiKeys: Record<string, string>;
  children: React.ReactNode;
}

const JotaiStorybookProvider: React.FC<JotaiProviderProps> = ({
  apiKeys,
  children,
}) => {
  const storybookStore = createStore();
  storybookStore.set(apiKeysAtom, apiKeys);
  return <JotaiProvider store={storybookStore}>{children}</JotaiProvider>;
};

const WrappedEnvVars: React.FC = () => {
  const apiKeys = useAtomValue(apiKeysAtom);
  return (
    <div>
      <ThemeProvider
        attribute="class"
        defaultTheme="dark"
        enableSystem={false}
        disableTransitionOnChange={true}
      >
        <div className="flex gap-8 items-start">
          <ApiKeysDialog />
          <div className="p-4 bg-[#1e1e1e] rounded-lg min-w-[300px]">
            <h3 className="mb-2 text-sm font-mono">envVars</h3>
            <pre className="text-xs">{JSON.stringify(apiKeys, null, 2)}</pre>
          </div>
        </div>
      </ThemeProvider>
    </div>
  );
};

export default {
  title: 'EnvVars',
  component: WrappedEnvVars,
  decorators: [],
  parameters: {
    // More on how to position stories at: https://storybook.js.org/docs/configure/story-layout
    layout: 'centered',
  },
};

// More on component testing: https://storybook.js.org/docs/writing-tests/component-testing
export const NoRequiredEnvVarsAreSet = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider apiKeys={{}}>
        <div>
          <ThemeProvider
            attribute="class"
            defaultTheme="dark"
            enableSystem={false}
            disableTransitionOnChange={true}
          >
            <div className="flex gap-8 items-start">
              <Story />
            </div>
          </ThemeProvider>
        </div>
      </JotaiStorybookProvider>
    ),
  ],
};

export const SomeRequiredEnvVarsAreSet = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider
        apiKeys={{
          ANTHROPIC_API_KEY: 'sk-ant456',
          COHERE_API_KEY: 'sk-coh789',
          OPENAI_API_KEY: '',
        }}
      >
        <div>
          <ThemeProvider
            attribute="class"
            defaultTheme="dark"
            enableSystem={false}
            disableTransitionOnChange={true}
          >
            <div className="flex gap-8 items-start">
              <Story />
            </div>
          </ThemeProvider>
        </div>
      </JotaiStorybookProvider>
    ),
  ],
};

export const AllRequiredEnvVarsAreSet = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider
        apiKeys={{
          ANTHROPIC_API_KEY: 'sk-ant456',
          COHERE_API_KEY: 'sk-coh789',
          OPENAI_API_KEY: 'sk-test123',
        }}
      >
        <Story />
      </JotaiStorybookProvider>
    ),
  ],
};

export const EnvVarContainsNewlines = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider
        apiKeys={{
          ANTHROPIC_API_KEY: 'line1\nline2\nline3',
          COHERE_API_KEY: 'sk-coh789',
          OPENAI_API_KEY: 'sk-test123',
        }}
      >
        <Story />
      </JotaiStorybookProvider>
    ),
  ],
};

export const TableWith100EnvVars = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider
        apiKeys={{
          ANTHROPIC_API_KEY: 'sk-ant456',
          COHERE_API_KEY: 'sk-coh789',
          OPENAI_API_KEY: 'sk-test123',
          ...Object.fromEntries(
            Array.from({ length: 100 }, (_, i) => `VAR_${i}`).map((key) => [
              key,
              `value_${key}`,
            ]),
          ),
        }}
      >
        <Story />
      </JotaiStorybookProvider>
    ),
  ],
};

export const TableWith100EnvVarsInDialog = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider
        apiKeys={{
          ANTHROPIC_API_KEY: 'sk-ant456',
          COHERE_API_KEY: 'sk-coh789',
          OPENAI_API_KEY: 'sk-test123',
          ...Object.fromEntries(
            Array.from({ length: 100 }, (_, i) => `VAR_${i}`).map((key) => [
              key,
              `value_${key}`,
            ]),
          ),
        }}
      >
        <ApiKeysDialog />
      </JotaiStorybookProvider>
    ),
  ],
};

export const VeryLongEnvVarNameInDialog = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider
        apiKeys={{
          ANTHROPIC_API_KEY: 'line1\nline2\nline3',
          LONG_ENV_VAR_NAME_THAT_EXCEEDS_MAX_WIDTH_OF_THE_TABLE_CELL:
            'sk-test123',
          OPENAI_API_KEY: 'sk-test123',
        }}
      >
        <ApiKeysDialog />
      </JotaiStorybookProvider>
    ),
  ],
};
