'use client';

import { useEffect, useState } from 'react';
import type { Post } from '../_lib/get-posts';
import { BlogPosts } from './blog-posts';
import { HeroSection } from './hero-section';

interface BlogContentProps {
  initialPosts: Post[];
}

export function BlogContent({ initialPosts }: BlogContentProps) {
  const [selectedCategory, setSelectedCategory] = useState('All');
  const [posts, setPosts] = useState<Post[]>(initialPosts);
  const [filteredPosts, setFilteredPosts] = useState<Post[]>(initialPosts);

  useEffect(() => {
    if (selectedCategory === 'All') {
      setFilteredPosts(posts);
    } else {
      const filterCategory =
        selectedCategory === 'LaunchWeek'
          ? 'launch week'
          : selectedCategory.toLowerCase();
      setFilteredPosts(
        posts.filter((post) =>
          post.tags.some((tag: string) => tag.toLowerCase() === filterCategory),
        ),
      );
    }
  }, [selectedCategory, posts]);

  return (
    <>
      <HeroSection
        onCategoryChange={setSelectedCategory}
        selectedCategory={selectedCategory}
      />
      <BlogPosts posts={filteredPosts} />
    </>
  );
}
