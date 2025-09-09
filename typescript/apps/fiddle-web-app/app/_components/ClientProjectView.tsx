'use client';

import dynamic from 'next/dynamic';

const ProjectView = dynamic(
  () => import('../[project_id]/_components/ProjectView'),
  { ssr: false },
);

export default ProjectView;