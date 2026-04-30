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
        subtitle="Typed prompt boundaries for structured-output LLM work."
        title="BAML"
      />
    ),
  });

  return new ImageResponse(baseLayout, {
    ...size,
  });
}
