import { AuthoredPage, authoredMetadata } from '@/components/authored-page';
export const metadata = authoredMetadata('/baml/language');
export default function LanguagePage() {
  return <AuthoredPage path="/baml/language" />;
}
