#!/usr/bin/env node
import 'source-map-support/register';
import * as cdk from 'aws-cdk-lib';
import { PkgBoundarymlComStack } from '../lib/pkg-boundaryml-com-stack';

const app = new cdk.App();

new PkgBoundarymlComStack(app, 'PkgBoundarymlComStack', {
  env: {
    account: process.env.CDK_DEFAULT_ACCOUNT,
    region: process.env.CDK_DEFAULT_REGION,
  },
});
