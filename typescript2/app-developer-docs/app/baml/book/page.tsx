import { AuthoredPage, authoredMetadata } from '@/components/authored-page';
export const metadata = authoredMetadata('/baml/book');
export default function BookPage() {
  return <AuthoredPage path="/baml/book" />;
}
