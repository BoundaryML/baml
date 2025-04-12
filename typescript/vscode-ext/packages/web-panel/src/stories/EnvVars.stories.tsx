import { expect } from '@storybook/test'
import { DevTools } from 'jotai-devtools'
import 'jotai-devtools/styles.css'
import { atom, createStore } from 'jotai'
import { default as EnvVars } from '../shared/baml-project-panel/playground-panel/side-bar/env-vars'
import { Provider as JotaiProvider } from 'jotai'
import { ThemeProvider } from 'next-themes'
import '../App.css'
import { envVarsAtom } from '../shared/baml-project-panel/atoms'

interface JotaiProviderProps {
  envVars: Record<string, string>
  children: React.ReactNode
}

const JotaiStorybookProvider: React.FC<JotaiProviderProps> = ({ envVars, children }) => {
  const storybookStore = createStore()
  storybookStore.set(envVarsAtom, envVars)
  return <JotaiProvider store={storybookStore}>{children}</JotaiProvider>
}

export default {
  title: 'EnvVars',
  component: EnvVars,
  decorators: [
    (Story: React.FC) => (
      <div>
        <ThemeProvider attribute='class' defaultTheme='dark' enableSystem={false} disableTransitionOnChange={true}>
          <Story />
        </ThemeProvider>
      </div>
    ),
  ],
  parameters: {
    // More on how to position stories at: https://storybook.js.org/docs/configure/story-layout
    layout: 'centered',
  },
}

// More on component testing: https://storybook.js.org/docs/writing-tests/component-testing
export const WithNoRequired = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider envVars={{}}>
        <Story />
      </JotaiStorybookProvider>
    ),
  ],
}

// ANTHROPIC_API_KEY and OPENAI_API_KEY are required by default
export const WithSomeRequiredAndMore = {
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
}

// ANTHROPIC_API_KEY and OPENAI_API_KEY are required by default
export const WithAllRequiredAndMore = {
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
}

// ANTHROPIC_API_KEY and OPENAI_API_KEY are required by default
export const With100EnvVars = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider
        envVars={{
          ANTHROPIC_API_KEY: 'sk-ant456',
          COHERE_API_KEY: 'sk-coh789',
          OPENAI_API_KEY: 'sk-test123',
          ...Object.fromEntries(Array.from({ length: 100 }, (_, i) => `VAR_${i}`).map((key) => [key, `value_${key}`])),
        }}
      >
        <Story />
      </JotaiStorybookProvider>
    ),
  ],
}
