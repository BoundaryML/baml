'use client';

import { createStore } from 'jotai';
import { Provider } from 'jotai/react';
// import { Provider, createStore } from 'jotai'
// import { DevTools } from 'jotai-devtools'
import { createJSONStorage } from 'jotai/utils';
import type { SyncStorage } from 'jotai/vanilla/utils/atomWithStorage';
// import 'jotai-devtools/styles.css'
import { vscode } from '../shared/baml-project-panel/vscode';
// import { vscode } from '../../../../../playground-common/src/shared/baml-project-panel/vscode'

export const atomStore: ReturnType<typeof createStore> = createStore();

function setVSCodeState(state: any) {
  vscode.setState(state);
}
function getLocalStorage() {
  const state = vscode.getState() || { localStorage: {} };
  return (state as any).localStorage;
}

// pollyfill for localStorage
if (typeof window !== 'undefined' && !window.localStorage) {
  Object.defineProperty(window, 'localStorage', {
    value: {
      setItem(key: string, value: string) {
        const localStorage = getLocalStorage();
        setVSCodeState({ localStorage: { ...localStorage, [key]: value } });
      },
      getItem(key: string) {
        return getLocalStorage()[key] || null;
      },
      removeItem(key: string) {
        const localStorage = getLocalStorage();
        delete localStorage[key];
        setVSCodeState({ localStorage });
      },
      clear() {
        setVSCodeState({ localStorage: {} });
      },
      key(index: number) {
        const keys = Object.keys(getLocalStorage());
        return keys[index] || null;
      },
      get length() {
        return Object.keys(getLocalStorage()).length;
      },
    },
    writable: false,
  });
}

export const vscodeLocalStorageStore: SyncStorage<any> = createJSONStorage(
  () => window.localStorage,
);
// export const persistentVSCodeStore: SyncStorage<any> = createJSONStorage(() => ({
//   getItem: (key: string) => {
//     if (vscodeAPI()) {
//       return vscodeAPI()?.getState()
//     }

//     const state = localStorage.getItem('vscodeState')
//     return state ? JSON.parse(state)[key] : undefined
//   },
//   setItem: (key: string, newValue: string) => {
//     if (vscodeAPI()) {
//       vscodeAPI()?.setState(newValue)
//       return newValue
//     }
//     localStorage.setItem('vscodeState', JSON.stringify(newValue))
//     return newState
//   },
//   removeItem(key) {
//     localStorage.removeItem(key)
//   },
// }))
export const sessionStore: SyncStorage<any> = createJSONStorage(
  () => sessionStorage,
);

export function JotaiProvider({ children }: { children: React.ReactNode }) {
  return (
    <Provider store={atomStore}>
      {/* <DevTools store={atomStore} /> */}
      {children}
    </Provider>
  );
}
