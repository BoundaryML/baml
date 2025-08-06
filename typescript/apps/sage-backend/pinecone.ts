import { populatePinecone } from './lib/pinecone-api';

async function main() {
  await populatePinecone('/Users/sam/baml2/fern/docs.yml');
  // await testPopulatePinecone();

  // const results = await searchPinecone('is there a baml zed extension?');
  // console.log(results);
  // return;

  // for (const result of results) {
  //   // console.log(result);
  //   // continue;
  //   console.log(`  title "${result.metadata?.title}"`);
  //   const indentedBody = result.metadata?.body
  //     ?.split('\n')
  //     .map((line) => ' '.repeat(4) + line)
  //     .join('\n');
  //   console.log(`  body #"\n${indentedBody}\n  "#`);
  //   // console.log(`  slug "${result.metadata?.slug}"`);
  //   console.log(`  relevance_score ${result.score}`);
  //   console.log('--------------------------------');
  // }
}

main();
