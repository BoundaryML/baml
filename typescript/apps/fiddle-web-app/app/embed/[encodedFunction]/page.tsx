import type { Metadata } from 'next';
import dynamic from 'next/dynamic';
import { notFound } from 'next/navigation';
import { decodeBase64 } from '../../../lib/base64';
import type { BAMLProject } from '../../../lib/exampleProjects';

const ProjectView = dynamic(
  () => import('../../[project_id]/_components/ProjectView'),
  { ssr: true },
);

interface EmbedPageProps {
  params: Promise<{ encodedFunction: string }>;
}

export async function generateMetadata({
  params,
}: EmbedPageProps): Promise<Metadata> {
  try {
    const { encodedFunction } = await params;

    // Decode the function data
    const decodedData = decodeBase64(encodedFunction);
    const project = JSON.parse(decodedData);

    return {
      title: `${project.name} — Prompt Fiddle`,
      description: project.description || 'Embedded BAML function',
    };
  } catch (error) {
    console.error('Error generating metadata for embed:', error);
    return {
      title: 'Embedded BAML Function — Prompt Fiddle',
      description: 'Embedded BAML function in the playground',
    };
  }
}

export default async function EmbedPage({ params }: EmbedPageProps) {
  try {
    const { encodedFunction } = await params;

    // Decode the function data
    const decodedData = decodeBase64(encodedFunction);

    let project: BAMLProject;
    try {
      project = JSON.parse(decodedData);
    } catch (error) {
      console.error('Error parsing project:', error);
      throw new Error('Invalid project structure');
    }

    // Validate the project structure
    if (!project.name || !project.files || !Array.isArray(project.files)) {
      throw new Error('Invalid project structure');
    }

    return (
      <main className="flex flex-col justify-between items-center min-h-screen font-sans bg-screen">
        <ProjectView project={project} />
      </main>
    );
  } catch (error) {
    console.error('Error loading embedded function:', error);
    notFound();
  }
}
