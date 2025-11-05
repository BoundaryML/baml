import { useAtomValue } from 'jotai';
import { wasmPanicAtom, useClearWasmPanic } from './atoms';

/**
 * Example component that displays WASM panic notifications.
 * Add this to your app's root component to show panic messages to users.
 */
export const WasmPanicNotification = () => {
  const panicState = useAtomValue(wasmPanicAtom);
  const clearPanic = useClearWasmPanic();

  if (!panicState) {
    return null;
  }

  // Note we probably have too many panics at some point with the wasm atom being unreachable,
  // so for now lets not render this one until we know we don't panic often/if at all.
  return null;

  // return (
  //   <div
  //     style={{
  //       position: 'fixed',
  //       top: 16,
  //       right: 16,
  //       maxWidth: 500,
  //       padding: 16,
  //       backgroundColor: '#dc2626',
  //       color: 'white',
  //       borderRadius: 8,
  //       boxShadow: '0 4px 6px rgba(0, 0, 0, 0.1)',
  //       zIndex: 9999,
  //     }}
  //   >
  //     <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'start' }}>
  //       <div style={{ flex: 1 }}>
  //         <h3 style={{ margin: '0 0 8px 0', fontSize: 16, fontWeight: 600 }}>
  //           WASM Runtime Panic
  //         </h3>
  //         <p style={{ margin: '0 0 8px 0', fontSize: 14, fontFamily: 'monospace' }}>
  //           {panicState.msg}
  //         </p>
  //         <p style={{ margin: 0, fontSize: 12, opacity: 0.9 }}>
  //           Time: {new Date(panicState.timestamp).toLocaleTimeString()}
  //         </p>
  //       </div>
  //       <button
  //         onClick={clearPanic}
  //         style={{
  //           marginLeft: 12,
  //           padding: '4px 8px',
  //           backgroundColor: 'rgba(255, 255, 255, 0.2)',
  //           border: 'none',
  //           borderRadius: 4,
  //           color: 'white',
  //           cursor: 'pointer',
  //           fontSize: 14,
  //         }}
  //       >
  //         Dismiss
  //       </button>
  //     </div>
  //   </div>
  // );
};
