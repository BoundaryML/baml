import { withBaml } from '@boundaryml/baml-nextjs-plugin';
import type { NextConfig } from 'next';

const nextConfig: NextConfig = {
  /* config options here */
  output: 'standalone',
  transpilePackages: ['@baml/sage-interface'],
};

export default withBaml()(nextConfig);
