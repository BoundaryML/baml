// Watch notification structure from WASM
export interface WatchNotification {
  variable_name?: string;     // Variable being watched
  channel_name?: string;       // Channel name if different from variable
  block_name?: string;       // Derived block label (populated client-side)
  function_name?: string;      // Function context from wasm
  test_name?: string;          // Optional test identifier from wasm (parallel runs)
  is_stream: boolean;          // Whether this is a streaming notification
  value: string;               // Debug-formatted value string
}

// Watch handler function type
export type WatchHandler = (notification: WatchNotification) => void;

// Categorized notifications for UI display
export interface CategorizedNotifications {
  variables: WatchNotification[];
  blocks: WatchNotification[];
  streams: WatchNotification[];
}
