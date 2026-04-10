import { formatDistanceToNow } from 'date-fns';
import { ArrowRight, Clock, Tag } from 'lucide-react';
import Image from 'next/image';
import Link from 'next/link';
import { Card } from '@/components/ui/card';
import type { Post } from '../_lib/get-posts';
import { formatCategoryForDisplay } from './category-filter';

interface FeaturedPostProps {
  post: Post;
}

export function FeaturedPost({ post }: FeaturedPostProps) {
  // Use og.image, then firstImage, then fallback to default
  const postImage = post.og?.image || post.firstImage;

  return (
    <Link href={`/blog/${post.slug}`}>
      <Card className="overflow-hidden cursor-pointer hover:shadow-lg transition-shadow h-96 pt-0 relative group">
        {/* Background Image */}
        <div className="absolute inset-0">
          {postImage ? (
            <Image
              alt={post.title}
              className="object-cover w-full h-full blur-[2px] group-hover:blur-none transition-all duration-300"
              fill
              priority
              sizes="100vw"
              src={postImage}
            />
          ) : (
            <Image
              alt="BAML Background"
              className="object-cover w-full h-full"
              fill
              priority
              sizes="100vw"
              src="/baml-og-background.png"
            />
          )}
          {/* Gradient overlay - darker at bottom for text readability */}
          <div className="absolute inset-0 bg-gradient-to-t from-black/95 via-black/70 to-black/40 transition-all duration-300" />
        </div>

        {/* Glassmorphism Card Content */}
        <div className="relative z-10 h-full flex flex-col justify-between p-8">
          <div className="flex-1">
            {/* Category and Meta Info */}
            <div className="flex items-center gap-4 text-sm text-white/80 mb-4">
              <span className="flex items-center gap-1 px-3 py-1 rounded-full text-sm font-medium bg-white/20 backdrop-blur-sm border border-white/20">
                <Tag className="h-4 w-4" />
                {formatCategoryForDisplay(post.tags[0])}
              </span>

              <span className="flex items-center gap-1">
                <Clock className="h-4 w-4" />
                {post.readingTime}
              </span>
            </div>

            {/* Title */}
            <h2 className="text-3xl font-bold mb-4 line-clamp-2 text-white">
              {post.title}
            </h2>

            {/* Description */}
            <p className="text-white/80 line-clamp-3">{post.description}</p>
          </div>

          {/* Bottom Section */}
          <div className="flex items-center justify-between mt-6">
            <div className="flex items-center gap-3">
              {post.author?.imageUrl && (
                <div className="relative size-12 rounded-full overflow-hidden border-2 border-white/20">
                  <Image
                    alt={post.author.name}
                    className="object-cover"
                    fill
                    sizes="48px"
                    src={post.author.imageUrl}
                  />
                </div>
              )}
              <div className="flex flex-col">
                <span className="text-sm text-white/80">
                  By {post.author?.name}
                </span>
                <span className="text-xs text-white/60">
                  {formatDistanceToNow(new Date(post.date), {
                    addSuffix: true,
                  })}
                </span>
              </div>
            </div>
            <div className="flex items-center gap-2 text-white/80 group-hover:text-white transition-colors">
              <span className="text-sm">Read More</span>
              <ArrowRight className="h-4 w-4 group-hover:translate-x-1 transition-transform" />
            </div>
          </div>
        </div>
      </Card>
    </Link>
  );
}
