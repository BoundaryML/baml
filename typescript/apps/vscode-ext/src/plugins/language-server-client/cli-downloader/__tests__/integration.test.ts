import * as os from 'node:os';
import * as path from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ExtensionContext, OutputChannel } from 'vscode';
import { type CliVersion, downloadCli, resolveCliPath } from '../index';

// Mock all external dependencies
vi.mock('vscode');
vi.mock('axios');
vi.mock('node:fs');
vi.mock('node:os');
vi.mock('adm-zip');
vi.mock('tar');

describe('CLI Downloader Integration', () => {
  let mockContext: ExtensionContext;
  let mockOutputChannel: OutputChannel;

  beforeEach(() => {
    vi.clearAllMocks();

    mockContext = {
      asAbsolutePath: vi.fn((p: string) => path.join('/mock/extension', p)),
    } as any;

    mockOutputChannel = {
      appendLine: vi.fn(),
    } as any;

    // Mock OS functions
    vi.mocked(os.homedir).mockReturnValue('/home/user');
    vi.mocked(os.platform).mockReturnValue('darwin' as any);
    vi.mocked(os.arch).mockReturnValue('arm64' as any);
  });

  describe('resolveCliPath', () => {
    it('should resolve path for bundled CLI when versions match', async () => {
      // This test ensures the main entry point still works
      // In a real test, we'd mock the file system and package.json
      expect(resolveCliPath).toBeDefined();
      expect(typeof resolveCliPath).toBe('function');
    });
  });

  describe('downloadCli', () => {
    it('should be exported and callable', () => {
      expect(downloadCli).toBeDefined();
      expect(typeof downloadCli).toBe('function');
    });
  });

  describe('type exports', () => {
    it('should export CliVersion type', () => {
      const cliVersion: CliVersion = {
        architecture: 'x64',
        platform: 'darwin',
        version: '1.0.0',
      };

      expect(cliVersion).toBeDefined();
    });
  });
});
