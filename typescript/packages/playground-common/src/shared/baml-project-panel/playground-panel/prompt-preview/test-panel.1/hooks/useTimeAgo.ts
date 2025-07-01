import { formatDistanceToNow } from 'date-fns';
import { useCallback, useEffect, useState } from 'react';
import { TIME_INTERVALS, UPDATE_THRESHOLDS } from '../constants';

export const useTimeAgo = (timestamp: number) => {
  const [formattedTime, setFormattedTime] = useState(() =>
    formatDistanceToNow(new Date(timestamp), { addSuffix: true }),
  );

  const getUpdateInterval = useCallback((age: number): number => {
    if (age < UPDATE_THRESHOLDS.MINUTE) return TIME_INTERVALS.SECOND;
    if (age < UPDATE_THRESHOLDS.HOUR) return TIME_INTERVALS.MINUTE;
    return TIME_INTERVALS.HOUR;
  }, []);

  const updateTime = useCallback(() => {
    const newFormattedTime = formatDistanceToNow(new Date(timestamp), {
      addSuffix: true,
      includeSeconds: true,
    });
    setFormattedTime(newFormattedTime);
  }, [timestamp]);

  useEffect(() => {
    updateTime();

    const age = Date.now() - timestamp;
    const intervalDuration = getUpdateInterval(age);
    const timer = setInterval(updateTime, intervalDuration);

    return () => clearInterval(timer);
  }, [timestamp, updateTime, getUpdateInterval]);

  return formattedTime;
};