import React from 'react';
import { AskBamlSidePanel, ChatProvider } from '../components/AskBaml';

interface RootProps {
  children: React.ReactNode;
}

export default function Root({ children }: RootProps) {
  return (
    <ChatProvider>
      {children}
      <AskBamlSidePanel />
    </ChatProvider>
  );
}
