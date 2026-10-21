import { AuthoredPage, authoredMetadata } from '@/components/authored-page';

export const metadata = authoredMetadata('/examples/vision');

export default function VisionExamplePage() {
  return <AuthoredPage path="/examples/vision" />;
}
