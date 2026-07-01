'use client';

import { Suspense } from 'react';
import LessonContent from './_components/LessonContent';
import URLSyncWrapper from './_components/URLSyncWrapper';

const GuidePage = () => {
  return (
    <Suspense fallback={<div>Loading...</div>}>
      <URLSyncWrapper>
        <LessonContent />
      </URLSyncWrapper>
    </Suspense>
  );
};

export default GuidePage;
