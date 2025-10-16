import { atom, useAtom } from 'jotai';
import {
  createParser,
  parseAsBoolean,
  parseAsStringEnum,
  useQueryState,
} from 'nuqs';
import {
  type FunctionNames,
  type HookOutput,
  useClassifyMessage,
  useCompletion,
  useDescribeMedia1599,
  useExtractContactInfo,
  useStructureDocument1559,
} from '../../baml_client/react/hooks';
import type { HookInput } from '../../baml_client/react/hooks';

// Define the type for the example options

// Define the input component types
export type InputComponentType = 'text' | 'image' | 'textarea' | 'multiInput';

// Interface for a single input field configuration
export interface InputFieldConfig {
  type: InputComponentType;
  label: string;
  placeholder: string;
  key: string; // Identifier for the input when handling multiple inputs
  required?: boolean;
  accept?: string; // For file inputs like images, specify accepted formats
  maxSize?: number; // For file inputs, maximum size in bytes
}

// Interface for hook configuration
export interface HookConfig<T extends FunctionNames> {
  hook: (
    props: HookInput<T, { stream: any }>,
  ) => HookOutput<T, { stream: any }>; // Using any for simplicity to avoid complex type constraints
  inputLabel: string; // Legacy field, can be used as fallback
  inputPlaceholder: string; // Legacy field, can be used as fallback
  description: string;
  displayName: string;
  inputs: InputFieldConfig[]; // New field for specifying multiple/varied input components
}

// Create a mapping of example types to hooks
export const hookConfigMap: Partial<
  Record<FunctionNames, HookConfig<FunctionNames>>
> = {
  Completion: {
    hook: useCompletion,
    inputLabel: 'Ask',
    inputPlaceholder: 'Ask a question...',
    description: 'Chat with an AI assistant',
    displayName: 'Chat Interface',
    inputs: [
      {
        type: 'text',
        label: 'Ask',
        placeholder: 'Ask a question...',
        key: 'prompt',
        required: true,
      },
    ],
  },
  ClassifyMessage: {
    hook: useClassifyMessage,
    inputLabel: 'Classify',
    inputPlaceholder: 'Enter text to classify...',
    description: 'Classify text into categories',
    displayName: 'Text Classification',
    inputs: [
      {
        type: 'textarea',
        label: 'Classify',
        placeholder: 'Enter text to classify...',
        key: 'message',
        required: true,
      },
    ],
  },
  ExtractContactInfo: {
    hook: useExtractContactInfo,
    inputLabel: 'Extract from',
    inputPlaceholder: 'Enter text to extract data from...',
    description: 'Extract structured data from text',
    displayName: 'Data Extraction',
    inputs: [
      {
        type: 'textarea',
        label: 'Extract from',
        placeholder: 'Enter text to extract data from...',
        key: 'text',
        required: true,
      },
    ],
  },
  StructureDocument1559: {
    hook: useStructureDocument1559, // Using TestAws as fallback for summarization example
    inputLabel: 'Structure Document 1559',
    inputPlaceholder: 'Enter text to structure...',
    description: 'Structure text into a document',
    displayName: 'Structure Document 1559',
    inputs: [
      {
        type: 'textarea',
        label: 'Structure Document 1559',
        placeholder: 'Enter text to structure...',
        key: 'document',
        required: true,
      },
    ],
  },
  DescribeMedia1599: {
    hook: useDescribeMedia1599, // Using TestAws as fallback for summarization example
    inputLabel: 'Describe Media 1599',
    inputPlaceholder: 'Enter text to describe...',
    description: 'Describe media',
    displayName: 'Describe Media 1599',
    inputs: [
      {
        type: 'image',
        label: 'Upload Image',
        placeholder: 'Choose an image to describe...',
        key: 'img',
        required: true,
        accept: 'image/*',
        maxSize: 5242880, // 5MB
      },
      {
        type: 'text',
        label: 'Client Sector',
        placeholder: 'Enter client sector...',
        key: 'client_sector',
        required: true,
      },
      {
        type: 'text',
        label: 'Client Name',
        placeholder: 'Enter client name...',
        key: 'client_name',
        required: true,
      },
      {
        type: 'text',
        label: 'Additional Context',
        placeholder: 'Add any additional context (optional)...',
        key: 'context',
        required: false,
      },
    ],
  },
};

// Create a nuqs parser for the selected example
export const exampleParser = parseAsStringEnum<FunctionNames>(
  Object.keys(hookConfigMap) as FunctionNames[],
).withDefault('Completion');

// For backwards compatibility, keep the atom (can be removed after full migration)
export const selectedExampleAtom = atom<FunctionNames>('Completion');

// Helper function to get the selected example's display name
export function getExampleDisplayName(example: FunctionNames): string {
  return hookConfigMap[example]?.displayName || '';
}

// Helper function to get the hook config for the selected example
export function getHookConfig<T extends FunctionNames>(
  example: T | string | null,
): HookConfig<FunctionNames> {
  if (!example) {
    // Return a default hook config if no example is selected
    return hookConfigMap['Completion'] as HookConfig<FunctionNames>;
  }
  const exampleKey = example as FunctionNames;
  return (
    (hookConfigMap[exampleKey] as HookConfig<FunctionNames>) ||
    (hookConfigMap['Completion'] as HookConfig<FunctionNames>)
  );
}

// Helper function to get the input configurations for a specific example
export function getInputConfigs<T extends FunctionNames>(
  example: T | string | null,
): InputFieldConfig[] {
  if (!example) return [];
  // Ensure example is treated as a FunctionNames
  const exampleKey = example as FunctionNames;

  // Check if this example exists in the hook config map
  if (!hookConfigMap[exampleKey]) {
    console.warn(`No hook config found for example: ${exampleKey}`);
    return [];
  }

  return hookConfigMap[exampleKey]?.inputs || [];
}

// Helper function to get a single input field configuration (for backward compatibility)
export function getDefaultInputConfig<T extends FunctionNames>(
  example: T,
): InputFieldConfig | null {
  const inputs = getInputConfigs(example);
  return inputs.length > 0 ? inputs[0] ?? null : null;
}

// Define the OutputFormat type
export type OutputFormat = 'raw' | 'json' | 'yaml' | 'markdown';

// Interface for response card configuration
export interface ResponseCardConfig {
  showDataTab: boolean;
  showStreamDataTab: boolean;
  showFinalDataTab: boolean;
  showErrorTab: boolean;
  defaultTab: string; // Using string to match what the Tabs component expects
  displayMode: 'tabs' | 'sections'; // New option to toggle between tabs and sections views
  showNetworkTimeline: boolean; // New option to toggle the NetworkTimeline visibility
  isStreamingEnabled: boolean; // New option to toggle streaming responses
  outputFormat: OutputFormat; // Format to render response data in
}

// Default configuration for the response card
export const defaultResponseCardConfig: ResponseCardConfig = {
  showDataTab: true,
  showStreamDataTab: true,
  showFinalDataTab: true,
  showErrorTab: true,
  defaultTab: 'data',
  displayMode: 'tabs', // Default to tabs view
  showNetworkTimeline: true, // Default to showing the NetworkTimeline
  isStreamingEnabled: true, // Default to streaming enabled
  outputFormat: 'raw', // Default output format
};

// Create nuqs parsers for the response card configuration
export const showDataTabParser = parseAsBoolean.withDefault(
  defaultResponseCardConfig.showDataTab,
);
export const showStreamDataTabParser = parseAsBoolean.withDefault(
  defaultResponseCardConfig.showStreamDataTab,
);
export const showFinalDataTabParser = parseAsBoolean.withDefault(
  defaultResponseCardConfig.showFinalDataTab,
);
export const showErrorTabParser = parseAsBoolean.withDefault(
  defaultResponseCardConfig.showErrorTab,
);
export const showNetworkTimelineParser = parseAsBoolean.withDefault(
  defaultResponseCardConfig.showNetworkTimeline,
);
export const isStreamingEnabledParser = parseAsBoolean.withDefault(
  defaultResponseCardConfig.isStreamingEnabled,
);
export const defaultTabParser = createParser<string>({
  parse: (value) => value,
  serialize: (value) => value,
}).withDefault(defaultResponseCardConfig.defaultTab);
export const displayModeParser = parseAsStringEnum([
  'tabs',
  'sections',
] as const).withDefault(defaultResponseCardConfig.displayMode);
export const outputFormatParser = parseAsStringEnum<OutputFormat>([
  'raw',
  'json',
  'yaml',
  'markdown',
] as const).withDefault(defaultResponseCardConfig.outputFormat);

// Custom atom that syncs with nuqs query parameters but starts with our default settings
export const responseCardConfigAtom = atom<ResponseCardConfig>({
  showDataTab: false, // Default to not showing data tab
  showStreamDataTab: true,
  showFinalDataTab: true,
  showErrorTab: true,
  defaultTab: 'streamData',
  displayMode: 'sections',
  showNetworkTimeline: true,
  isStreamingEnabled: true,
  outputFormat: 'raw', // Default output format
});

// Helper function to get the response card configuration
export function getResponseCardConfig(): ResponseCardConfig {
  return responseCardConfigAtom.init;
}

// Create an atom to update response card configuration
export const responseCardConfigUpdaterAtom = atom(
  null,
  (get, set, config: Partial<ResponseCardConfig>) => {
    const currentConfig = get(responseCardConfigAtom);

    // Set the updated configuration
    set(responseCardConfigAtom, {
      ...currentConfig,
      ...config,
    });
  },
);

// Hook to connect the Jotai atom with nuqs query parameters
export function useResponseCardConfigWithQueryParams() {
  // Get the current config from the atom
  const [configFromAtom, setConfigAtom] = useAtom(responseCardConfigAtom);

  // Define query parameters with nuqs
  const [showDataTab, setShowDataTab] = useQueryState(
    'showDataTab',
    showDataTabParser,
  );
  const [showStreamDataTab, setShowStreamDataTab] = useQueryState(
    'showStreamDataTab',
    showStreamDataTabParser,
  );
  const [showFinalDataTab, setShowFinalDataTab] = useQueryState(
    'showFinalDataTab',
    showFinalDataTabParser,
  );
  const [showErrorTab, setShowErrorTab] = useQueryState(
    'showErrorTab',
    showErrorTabParser,
  );
  const [showNetworkTimeline, setShowNetworkTimeline] = useQueryState(
    'showNetworkTimeline',
    showNetworkTimelineParser,
  );
  const [isStreamingEnabled, setIsStreamingEnabled] = useQueryState(
    'isStreamingEnabled',
    isStreamingEnabledParser,
  );
  const [defaultTab, setDefaultTab] = useQueryState(
    'defaultTab',
    defaultTabParser,
  );
  const [displayMode, setDisplayMode] = useQueryState(
    'displayMode',
    displayModeParser,
  );
  const [outputFormat, setOutputFormat] = useQueryState(
    'outputFormat',
    outputFormatParser,
  );

  // Helper function to update both the atom and query params
  const updateConfig = (newConfig: Partial<ResponseCardConfig>) => {
    // Update the atom
    setConfigAtom((prev: ResponseCardConfig) => ({
      ...prev,
      ...newConfig,
    }));

    // Update the query parameters for each changed field
    if (newConfig.showDataTab !== undefined)
      setShowDataTab(newConfig.showDataTab);
    if (newConfig.showStreamDataTab !== undefined)
      setShowStreamDataTab(newConfig.showStreamDataTab);
    if (newConfig.showFinalDataTab !== undefined)
      setShowFinalDataTab(newConfig.showFinalDataTab);
    if (newConfig.showErrorTab !== undefined)
      setShowErrorTab(newConfig.showErrorTab);
    if (newConfig.showNetworkTimeline !== undefined)
      setShowNetworkTimeline(newConfig.showNetworkTimeline);
    if (newConfig.isStreamingEnabled !== undefined)
      setIsStreamingEnabled(newConfig.isStreamingEnabled);
    if (newConfig.defaultTab !== undefined) setDefaultTab(newConfig.defaultTab);
    if (newConfig.displayMode !== undefined)
      setDisplayMode(newConfig.displayMode as 'tabs' | 'sections');
    if (newConfig.outputFormat !== undefined)
      setOutputFormat(newConfig.outputFormat as OutputFormat);
  };

  // Get the full config from query params, falling back to atom defaults
  const fullConfig: ResponseCardConfig = {
    showDataTab: showDataTab ?? configFromAtom.showDataTab,
    showStreamDataTab: showStreamDataTab ?? configFromAtom.showStreamDataTab,
    showFinalDataTab: showFinalDataTab ?? configFromAtom.showFinalDataTab,
    showErrorTab: showErrorTab ?? configFromAtom.showErrorTab,
    showNetworkTimeline:
      showNetworkTimeline ?? configFromAtom.showNetworkTimeline,
    isStreamingEnabled: isStreamingEnabled ?? configFromAtom.isStreamingEnabled,
    defaultTab: defaultTab ?? configFromAtom.defaultTab,
    displayMode: displayMode ?? configFromAtom.displayMode,
    outputFormat: outputFormat ?? configFromAtom.outputFormat,
  };

  return { config: fullConfig, updateConfig };
}
