/* eslint-disable @next/next/no-img-element */
import { ImageResponse } from 'next/og';
import {
  BaseLayout,
  contentType,
  size,
} from '@/components/shared-images/base-layout';
import { Title } from '@/components/shared-images/title';

export { size, contentType };

export default function Image() {
  const baseLayout = BaseLayout({
    children: (
      <Title
        subtitle="Build, test, and develop LLM applications."
        title="Boundary"
      />
    ),
  });

  return new ImageResponse(baseLayout, {
    ...size,
  });
}
