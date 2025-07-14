import { describe, it, expect, vi } from 'vitest';
import os from 'node:os';
import {
  getReleaseArchitecture,
  getReleasePlatform,
  getTargetTriple,
  getCliCompressedFileExtension,
  getExecutableName,
} from '../platform';

vi.mock('node:os');
vi.mock('node:fs');

describe('platform', () => {
  describe('getReleaseArchitecture', () => {
    it('should convert arm64 to aarch64', () => {
      expect(getReleaseArchitecture('arm64')).toBe('aarch64');
    });

    it('should convert x64 to x86_64', () => {
      expect(getReleaseArchitecture('x64')).toBe('x86_64');
    });

    it('should return unknown architectures as-is', () => {
      expect(getReleaseArchitecture('unknown-arch')).toBe('unknown-arch');
    });
  });

  describe('getReleasePlatform', () => {
    it('should convert win32 to pc-windows-msvc', () => {
      expect(getReleasePlatform('win32')).toBe('pc-windows-msvc');
    });

    it('should convert darwin to apple-darwin', () => {
      expect(getReleasePlatform('darwin')).toBe('apple-darwin');
    });

    it('should return unknown platforms as-is', () => {
      expect(getReleasePlatform('unknown-platform')).toBe('unknown-platform');
    });
  });

  describe('getTargetTriple', () => {
    it('should combine architecture and platform correctly', () => {
      vi.mocked(os.platform).mockReturnValue('darwin' as any);
      vi.mocked(os.arch).mockReturnValue('arm64' as any);
      
      expect(getTargetTriple()).toBe('aarch64-apple-darwin');
    });

    it('should handle Windows platform', () => {
      vi.mocked(os.platform).mockReturnValue('win32' as any);
      vi.mocked(os.arch).mockReturnValue('x64' as any);
      
      expect(getTargetTriple()).toBe('x86_64-pc-windows-msvc');
    });
  });

  describe('getCliCompressedFileExtension', () => {
    it('should return zip for Windows', () => {
      expect(getCliCompressedFileExtension('win32')).toBe('zip');
    });

    it('should return tar.gz for macOS', () => {
      expect(getCliCompressedFileExtension('darwin')).toBe('tar.gz');
    });

    it('should return tar.gz for Linux', () => {
      expect(getCliCompressedFileExtension('linux')).toBe('tar.gz');
    });
  });

  describe('getExecutableName', () => {
    it('should return baml-cli.exe for Windows', () => {
      vi.mocked(os.platform).mockReturnValue('win32' as any);
      expect(getExecutableName()).toBe('baml-cli.exe');
    });

    it('should return baml-cli for non-Windows', () => {
      vi.mocked(os.platform).mockReturnValue('darwin' as any);
      expect(getExecutableName()).toBe('baml-cli');
    });
  });
});