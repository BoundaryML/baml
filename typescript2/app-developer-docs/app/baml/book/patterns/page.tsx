import { AuthoredPage, authoredMetadata } from '@/components/authored-page';
export const metadata = authoredMetadata('/baml/book/patterns');
export default function ChapterPage() {
  return <AuthoredPage path="/baml/book/patterns" />;
}
