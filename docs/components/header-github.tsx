'use client';

import { createContext, type ReactNode, useContext } from 'react';

const HeaderGitHubContext = createContext<ReactNode>(null);

export function HeaderGitHubProvider({ children, link }: { children: ReactNode; link: ReactNode }) {
  return <HeaderGitHubContext.Provider value={link}>{children}</HeaderGitHubContext.Provider>;
}

export function useHeaderGitHub() {
  return useContext(HeaderGitHubContext);
}
