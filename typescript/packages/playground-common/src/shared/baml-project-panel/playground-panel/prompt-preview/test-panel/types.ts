// Watch notification structure from WASM
export interface WatchNotification {
  variable_name?: string;     // Variable being watched
  channel_name?: string;       // Channel name if different from variable
  function_name: string;       // Function context
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