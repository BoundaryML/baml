'use client';

import { useSearchParams } from 'next/navigation';
import { type JSX, useEffect } from 'react';

const isDevelopment = process.env.NODE_ENV === 'development';

export function ReactScan(): JSX.Element {
  const pathParams = useSearchParams();
  const enabled = pathParams.get('react-scan') === 'true';

  useEffect(() => {
    if (enabled && isDevelopment) {
      import('react-scan').then(({ scan }) => {
        scan({
          enabled,
        });
      });
    }
  }, [enabled]);

  return <></>;
}
