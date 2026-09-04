import { AuthoredPage, authoredMetadata } from '@/components/authored-page';
export const metadata = authoredMetadata('/bcs');
export default function BcsPage() {
  return <AuthoredPage path="/bcs" />;
}
