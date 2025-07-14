/* eslint-disable @typescript-eslint/no-unsafe-call */
/* eslint-disable @typescript-eslint/no-unsafe-assignment */
'use client';

import { Provider, type createStore } from 'jotai';
import { createJSONStorage } from 'jotai/utils';
import type { SyncStorage } from 'jotai/vanilla/utils/atomWithStorage';

// export const atomStore = createStore();

// export const vscodeLocalStorageStore: SyncStorage<any> = createJSONStorage(
//   () => window.localStorage
// );
// export const sessionStore: SyncStorage<any> = createJSONStorage(
//   () => sessionStorage
// );

export const vscodeLocalStorageStore: SyncStorage<any> = createJSONStorage(
  () => window.localStorage,
);

export default function JotaiProvider({
  children,
  store,
}: {
  children: React.ReactNode;
  store?: ReturnType<typeof createStore>;
}) {
  return <Provider store={store}>{children}</Provider>;
}
