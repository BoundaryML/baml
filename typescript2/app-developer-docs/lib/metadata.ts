import type { Metadata } from 'next';

import { shouldIndexDeployment } from '@/lib/deployment';

export function documentationMetadata({
  description,
  index = true,
  path,
  title,
}: {
  description: string;
  index?: boolean;
  path: string;
  title: string;
}): Metadata {
  const deploymentIsPublic = shouldIndexDeployment();
  return {
    alternates: { canonical: path },
    description,
    openGraph: {
      description,
      title,
      url: path,
    },
    robots: {
      follow: deploymentIsPublic,
      index: deploymentIsPublic && index,
    },
    title,
  };
}
