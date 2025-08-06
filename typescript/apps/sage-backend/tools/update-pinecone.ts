import { populatePinecone } from '@/lib/pinecone-api';

async function main() {
  try {
    console.log('Starting Pinecone update...');
    await populatePinecone(process.argv[2]);
    console.log('Pinecone update completed successfully!');
  } catch (error) {
    console.error('Error updating Pinecone:', error);
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}
