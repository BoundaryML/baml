import type { Post } from '../_lib/get-posts';
import { BlogList } from './blog-list';
import { HeroSection } from './hero-section';

interface BlogContentProps {
  initialPosts: Post[];
  selectedType: 'all' | 'article' | 'release';
}

export function BlogContent({ initialPosts, selectedType }: BlogContentProps) {
  const latestRelease = initialPosts.find((post) => post.type === 'release');
  const posts =
    selectedType === 'all'
      ? initialPosts
      : initialPosts.filter((post) => post.type === selectedType);

  return (
    <>
      <HeroSection latestRelease={latestRelease} selectedType={selectedType} />
      <BlogList posts={posts} selectedType={selectedType} />
    </>
  );
}
