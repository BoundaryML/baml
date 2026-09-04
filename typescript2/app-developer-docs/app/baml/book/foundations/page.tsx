import { AuthoredPage, authoredMetadata } from '@/components/authored-page';
export const metadata = authoredMetadata('/baml/book/foundations');
export default function FoundationsPage() {
  return <AuthoredPage path="/baml/book/foundations" />;
}
