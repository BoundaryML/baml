import { AuthoredPage, authoredMetadata } from '@/components/authored-page';
export const metadata = authoredMetadata('/examples');
export default function ExamplesPage() {
  return <AuthoredPage path="/examples" />;
}
