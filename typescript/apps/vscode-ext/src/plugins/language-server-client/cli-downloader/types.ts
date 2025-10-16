export type CliVersion = {
  architecture: string;
  platform: string;
  version: string;
};

export type BackoffState = {
  failureCount: number;
  lastAttemptTimestamp: number;
};