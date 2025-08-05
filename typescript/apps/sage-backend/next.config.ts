import type { NextConfig } from 'next';
import { withBaml } from '@boundaryml/baml-nextjs-plugin';

const nextConfig: NextConfig = {
  /* config options here */
  output: 'standalone',
};

export default withBaml()(nextConfig);
