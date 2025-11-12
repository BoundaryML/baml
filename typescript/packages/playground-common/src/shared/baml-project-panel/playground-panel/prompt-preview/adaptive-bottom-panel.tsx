'use client';

import { useAtomValue } from 'jotai';
import { bottomPanelModeAtom } from '../atoms';
import { TestPanel } from './test-panel';
import { DetailPanel } from '../../../../features/detail-panel';

/**
 * AdaptiveBottomPanel - Switches between TestPanel and DetailPanel
 *
 * This component automatically switches between:
 * - TestPanel: For Preview/cURL tabs (showing test results, parsed responses)
 * - DetailPanel: For Graph tab (showing selected node I/O, execution data)
 */
export const AdaptiveBottomPanel = () => {
  const bottomPanelMode = useAtomValue(bottomPanelModeAtom);
  console.log('viewtype: bottomPanelMode', bottomPanelMode);
  if (bottomPanelMode === 'detail-panel') {
    return <DetailPanel />;
  }

  return <TestPanel />;
};
