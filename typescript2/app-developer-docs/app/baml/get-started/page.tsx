import { AuthoredPage, authoredMetadata } from '@/components/authored-page';
export const metadata = authoredMetadata('/baml/get-started');
export default function GetStartedPage() {
  return <AuthoredPage path="/baml/get-started" />;
}
