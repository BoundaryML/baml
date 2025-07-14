import type { BAMLProject } from '../../../lib/exampleProjects';
import { ExampleProjectCard } from '../../_components/ExampleProjectCard';

export const ExploreProjects = () => {
  return null;
};

const ExampleCarousel = ({
  title,
  projects,
}: { title: string; projects: BAMLProject[] }) => {
  return (
    <>
      <div className="flex flex-col py-4 gap-y-3">
        <div className="text-lg font-semibold">{title}</div>
        <div className="flex flex-wrap gap-x-4 gap-y-4">
          {projects.map((p) => {
            return <ExampleProjectCard key={p.id} project={p} />;
          })}
        </div>
      </div>
    </>
  );
};
