import { AuthoredPage, authoredMetadata } from '@/components/authored-page';
export const metadata = authoredMetadata('/tutorials');
export default function TutorialsPage() {
  return <AuthoredPage path="/tutorials" />;
}
