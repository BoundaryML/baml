import type { OutputChannel } from 'vscode';
import { BACKOFF_CONSTANTS } from './constants';
import type { BackoffState } from './types';

/**
 * Manages download backoff state to prevent excessive retry attempts
 */
export class BackoffManager {
  private downloadBackoffState = new Map<string, BackoffState>();
  private downloadsInProgress = new Set<string>();

  /**
   * Checks if download should be attempted based on backoff state
   */
  shouldAttemptDownload(
    version: string,
    outputChannel: OutputChannel,
  ): boolean {
    // Check if download is already in progress
    if (this.downloadsInProgress.has(version)) {
      outputChannel.appendLine(
        `Download for version ${version} is already in progress. Skipping duplicate request.`,
      );
      return false;
    }

    // Check backoff state
    const backoffInfo = this.downloadBackoffState.get(version);
    if (backoffInfo) {
      const { failureCount, lastAttemptTimestamp } = backoffInfo;
      const backoffDelay = Math.min(
        BACKOFF_CONSTANTS.INITIAL_DELAY_MS * 2 ** (failureCount - 1),
        BACKOFF_CONSTANTS.MAX_DELAY_MS,
      );
      const nextAttemptTime = lastAttemptTimestamp + backoffDelay;

      if (Date.now() < nextAttemptTime) {
        const waitTimeMinutes = Math.ceil(
          (nextAttemptTime - Date.now()) / (60 * 1000),
        );
        outputChannel.appendLine(
          `Download for version ${version} failed previously (${failureCount} times). Backoff active. Will not attempt download for another ${waitTimeMinutes} minutes.`,
        );
        return false;
      }
      outputChannel.appendLine(
        `Backoff period for version ${version} has elapsed. Proceeding with download attempt.`,
      );
    }

    return true;
  }

  /**
   * Marks a download as in progress
   */
  markDownloadStarted(version: string): void {
    this.downloadsInProgress.add(version);
  }

  /**
   * Marks a download as completed (success or failure)
   */
  markDownloadCompleted(version: string): void {
    this.downloadsInProgress.delete(version);
  }

  /**
   * Records a download failure and updates backoff state
   */
  recordFailure(version: string, outputChannel: OutputChannel): void {
    const now = Date.now();
    const state = this.downloadBackoffState.get(version) ?? {
      failureCount: 0,
      lastAttemptTimestamp: 0,
    };

    state.failureCount = state.failureCount + 1;
    state.lastAttemptTimestamp = now;

    // Reset failure count if it gets too high
    if (state.failureCount > BACKOFF_CONSTANTS.MAX_FAILURE_COUNT_BEFORE_RESET) {
      outputChannel.appendLine(
        `Download failure count for ${version} reached ${state.failureCount}. Resetting count but maintaining backoff timestamp.`,
      );
      state.failureCount = 1;
    }

    this.downloadBackoffState.set(version, state);
    outputChannel.appendLine(
      `Updated download backoff state for version ${version}: count=${state.failureCount}, lastAttempt=${new Date(
        state.lastAttemptTimestamp,
      ).toISOString()}`,
    );
  }

  /**
   * Clears backoff state for a successful download
   */
  clearBackoff(version: string): void {
    this.downloadBackoffState.delete(version);
  }
}
