// Mock implementation of VSCode API for tests
import { vi } from 'vitest';

export const window = {
  showInformationMessage: vi.fn(),
  showErrorMessage: vi.fn(),
  withProgress: vi.fn(),
};

export const ProgressLocation = {
  Notification: 15,
};

export interface ExtensionContext {
  asAbsolutePath: (relativePath: string) => string;
}

export interface OutputChannel {
  appendLine: (value: string) => void;
}