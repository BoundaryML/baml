'use client';

import NextImage, { type ImageProps } from 'next/image';
import { useEffect, useRef, useState } from 'react';
import { cn } from '@/lib/utils';

export function MDXMedia({
  src,
  alt,
}: React.DetailedHTMLProps<
  React.ImgHTMLAttributes<HTMLImageElement>,
  HTMLImageElement
> & {
  src: string;
  alt: string;
}) {
  const [hasBeenFullyVisible, setHasBeenFullyVisible] = useState(false);
  const imageRef = useRef<HTMLSpanElement>(null);
  const isGif = src.toLowerCase().endsWith('.gif');
  const isVideo =
    src.toLowerCase().endsWith('.mp4') || src.toLowerCase().endsWith('.webm');

  useEffect(() => {
    // Skip visibility check for videos
    if (isVideo) return;

    const checkVisibility = () => {
      if (!imageRef.current) return;

      const rect = imageRef.current.getBoundingClientRect();
      const windowHeight = window.innerHeight;

      // Check if the element is fully within the viewport
      const isElementFullyVisible =
        rect.top >= 0 && rect.bottom <= windowHeight;

      // Check if element is completely out of view
      const isElementCompletelyHidden =
        rect.bottom < 0 || rect.top > windowHeight;

      if (isElementFullyVisible && !hasBeenFullyVisible) {
        setHasBeenFullyVisible(true);
      } else if (isElementCompletelyHidden) {
        setHasBeenFullyVisible(false);
      }
    };

    // Check visibility on mount and scroll
    checkVisibility();
    window.addEventListener('scroll', checkVisibility, { passive: true });
    window.addEventListener('resize', checkVisibility, { passive: true });

    return () => {
      window.removeEventListener('scroll', checkVisibility);
      window.removeEventListener('resize', checkVisibility);
    };
  }, [hasBeenFullyVisible]);

  let widthFromSrc: number | undefined;
  let heightFromSrc: number | undefined;
  const baseUrl =
    process.env.NEXT_PUBLIC_BASE_URL ??
    (process.env.VERCEL_PROJECT_PRODUCTION_URL
      ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
      : 'http://localhost:3000');
  const url = new URL(src, baseUrl);
  const widthParam = url.searchParams.get('w') || url.searchParams.get('width');
  const heightParam =
    url.searchParams.get('h') || url.searchParams.get('height');
  if (widthParam) {
    widthFromSrc = Number.parseInt(widthParam, 10);
  }
  if (heightParam) {
    heightFromSrc = Number.parseInt(heightParam, 10);
  }

  // Default dimensions for full-width display
  const defaultWidth = 2400;
  const defaultHeight = 1600;

  const imageProps = {
    src: isGif && !hasBeenFullyVisible ? `${src}#still` : src,
    alt,
    height: heightFromSrc || defaultHeight,
    width: widthFromSrc || defaultWidth,
    loading: 'lazy' as const,
    unoptimized: isGif,
    className: cn('h-auto object-contain ', widthFromSrc ? 'w-auto' : 'w-full'),
  } satisfies ImageProps;

  if (isVideo) {
    return (
      <span className="block w-full text-center">
        <span ref={imageRef} className="inline-block w-full">
          <video
            src={src}
            controls
            autoPlay
            loop
            muted
            playsInline
            preload="auto"
            controlsList="nodownload"
            width={widthFromSrc || 1728}
            height={heightFromSrc || 1080}
            className={cn('w-full h-auto', 'max-w-[100vw]')}
            style={{
              aspectRatio: `${widthFromSrc || defaultWidth}/${heightFromSrc || defaultHeight}`,
            }}
          />
        </span>
      </span>
    );
  }

  return (
    <span className="block w-full text-center">
      <span ref={imageRef} className="inline-block">
        <a
          href={src}
          target="_blank"
          rel="noopener noreferrer"
          className="cursor-zoom-in"
        >
          <NextImage {...imageProps} />
        </a>
      </span>
    </span>
  );
}
