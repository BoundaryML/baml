import { withBaml } from '@boundaryml/baml-nextjs-plugin';
import type { NextConfig } from 'next';

const nextConfig: NextConfig = {
	transpilePackages: ['@baml/ui'],
};

export default withBaml()(nextConfig);
