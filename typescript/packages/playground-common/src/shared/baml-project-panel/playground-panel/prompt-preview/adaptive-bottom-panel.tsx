'use client';

import { useAtomValue } from 'jotai';
import { bottomPanelModeAtom } from '../atoms';
import { TestPanel } from './test-panel';
import { ExecutionLogPanel } from '../../../../features/detail-panel';

/**
 * AdaptiveBottomPanel - Switches between TestPanel and ExecutionLogPanel
 *
 * This component automatically switches between:
 * - TestPanel: For Preview/cURL tabs (showing test results, parsed responses)
 * - ExecutionLogPanel: For Graph tab (showing chronological execution timeline)
 */
export const AdaptiveBottomPanel = () => {
  const bottomPanelMode = useAtomValue(bottomPanelModeAtom);
  console.log('viewtype: bottomPanelMode', bottomPanelMode);
  if (bottomPanelMode === 'detail-panel') {
    return <ExecutionLogPanel />;
  }

  return <TestPanel />;
};
