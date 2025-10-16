import { loadProject } from '../lib/loadProject';
import ProjectView from './_components/ClientProjectView';

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
    <main className="flex flex-col justify-between items-center h-screen overflow-hidden font-sans bg-background">
      <ProjectView project={data} />
    </main>
  );
}
