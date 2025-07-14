export const BASE_URL = 'https://github.com/BoundaryML/baml/releases/download';

export const BACKOFF_CONSTANTS = {
  INITIAL_DELAY_MS: 10 * 60 * 1000, // 10 minutes
  MAX_DELAY_MS: 60 * 60 * 1000, // 1 hour
  MAX_FAILURE_COUNT_BEFORE_RESET: 5,
} as const;

export const DOWNLOAD_TIMEOUT = {
  BINARY: 60000, // 60 seconds
  CHECKSUM: 10000, // 10 seconds
} as const;