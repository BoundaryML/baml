import { AuthoredPage, authoredMetadata } from '@/components/authored-page';

export const metadata = authoredMetadata('/baml');

export default function BamlPage() {
  return <AuthoredPage path="/baml" />;
}
