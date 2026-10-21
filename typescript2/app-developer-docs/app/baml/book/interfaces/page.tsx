import { AuthoredPage, authoredMetadata } from '@/components/authored-page';
export const metadata = authoredMetadata('/baml/book/interfaces');
export default function ChapterPage() {
  return <AuthoredPage path="/baml/book/interfaces" />;
}
