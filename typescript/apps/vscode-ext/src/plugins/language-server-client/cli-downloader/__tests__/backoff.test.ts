import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { OutputChannel } from 'vscode';
import { BackoffManager } from '../backoff';

describe('BackoffManager', () => {
  let backoffManager: BackoffManager;
  let mockOutputChannel: OutputChannel;

  beforeEach(() => {
    backoffManager = new BackoffManager();
    mockOutputChannel = {
      appendLine: vi.fn(),
    } as any;
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('shouldAttemptDownload', () => {
    it('should allow download when no previous failures', () => {
      const result = backoffManager.shouldAttemptDownload(
        '1.0.0',
        mockOutputChannel,
      );
      expect(result).toBe(true);
    });

    it('should prevent download when already in progress', () => {
      backoffManager.markDownloadStarted('1.0.0');
      const result = backoffManager.shouldAttemptDownload(
        '1.0.0',
        mockOutputChannel,
      );
      expect(result).toBe(false);
      expect(mockOutputChannel.appendLine).toHaveBeenCalledWith(
        expect.stringContaining('already in progress'),
      );
    });

    it('should enforce backoff after failure', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      backoffManager.recordFailure('1.0.0', mockOutputChannel);

      // Should not allow immediate retry
      const result1 = backoffManager.shouldAttemptDownload(
        '1.0.0',
        mockOutputChannel,
      );
      expect(result1).toBe(false);

      // Should allow after backoff period
      vi.setSystemTime(now + 11 * 60 * 1000); // 11 minutes later
      const result2 = backoffManager.shouldAttemptDownload(
        '1.0.0',
        mockOutputChannel,
      );
      expect(result2).toBe(true);
    });

    it('should increase backoff time with multiple failures', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      // First failure - 10 minute backoff
      backoffManager.recordFailure('1.0.0', mockOutputChannel);

      vi.setSystemTime(now + 11 * 60 * 1000);
      backoffManager.shouldAttemptDownload('1.0.0', mockOutputChannel);

      // Second failure - 20 minute backoff
      backoffManager.recordFailure('1.0.0', mockOutputChannel);

      vi.setSystemTime(now + 11 * 60 * 1000 + 15 * 60 * 1000); // 15 minutes after second failure
      expect(
        backoffManager.shouldAttemptDownload('1.0.0', mockOutputChannel),
      ).toBe(false);

      vi.setSystemTime(now + 11 * 60 * 1000 + 21 * 60 * 1000); // 21 minutes after second failure
      expect(
        backoffManager.shouldAttemptDownload('1.0.0', mockOutputChannel),
      ).toBe(true);
    });
  });

  describe('download lifecycle', () => {
    it('should track download lifecycle correctly', () => {
      // Start download
      expect(
        backoffManager.shouldAttemptDownload('1.0.0', mockOutputChannel),
      ).toBe(true);
      backoffManager.markDownloadStarted('1.0.0');

      // Cannot start another while in progress
      expect(
        backoffManager.shouldAttemptDownload('1.0.0', mockOutputChannel),
      ).toBe(false);

      // Complete download
      backoffManager.markDownloadCompleted('1.0.0');

      // Can start again
      expect(
        backoffManager.shouldAttemptDownload('1.0.0', mockOutputChannel),
      ).toBe(true);
    });
  });

  describe('clearBackoff', () => {
    it('should reset backoff state on success', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      backoffManager.recordFailure('1.0.0', mockOutputChannel);
      expect(
        backoffManager.shouldAttemptDownload('1.0.0', mockOutputChannel),
      ).toBe(false);

      backoffManager.clearBackoff('1.0.0');
      expect(
        backoffManager.shouldAttemptDownload('1.0.0', mockOutputChannel),
      ).toBe(true);
    });
  });

  describe('failure count reset', () => {
    it('should reset failure count after max failures', () => {
      const now = Date.now();
      vi.setSystemTime(now);

      // Record max failures
      for (let i = 0; i < 6; i++) {
        backoffManager.recordFailure('1.0.0', mockOutputChannel);
        vi.setSystemTime(now + (i + 1) * 70 * 60 * 1000); // Advance time past backoff
      }

      expect(mockOutputChannel.appendLine).toHaveBeenCalledWith(
        expect.stringContaining(
          'Resetting count but maintaining backoff timestamp',
        ),
      );
    });
  });
});
