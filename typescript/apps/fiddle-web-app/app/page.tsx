'use client';

import { loadProject } from '../lib/loadProject';
import { useEffect, useState } from 'react';
import dynamic from 'next/dynamic';

// Opt out of static generation for this page

// Dynamic import to avoid SSR issues
const ProjectView = dynamic(
  () => import('./[project_id]/_components/ProjectView'),
  {
    ssr: false,
    loading: () => <div>Loading project...</div>
  }
);

type Params = Promise<{ project_id: string }>;
type SearchParams = Promise<{ [key: string]: string | string[] | undefined }>;

// We don't need this since it's already part of layout.tsx
// export const metadata: Metadata = {
//   title: 'Prompt Fiddle',
//   description: '...',
// }

export default function Home({
  searchParams,
  params,
}: {
  searchParams: SearchParams;
  params: Promise<{ project_id: string }>;
}) {
  const [data, setData] = useState<any>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadProject(Promise.resolve(params), true).then((projectData) => {
      setData(projectData);
      setLoading(false);
    });
  }, []);

  if (loading) {
    return <div>Loading...</div>;
  }

  if (!data) {
    return <div>No project found</div>;
  }

  return (
    <main className="flex flex-col justify-between items-center min-h-screen font-sans">
      <div className="w-screen h-screen">
        <ProjectView project={data} />
        {/* <Suspense fallback={<div>Loading...</div>}>{children}</Suspense> */}
      </div>
    </main>
  );
}
