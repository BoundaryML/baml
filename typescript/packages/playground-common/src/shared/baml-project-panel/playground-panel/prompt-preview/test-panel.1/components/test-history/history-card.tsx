import { memo } from 'react';
import { type TestHistoryRun } from '../../atoms';
import { TestResultView } from '../test-results/simple-test-result-view';
import { TimeAgo } from './time-ago';

export const HistoryCard = memo<{ run: TestHistoryRun }>(({ run }) => (
  <div className="flex flex-col gap-1">
    <TimeAgo timestamp={run.timestamp} />
    <TestResultView run={run} />
  </div>
));

HistoryCard.displayName = 'HistoryCard';