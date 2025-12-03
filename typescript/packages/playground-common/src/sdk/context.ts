import { createContext } from 'react';
import type { BAMLSDK } from './sdk';

// Separate file for context to avoid Fast Refresh issues
// (React components and non-components shouldn't be exported from the same file)
export const BAMLSDKContext = createContext<BAMLSDK | null>(null);
