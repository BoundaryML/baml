import { AuthoredPage, authoredMetadata } from '@/components/authored-page';
export const metadata = authoredMetadata('/cli');
export default function CliPage() {
  return <AuthoredPage path="/cli" />;
}
