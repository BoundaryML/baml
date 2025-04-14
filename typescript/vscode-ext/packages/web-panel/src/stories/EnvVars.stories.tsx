import { expect } from '@storybook/test'
import { DevTools } from 'jotai-devtools'
import 'jotai-devtools/styles.css'
import { atom, createStore, useAtomValue, useSetAtom } from 'jotai'
import { default as EnvVars } from '../shared/baml-project-panel/playground-panel/side-bar/env-vars'
import { Provider as JotaiProvider } from 'jotai'
import { ThemeProvider } from 'next-themes'
import '../App.css'
import { envVarsAtom } from '../shared/baml-project-panel/atoms'
import { useState } from 'react'

interface JotaiProviderProps {
  envVars: Record<string, string>
  children: React.ReactNode
}

const JotaiStorybookProvider: React.FC<JotaiProviderProps> = ({ envVars, children }) => {
  const storybookStore = createStore()
  storybookStore.set(envVarsAtom, envVars)
  return <JotaiProvider store={storybookStore}>{children}</JotaiProvider>
}

const WrappedEnvVars: React.FC = () => {
  const envVars = useAtomValue(envVarsAtom)
  return (
    <div>
      <ThemeProvider attribute='class' defaultTheme='dark' enableSystem={false} disableTransitionOnChange={true}>
        <div className='flex gap-8 items-start'>
          <EnvVars />
          <div className='p-4 bg-[#1e1e1e] rounded-lg min-w-[300px]'>
            <h3 className='mb-2 text-sm font-mono'>JSON.stringify(useAtomValue(envVarsAtom))</h3>
            <pre className='text-xs'>{JSON.stringify(envVars, null, 2)}</pre>
          </div>
        </div>
      </ThemeProvider>
    </div>
  )
}

export default {
  title: 'EnvVars',
  component: WrappedEnvVars,
  decorators: [],
  parameters: {
    // More on how to position stories at: https://storybook.js.org/docs/configure/story-layout
    layout: 'centered',
  },
}

// More on component testing: https://storybook.js.org/docs/writing-tests/component-testing
export const NoRequiredEnvVarsAreSet = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider envVars={{}}>
        <div>
          <ThemeProvider attribute='class' defaultTheme='dark' enableSystem={false} disableTransitionOnChange={true}>
            <div className='flex gap-8 items-start'>
              <Story />
            </div>
          </ThemeProvider>
        </div>
      </JotaiStorybookProvider>
    ),
  ],
}

// ANTHROPIC_API_KEY and OPENAI_API_KEY are required by default
export const SomeRequiredEnvVarsAreSet = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider
        envVars={{
          ANTHROPIC_API_KEY: 'sk-ant456',
          COHERE_API_KEY: 'sk-coh789',
          OPENAI_API_KEY: '',
        }}
      >
        <div>
          <ThemeProvider attribute='class' defaultTheme='dark' enableSystem={false} disableTransitionOnChange={true}>
            <div className='flex gap-8 items-start'>
              <Story />
            </div>
          </ThemeProvider>
        </div>
      </JotaiStorybookProvider>
    ),
  ],
}

// ANTHROPIC_API_KEY and OPENAI_API_KEY are required by default
export const AllRequiredEnvVarsAreSet = {
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

export const EnvVarContainsNewlines = {
  decorators: [
    (Story: React.FC) => (
      <JotaiStorybookProvider
        envVars={{
          ANTHROPIC_API_KEY: 'line1\nline2\nline3',
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
export const TableWith100EnvVars = {
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
