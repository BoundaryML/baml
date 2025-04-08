import { withBaml } from '@boundaryml/baml-nextjs-plugin';
import type { NextConfig } from 'next';

const nextConfig: NextConfig = {};

export default withBaml()(nextConfig);
