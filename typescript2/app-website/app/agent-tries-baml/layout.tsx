import type { Metadata } from 'next';
import { Navbar } from '@/components/navbar';
import './agent-tries-baml.css';

export const metadata: Metadata = {
  title: 'agent tries baml',
  description:
    'Cross-language perf and Claude Code agent metrics for BAML, captured on the cloud worker.',
};

export default function AgentTriesBamlLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <div style={{ background: '#FBF7ED' }}>
      <Navbar />
      <div className="atb-scope">{children}</div>
    </div>
  );
}
