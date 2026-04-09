import 'dotenv/config';
import { readFileSync, writeFileSync, readdirSync, statSync, mkdirSync, existsSync } from 'fs';
import { join, relative } from 'path';
import matter from 'gray-matter';
import OpenAI from 'openai';
import { chunkMarkdown, DocChunk } from '../src/lib/chunk';

const DOCS_DIR = join(__dirname, '../docs');
const OUTPUT_FILE = join(__dirname, '../static/embeddings.json');
const EMBEDDING_MODEL = 'text-embedding-3-small'; // Cheaper, still good quality

interface EmbeddedChunk extends DocChunk {
  embedding: number[];
}

async function main() {
  const apiKey = process.env.OPENAI_API_KEY;
  if (!apiKey) {
    console.error('Error: OPENAI_API_KEY environment variable is not set');
    process.exit(1);
  }

  const openai = new OpenAI({ apiKey });

  console.log('Scanning documentation files...');
  const mdxFiles = findMdxFiles(DOCS_DIR);
  console.log(`   Found ${mdxFiles.length} MDX files`);

  // Parse and chunk all docs
  console.log('Chunking documents...');
  const allChunks: DocChunk[] = [];

  for (const filepath of mdxFiles) {
    const content = readFileSync(filepath, 'utf-8');
    const { data: frontmatter, content: body } = matter(content);

    const relativePath = relative(DOCS_DIR, filepath);
    const url = '/' + relativePath.replace('.mdx', '').replace('/index', '');
    const title = (frontmatter.title as string) || (frontmatter.sidebar_label as string) || url;

    const chunks = chunkMarkdown(body, { title, url });
    allChunks.push(...chunks);
  }

  console.log(`   Created ${allChunks.length} chunks`);

  // Generate embeddings in batches
  console.log('Generating embeddings...');
  const embeddedChunks: EmbeddedChunk[] = [];
  const batchSize = 100;

  for (let i = 0; i < allChunks.length; i += batchSize) {
    const batch = allChunks.slice(i, i + batchSize);
    const texts = batch.map(chunk => chunk.content);

    const response = await openai.embeddings.create({
      model: EMBEDDING_MODEL,
      input: texts,
    });

    for (let j = 0; j < batch.length; j++) {
      embeddedChunks.push({
        ...batch[j],
        embedding: response.data[j].embedding,
      });
    }

    console.log(`   Processed ${Math.min(i + batchSize, allChunks.length)}/${allChunks.length}`);
  }

  // Ensure static directory exists
  const staticDir = join(__dirname, '../static');
  if (!existsSync(staticDir)) {
    mkdirSync(staticDir, { recursive: true });
  }

  // Write to static file
  console.log('Writing embeddings.json...');
  writeFileSync(OUTPUT_FILE, JSON.stringify({
    model: EMBEDDING_MODEL,
    dimensions: embeddedChunks[0]?.embedding.length || 1536,
    chunks: embeddedChunks,
    generatedAt: new Date().toISOString(),
  }, null, 2));

  const fileSizeKB = Math.round(statSync(OUTPUT_FILE).size / 1024);
  console.log(`Done! Generated ${OUTPUT_FILE} (${fileSizeKB} KB)`);
}

function findMdxFiles(dir: string): string[] {
  const files: string[] = [];

  for (const entry of readdirSync(dir)) {
    const fullPath = join(dir, entry);
    const stat = statSync(fullPath);

    if (stat.isDirectory()) {
      files.push(...findMdxFiles(fullPath));
    } else if (entry.endsWith('.mdx')) {
      files.push(fullPath);
    }
  }

  return files;
}

main().catch(console.error);
