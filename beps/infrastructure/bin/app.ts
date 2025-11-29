#!/usr/bin/env node
import 'source-map-support/register';
import * as cdk from 'aws-cdk-lib';
import { BepsStack } from '../lib/beps-stack';

const app = new cdk.App();

// Configuration
const accountId = process.env.CDK_DEFAULT_ACCOUNT;
const config = {
  // Domain is optional - skip for testing (uses CloudFront default domain)
  domain: process.env.BEPS_DOMAIN, // Set to 'beps.boundaryml.com' for production
  githubOrg: process.env.GITHUB_ORG || 'BoundaryML',
  githubRepo: process.env.GITHUB_REPO || 'baml',
  certificateArn: process.env.CERTIFICATE_ARN, // Optional: Use existing certificate
  githubOidcProviderArn: process.env.GITHUB_OIDC_PROVIDER_ARN ||
    (accountId ? `arn:aws:iam::${accountId}:oidc-provider/token.actions.githubusercontent.com` : undefined),
};

new BepsStack(app, 'BepsStack', {
  env: {
    account: accountId,
    region: 'us-east-1', // Must be us-east-1 for CloudFront + ACM
  },
  description: 'BAML BEPs documentation infrastructure with subdomain-based previews',
  ...config,
});

app.synth();

