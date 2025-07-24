import { useAtomValue } from 'jotai';
import { memo } from 'react';
import { isClientCallGraphEnabledAtom } from '../../preview-toolbar';
import { ClientGraphView } from './components/ClientGraphView';
import { TestPanelContent } from './components/test-panel-content';

export const TestPanel = memo(() => {
  const isClientCallGraphEnabled = useAtomValue(isClientCallGraphEnabledAtom);

  if (isClientCallGraphEnabled) {
    return <ClientGraphView />;
  }

  return <TestPanelContent />;
});

TestPanel.displayName = 'TestPanel';
