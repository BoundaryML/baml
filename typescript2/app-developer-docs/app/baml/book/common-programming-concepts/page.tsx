import { AuthoredPage, authoredMetadata } from '@/components/authored-page';
export const metadata = authoredMetadata(
  '/baml/book/common-programming-concepts',
);
export default function ChapterPage() {
  return <AuthoredPage path="/baml/book/common-programming-concepts" />;
}
