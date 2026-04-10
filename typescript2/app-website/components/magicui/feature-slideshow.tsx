'use client';

import { motion } from 'motion/react';
import Image from 'next/image';
import { cn } from '@/lib/utils';

type FeatureItem = {
  id: number;
  title: string | React.ReactNode;
  content: string | React.ReactNode;
  image?: string;
  video?: string;
  component?: React.ReactNode;
};
type FeatureProps = {
  featureItems: FeatureItem[];
};

export const Feature = ({ featureItems }: FeatureProps) => {
  const containerVariants = {
    hidden: { opacity: 0 },
    visible: {
      opacity: 1,
      transition: {
        delayChildren: 0.2,
        staggerChildren: 0.3,
      },
    },
  };

  const itemVariants = {
    hidden: {
      opacity: 0,
      scale: 0.95,
      // y: 50,
    },
    visible: {
      opacity: 1,
      scale: 1,
      transition: {
        duration: 0.6,
        ease: 'easeOut',
      },
      // y: 0,
    },
  };

  const textVariants = (isEven: boolean) => ({
    hidden: {
      opacity: 0,
      x: isEven ? -50 : 50,
    },
    visible: {
      opacity: 1,
      transition: {
        delay: 0.2,
        duration: 0.5,
        ease: 'easeOut',
      },
      x: 0,
    },
  });

  const mediaVariants = {
    hidden: {
      opacity: 0,
      scale: 0.95,
      // y: 30,
    },
    visible: {
      opacity: 1,
      scale: 1,
      transition: {
        delay: 0.4,
        duration: 0.6,
        ease: 'easeOut',
      },
      // y: 0,
    },
  };

  return (
    <div className="w-full">
      <motion.div
        className="flex w-full flex-col items-center justify-center max-w-7xl mx-auto"
        initial="visible"
        variants={containerVariants}
        viewport={{ margin: '-100px', once: true }}
        whileInView="visible"
      >
        <div className="flex flex-col gap-16 px-10 md:px-20 w-full">
          {featureItems.map((item, index) => {
            const isEven = index % 2 === 0;

            return (
              <motion.div
                className={cn(
                  'flex flex-col lg:flex-row gap-8 lg:gap-12 items-center',
                  isEven ? 'lg:flex-row' : 'lg:flex-row-reverse',
                )}
                initial="visible"
                key={item.id}
                variants={itemVariants}
              >
                {/* Text Content */}
                <motion.div
                  className={cn(
                    'flex flex-col gap-4 lg:w-1/2',
                    isEven ? 'lg:pr-8' : 'lg:pl-8',
                  )}
                  initial="visible"
                  variants={textVariants(isEven)}
                >
                  <div className="flex flex-col gap-3">
                    <motion.h2
                      className="text-2xl lg:text-3xl font-bold tracking-tight"
                      initial={false}
                      viewport={{ once: true }}
                      whileInView={{ opacity: 1, y: 0 }}
                    >
                      {item.title}
                    </motion.h2>
                    <motion.div
                      className="text-base lg:text-lg text-muted-foreground leading-relaxed"
                      initial={false}
                      viewport={{ once: true }}
                      whileInView={{ opacity: 1, y: 0 }}
                    >
                      {item.content}
                    </motion.div>
                  </div>
                </motion.div>

                {/* Media Content */}
                <motion.div
                  className="lg:w-1/2 w-full min-h-[240px]"
                  initial="visible"
                  variants={mediaVariants}
                >
                  {item.component != null && (
                    <motion.div className="w-full min-h-[220px] rounded-xl border border-border overflow-visible bg-background shadow-lg">
                      {item.component}
                    </motion.div>
                  )}
                  {item.image != null && (
                    <motion.div className="w-full aspect-video rounded-xl border border-border overflow-hidden shadow-lg">
                      <Image
                        alt={typeof item.title === 'string' ? item.title : ''}
                        className="w-full h-full object-cover"
                        height={1000}
                        src={item.image}
                        width={1000}
                      />
                    </motion.div>
                  )}
                  {item.video != null && !item.component && (
                    <motion.div className="w-full aspect-video rounded-xl border border-border overflow-hidden shadow-lg">
                      <video
                        autoPlay
                        className="w-full h-full object-cover"
                        loop
                        muted
                        playsInline
                        preload="auto"
                        src={item.video}
                      />
                    </motion.div>
                  )}
                </motion.div>
              </motion.div>
            );
          })}
        </div>
      </motion.div>
    </div>
  );
};
