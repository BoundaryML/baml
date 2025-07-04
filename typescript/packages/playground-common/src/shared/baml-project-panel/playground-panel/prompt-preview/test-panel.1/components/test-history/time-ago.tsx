import { memo } from 'react';
import { useTimeAgo } from '../../hooks/useTimeAgo';

export const TimeAgo = memo<{ timestamp: number }>(({ timestamp }) => {
  const formattedTime = useTimeAgo(timestamp);

  return <div className="text-xs text-muted-foreground">{formattedTime}</div>;
});

TimeAgo.displayName = 'TimeAgo';