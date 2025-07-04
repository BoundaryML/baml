import { useAtomValue } from 'jotai';
import { memo } from 'react';
import { testHistoryAtom } from '../atoms';
import { testPanelViewTypeAtom } from './atoms';
import { SafeErrorBoundary } from './ui';
import { EmptyTestState } from './ui';
import { HistoryCard } from './test-history';
import { TestMenu } from './TestMenu';

export const TestPanelContent = memo(() => {
  const testHistory = useAtomValue(testHistoryAtom);
  const viewType = useAtomValue(testPanelViewTypeAtom);

  if (testHistory.length === 0) {
    return (
      <>
        <div className="px-1 pt-2">
          <SafeErrorBoundary resetKeys={[viewType]}>
            <TestMenu />
          </SafeErrorBoundary>
        </div>
        <EmptyTestState />
      </>
    );
  }

  return (
    <>
      <div className="px-1 pt-2">
        <SafeErrorBoundary resetKeys={[viewType]}>
          <TestMenu />
        </SafeErrorBoundary>
      </div>

      <div className="px-1">
        <SafeErrorBoundary resetKeys={[viewType, testHistory]}>
          {testHistory.map((run, index) => (
            <HistoryCard
              key={`${run.timestamp}-${index}`}
              run={run}
            />
          ))}
        </SafeErrorBoundary>
      </div>
    </>
  );
});

TestPanelContent.displayName = 'TestPanelContent';