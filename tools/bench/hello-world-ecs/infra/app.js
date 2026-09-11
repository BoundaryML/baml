#!/usr/bin/env node
const cdk = require('aws-cdk-lib');
const fs = require('node:fs');
const { FoundationStack, BenchmarkStack } = require('./benchmark');
const app = new cdk.App();
const env = { account: process.env.CDK_DEFAULT_ACCOUNT, region: process.env.CDK_DEFAULT_REGION || process.env.AWS_REGION || 'us-east-1' };
const foundation = new FoundationStack(app, 'hello-world-foundation', { env });
const run = app.node.tryGetContext('run');
if (run) {
  const imagePath = app.node.tryGetContext('images');
  const profilePath = app.node.tryGetContext('profile') || 'profiles/steady-100.json';
  if (!imagePath) throw new Error('Use -c images=artifacts/<build>/images.json');
  const manifest = JSON.parse(fs.readFileSync(imagePath, 'utf8'));
  if (Object.values(manifest.images).some(image => !image.pushed)) throw new Error('Deploy only pushed images');
  const images = Object.fromEntries(Object.entries(manifest.images).map(([name, item]) => [name, item.image]));
  const profile = JSON.parse(fs.readFileSync(profilePath, 'utf8'));
  new BenchmarkStack(app, run, { env, foundation, images, rate: profile.rate_per_target,
    appCount: Number(app.node.tryGetContext('appCount') ?? 1), loadCount: Number(app.node.tryGetContext('loadCount') ?? 1) });
}
