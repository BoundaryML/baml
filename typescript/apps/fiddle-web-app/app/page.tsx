import dynamic from 'next/dynamic';
import { loadProject } from '../lib/loadProject';
const ProjectView = dynamic(
  () => import('./[project_id]/_components/ProjectView'),
  { ssr: true },
);

type Params = Promise<{ project_id: string }>;
type SearchParams = Promise<{ [key: string]: string | string[] | undefined }>;

// We don't need this since it's already part of layout.tsx
// export const metadata: Metadata = {
//   title: 'Prompt Fiddle',
//   description: '...',
// }
//

export default async function Home({
  searchParams,
  params,
}: {
  searchParams: SearchParams;
  params: Promise<{ project_id: string }>;
}) {
  const data = await loadProject(Promise.resolve(params), true);
  if (!data) {
    return <div>Project not found</div>;
  }
  return (
    <main className="flex flex-col justify-between items-center min-h-screen font-sans bg-background">
      <ProjectView project={data} />
    </main>
  );
}
