import 'jotai-devtools/styles.css';
import { default as EnvVars } from '@baml/playground-common/shared/baml-project-panel/playground-panel/side-bar/env-vars-old';
import { createStore } from 'jotai';
import { Provider as JotaiProvider } from 'jotai';
import { ThemeProvider } from 'next-themes';
import '../App.css';
import { envVarsAtom } from '@baml/playground-common/shared/baml-project-panel/atoms';

interface JotaiProviderProps {
  envVars: Record<string, string>;
  children: React.ReactNode;
}

const JotaiStorybookProvider: React.FC<JotaiProviderProps> = ({
  envVars,
  children,
}) => {
  const storybookStore = createStore();
  storybookStore.set(envVarsAtom, envVars);
  return <JotaiProvider store={storybookStore}>{children}</JotaiProvider>;
};

export default {
  title: 'EnvVarsOld',
  component: EnvVars,
  decorators: [
    (Story: React.FC) => (
      <div>
        <ThemeProvider
          attribute="class"
          defaultTheme="dark"
          enableSystem={false}
          disableTransitionOnChange={true}
        >
          <Story />
        </ThemeProvider>
      </div>
    ),
  ],
  parameters: {
    // More on how to position stories at: https://storybook.js.org/docs/configure/story-layout
    layout: 'centered',
  },
};

// More on component testing: https://storybook.js.org/docs/writing-tests/component-testing
export const WithFilledVariables = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider
        envVars={{
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

export const WithMissingRequired = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider
        envVars={{
          ANTHROPIC_API_KEY: 'sk-ant456',
          COHERE_API_KEY: 'sk-coh789',
          OPENAI_API_KEY: '',
        }}
      >
        <Story />
      </JotaiStorybookProvider>
    ),
  ],
};
