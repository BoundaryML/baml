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
        subtitle="Code that agents write. Software that humans trust."
        title="BAML"
      />
    ),
  });

  return new ImageResponse(baseLayout, {
    ...size,
  });
}
