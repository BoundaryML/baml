import { readFileSync, existsSync, statSync } from 'fs';
import { resolve } from 'path';

const projectRoot = resolve(process.cwd());
const llmsTxtPath = resolve(projectRoot, 'build/llms.txt');
const llmsFullTxtPath = resolve(projectRoot, 'build/llms-full.txt');

const requiredPaths = [llmsTxtPath, llmsFullTxtPath];
const requiredMarkers = ['/agent-start-here', '/tour/hello-baml', '/tutorials/getting-started', '/reference/baml-syntax'];

const errors = [];

for (const filePath of requiredPaths) {
  if (!existsSync(filePath)) {
    errors.push(`Missing required file: ${filePath}`);
  }
}

if (existsSync(llmsTxtPath)) {
  const llmsTxt = readFileSync(llmsTxtPath, 'utf-8');
  for (const marker of requiredMarkers) {
    if (!llmsTxt.includes(marker)) {
      errors.push(`Missing required llms.txt marker: ${marker}`);
    }
  }
}

if (existsSync(llmsFullTxtPath)) {
  const size = statSync(llmsFullTxtPath).size;
  if (size < 2048) {
    errors.push(`llms-full.txt appears too small (${size} bytes).`);
  }
}

if (errors.length > 0) {
  console.error('Agent surface checks failed:');
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log('Agent surface checks passed.');
