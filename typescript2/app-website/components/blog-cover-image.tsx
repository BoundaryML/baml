'use client';

import { motion } from 'motion/react';
import Image from 'next/image';
import type { Post } from '@/app/blog/_lib/get-posts';
import { cn } from '@/lib/utils';

interface BlogCoverImageProps {
  post: Post;
  className?: string;
}

// Generate a subtle color based on post title
function generateColor(title: string) {
  const hash = title.split('').reduce((acc, char) => {
    const newAcc = (acc << 5) - acc + char.charCodeAt(0);
    return newAcc & newAcc;
  }, 0);

  const colors = [
    'from-slate-600 to-slate-700',
    'from-gray-600 to-gray-700',
    'from-zinc-600 to-zinc-700',
    'from-neutral-600 to-neutral-700',
    'from-stone-600 to-stone-700',
  ];

  return colors[Math.abs(hash) % colors.length];
}

export function BlogCoverImage({ post, className }: BlogCoverImageProps) {
  const gradientColor = generateColor(post.title);

  // Check if post has an OG image
  const hasOgImage = Boolean(post.og?.image);

  const renderPattern = () => {
    // If we have an OG image, show it as background
    if (post.og?.image) {
      return (
        <div className="absolute inset-0">
          <Image
            alt={post.title}
            className="object-cover"
            fill
            priority
            sizes="100vw"
            src={post.og.image}
          />
          {/* Dark overlay to ensure text readability */}
          {/* <div className="absolute inset-0 bg-black/40" /> */}
        </div>
      );
    }

    // Fall back to BAML OG background if no OG image
    return (
      <div className="absolute inset-0">
        <Image
          alt="BAML Background"
          className="object-cover"
          fill
          priority
          sizes="100vw"
          src="/baml-og-background.png"
        />
        {/* Add a semi-transparent overlay to ensure text readability */}
        <div className="absolute inset-0 bg-gradient-to-br from-purple-900/30 to-transparent" />
      </div>
    );
  };

  return (
    <div
      className={cn(
        'relative h-48 overflow-hidden rounded-t-lg group',
        className,
      )}
    >
      {renderPattern()}

      {/* Overlay with post info */}
      <div className="absolute inset-0 bg-gradient-to-t from-black/60 via-transparent to-transparent" />

      {/* Floating elements based on post type */}
      <div className="absolute inset-0 flex items-center justify-center">
        <div className="text-center text-white">
          <motion.div
            animate={{ y: 0 }}
            className="space-y-2 opacity-0 group-hover:opacity-100 transition-opacity duration-300"
            initial={{ y: 20 }}
            transition={{ duration: 0.6 }}
          >
            {/* Category badge */}
            <div className="inline-block px-3 py-1.5 bg-white/20 backdrop-blur-sm rounded-full text-sm font-medium">
              {post.tags[0]}
            </div>

            {/* Reading time */}
            <div className="text-sm opacity-80">{post.readingTime}</div>
          </motion.div>
        </div>
      </div>
    </div>
  );
}
