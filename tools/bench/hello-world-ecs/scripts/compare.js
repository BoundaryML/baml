#!/usr/bin/env node
// Writes a native CloudWatch dashboard to stdout; uploading it is explicit.
const { dashboard } = require('../infra/benchmark');
const runs = process.argv.slice(2);
if (!runs.length || runs.some(run => !/^[a-z][a-z0-9-]{1,30}[a-z0-9]$/.test(run))) throw new Error('Usage: node scripts/compare.js run-name [another-run]');
console.log(JSON.stringify(dashboard(runs, process.env.AWS_REGION || 'us-east-1'), null, 2));
