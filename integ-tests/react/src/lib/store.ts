import { atom } from 'jotai';
import { parseAsStringEnum } from 'nuqs';
import {
  useTestAws,
  useClassifyMessage,
  useExtractContactInfo,
  useCompletion,
  useDescribeMedia1599,
  FunctionNames,
  useStructureDocument1559,
  HookOutput,
} from '../../baml_client/react/hooks';
import { HookInput} from '../../baml_client/react/hooks';

// Define the type for the example options

// Interface for hook configuration
export interface HookConfig<T extends FunctionNames> {
  hook: (props: HookInput<T, { stream: any }>) => HookOutput<T, { stream: any }>; // Using any for simplicity to avoid complex type constraints
  inputLabel: string;
  inputPlaceholder: string;
  description: string;
  displayName: string;
}

// Create a mapping of example types to hooks
export const hookConfigMap: Partial<Record<FunctionNames, HookConfig<FunctionNames>>> = {
  Completion: {
    hook: useCompletion,
    inputLabel: 'Ask',
    inputPlaceholder: 'Ask a question...',
    description: 'Chat with an AI assistant',
    displayName: 'Chat Interface',
  },
  ClassifyMessage: {
    hook: useClassifyMessage,
    inputLabel: 'Classify',
    inputPlaceholder: 'Enter text to classify...',
    description: 'Classify text into categories',
    displayName: 'Text Classification',
  },
  ExtractContactInfo: {
    hook: useExtractContactInfo,
    inputLabel: 'Extract from',
    inputPlaceholder: 'Enter text to extract data from...',
    description: 'Extract structured data from text',
    displayName: 'Data Extraction',
  },
  StructureDocument1559: {
    hook: useStructureDocument1559, // Using TestAws as fallback for summarization example
    inputLabel: 'Structure Document 1559',
    inputPlaceholder: 'Enter text to structure...',
    description: 'Structure text into a document',
    displayName: 'Structure Document 1559',
  },
  DescribeMedia1599: {
    hook: useDescribeMedia1599, // Using TestAws as fallback for summarization example
    inputLabel: 'Describe Media 1599',
    inputPlaceholder: 'Enter text to describe...',
    description: 'Describe media',
    displayName: 'Describe Media 1599',
  },
};

// Create a nuqs parser for the selected example
export const exampleParser = parseAsStringEnum<FunctionNames>(
  Object.keys(hookConfigMap) as FunctionNames[]
).withDefault('Completion');

// For backwards compatibility, keep the atom (can be removed after full migration)
export const selectedExampleAtom = atom<FunctionNames>('Completion');

// Helper function to get the selected example's display name
export function getExampleDisplayName(example: FunctionNames): string {
  return hookConfigMap[example]?.displayName || '';
}

// Helper function to get the hook config for the selected example
export function getHookConfig<T extends FunctionNames>(example: T): HookConfig<T> {
  return hookConfigMap[example] as HookConfig<T>;
}
