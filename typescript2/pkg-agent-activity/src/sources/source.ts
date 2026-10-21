import type { ServerMessage } from '../engine/protocol';

export type ConnectionStatus = 'connecting' | 'open' | 'closed';

/** Where activity comes from: a `baml agent watch` server or a recorded scenario. */
export interface ActivitySource {
  readonly kind: 'live' | 'mock';
  start(
    onMessage: (message: ServerMessage) => void,
    onStatus: (status: ConnectionStatus) => void,
  ): void;
  stop(): void;
  /** The session clock in Unix milliseconds. A replayed scenario runs its own clock. */
  now(): number;
  /** Whether the session clock is currently running faster than real time. */
  fastForwarding(): boolean;
  setSpeed(speed: number): void;
}
