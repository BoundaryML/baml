'use client';

import { useEffect, useState } from 'react';

function Loading() {
  const [message, setMessage] = useState('Loading data...');

  useEffect(() => {
    const messages = [
      'Loading data...',
      'Almost there...',
      'Just a moment...',
      'Hang tight...',
      'Fetching results...',
    ];
    let index = 0;
    const interval = setInterval(() => {
      index = (index + 1) % messages.length;

      setMessage(messages[index]!);
    }, 2000); // Change message every 5 seconds

    return () => clearInterval(interval);
  }, []);

  return (
    <div className="flex h-full flex-col items-center justify-center">
      <div className="mb-4 text-lg font-semibold text-gray-700">{message}</div>
      <div className="h-8 w-8 animate-spin rounded-full border-b-2 border-t-2 border-blue-500"></div>
    </div>
  );
}

export default Loading;
