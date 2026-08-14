/* eslint-disable @next/next/no-img-element */
'use client';

import { Play, XIcon } from 'lucide-react';
import { AnimatePresence, motion } from 'motion/react';
import Image from 'next/image';
import { useState } from 'react';
import { cn } from '@/lib/utils';

type AnimationStyle =
  | 'from-bottom'
  | 'from-center'
  | 'from-top'
  | 'from-left'
  | 'from-right'
  | 'fade'
  | 'top-in-bottom-out'
  | 'left-in-right-out';

interface HeroVideoProps {
  animationStyle?: AnimationStyle;
  videoSrc: string;
  thumbnailSrc?: string;
  thumbnailAlt?: string;
  className?: string;
}

const animationVariants = {
  fade: {
    animate: { opacity: 1 },
    exit: { opacity: 0 },
    initial: { opacity: 0 },
  },
  'from-bottom': {
    animate: { opacity: 1, y: 0 },
    exit: { opacity: 0, y: '100%' },
    initial: { opacity: 0, y: '100%' },
  },
  'from-center': {
    animate: { opacity: 1, scale: 1 },
    exit: { opacity: 0, scale: 0.5 },
    initial: { opacity: 0, scale: 0.5 },
  },
  'from-left': {
    animate: { opacity: 1, x: 0 },
    exit: { opacity: 0, x: '-100%' },
    initial: { opacity: 0, x: '-100%' },
  },
  'from-right': {
    animate: { opacity: 1, x: 0 },
    exit: { opacity: 0, x: '100%' },
    initial: { opacity: 0, x: '100%' },
  },
  'from-top': {
    animate: { opacity: 1, y: 0 },
    exit: { opacity: 0, y: '-100%' },
    initial: { opacity: 0, y: '-100%' },
  },
  'left-in-right-out': {
    animate: { opacity: 1, x: 0 },
    exit: { opacity: 0, x: '100%' },
    initial: { opacity: 0, x: '-100%' },
  },
  'top-in-bottom-out': {
    animate: { opacity: 1, y: 0 },
    exit: { opacity: 0, y: '100%' },
    initial: { opacity: 0, y: '-100%' },
  },
};

export function HeroVideoDialog({
  animationStyle = 'from-center',
  videoSrc,
  thumbnailSrc,
  thumbnailAlt = 'Video thumbnail',
  className,
}: HeroVideoProps) {
  const [isVideoOpen, setIsVideoOpen] = useState(false);
  const selectedAnimation = animationVariants[animationStyle];

  return (
    <div className={cn('relative', className)}>
      <div
        className="group relative cursor-pointer"
        onClick={() => setIsVideoOpen(true)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            setIsVideoOpen(true);
          }
        }}
      >
        {thumbnailSrc ? (
          <Image
            alt={thumbnailAlt}
            className="w-full transition-all duration-200 ease-out group-hover:brightness-[0.8] isolate"
            height={1080}
            src={thumbnailSrc}
            width={1920}
          />
        ) : (
          <div className="w-full aspect-video bg-background rounded-2xl" />
        )}
        <div className="absolute isolate inset-0 flex scale-[0.9] items-center justify-center rounded-2xl transition-all duration-200 ease-out group-hover:scale-100">
          <div className="flex size-28 items-center justify-center rounded-full bg-gradient-to-t from-secondary/20 to-[#ACC3F7/15] backdrop-blur-md">
            <div className="relative flex size-20 scale-100 items-center justify-center rounded-full bg-gradient-to-t from-secondary to-white/10 shadow-md transition-all duration-200 ease-out group-hover:scale-[1.2]">
              <Play
                className="size-8 scale-100 fill-white text-white transition-transform duration-200 ease-out group-hover:scale-105"
                style={{
                  filter:
                    'drop-shadow(0 4px 3px rgb(0 0 0 / 0.07)) drop-shadow(0 2px 2px rgb(0 0 0 / 0.06))',
                }}
              />
            </div>
          </div>
        </div>
      </div>
      <AnimatePresence>
        {isVideoOpen && (
          <motion.div
            animate={{ opacity: 1 }}
            className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-md"
            exit={{ opacity: 0 }}
            initial={{ opacity: 0 }}
            onClick={() => setIsVideoOpen(false)}
          >
            <motion.div
              {...selectedAnimation}
              className="relative mx-4 aspect-video w-full max-w-4xl md:mx-0"
              transition={{ damping: 30, stiffness: 300, type: 'spring' }}
            >
              <motion.button
                className="absolute cursor-pointer hover:scale-[98%] transition-all duration-200 ease-out -top-16 right-0 rounded-full bg-neutral-900/50 p-2 text-xl text-white ring-1 backdrop-blur-md dark:bg-neutral-100/50 dark:text-black"
                onClick={() => setIsVideoOpen(false)}
              >
                <XIcon className="size-5" />
              </motion.button>
              <div className="relative isolate z-[1] size-full overflow-hidden rounded-2xl border-2 border-white">
                <iframe
                  allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
                  allowFullScreen
                  className="size-full"
                  src={videoSrc}
                  title="Video"
                />
              </div>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
