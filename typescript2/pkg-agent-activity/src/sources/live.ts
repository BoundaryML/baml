import {
  AGENT_ACTIVITY_PROTOCOL,
  type ServerMessage,
} from '../engine/protocol';
import type { ActivitySource, ConnectionStatus } from './source';

/** Streams activity from `baml agent watch` over its WebSocket. */
export class LiveSource implements ActivitySource {
  readonly kind = 'live' as const;
  private socket: WebSocket | null = null;
  private retry: ReturnType<typeof setTimeout> | null = null;
  private attempts = 0;
  private stopped = false;

  constructor(private url: string) {}

  start(
    onMessage: (message: ServerMessage) => void,
    onStatus: (status: ConnectionStatus) => void,
  ) {
    this.stopped = false;
    const connect = () => {
      onStatus('connecting');
      const socket = new WebSocket(this.url);
      this.socket = socket;
      // Some servers (a dev server without the watch backend, a proxy) leave
      // the upgrade hanging rather than refusing it.
      const timeout = setTimeout(() => socket.close(), 3000);
      socket.onopen = () => {
        clearTimeout(timeout);
        this.attempts = 0;
        onStatus('open');
      };
      socket.onmessage = (event) => {
        let message: ServerMessage;
        try {
          message = JSON.parse(String(event.data));
        } catch {
          return;
        }
        if (
          message.type === 'hello' &&
          message.protocol !== AGENT_ACTIVITY_PROTOCOL
        ) {
          console.error(
            `agent activity: server speaks protocol ${message.protocol}, this page speaks ${AGENT_ACTIVITY_PROTOCOL}`,
          );
          socket.close();
          return;
        }
        onMessage(message);
      };
      socket.onclose = () => {
        clearTimeout(timeout);
        this.socket = null;
        onStatus('closed');
        if (this.stopped) return;
        const delay = Math.min(8000, 400 * 2 ** this.attempts++);
        this.retry = setTimeout(connect, delay);
      };
    };
    connect();
  }

  stop() {
    this.stopped = true;
    if (this.retry) clearTimeout(this.retry);
    this.socket?.close();
  }

  now() {
    return Date.now();
  }

  fastForwarding() {
    return false;
  }

  setSpeed() {}
}

export function defaultLiveUrl(): string {
  const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
  return `${scheme}://${window.location.host}/api/agent-activity/ws`;
}
