'use client';

import dynamic from 'next/dynamic';

const BamlBlock = dynamic(() => import('./BamlBlock2'), {
  ssr: false,
});

export default BamlBlock;
