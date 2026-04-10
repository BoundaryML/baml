'use client';

import React from 'react';
import { Suspense } from 'react';
import Loading from './_components/LoadingBar';
const TestResultsTable = React.lazy(
  () => import('./_components/TestResultsTable'),
);

// const TestResultsTable = dynamic(
//   () => import("./_components/TestResultsTable")
// );

export default function DataComponent() {
  return (
    <Suspense fallback={<Loading />}>
      <TestResultsTable />
    </Suspense>
  );
}
