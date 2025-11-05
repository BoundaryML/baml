import { withBaml } from '@boundaryml/baml-nextjs-plugin';
import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  /* config options here */
};

export default withBaml()(nextConfig);
