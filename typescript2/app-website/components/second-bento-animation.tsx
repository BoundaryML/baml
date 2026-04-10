/* eslint-disable @next/next/no-img-element */
'use client';

import { AnimatePresence, motion, useInView } from 'motion/react';
import { useEffect, useRef, useState } from 'react';
import { Icons } from '@/components/icons';
import { SyntaxTypingAnimation } from '@/components/magicui/syntax-typing-animation';

export function SecondBentoAnimation() {
  const ref = useRef(null);
  const isInView = useInView(ref);
  const [shouldAnimate, setShouldAnimate] = useState(false);

  useEffect(() => {
    let timeoutId: NodeJS.Timeout;
    if (isInView) {
      timeoutId = setTimeout(() => {
        setShouldAnimate(true);
      }, 1000);
    } else {
      setShouldAnimate(false);
    }

    return () => {
      if (timeoutId) clearTimeout(timeoutId);
    };
  }, [isInView]);

  const chainOfThoughtCode = `Analyzing the resume text...
Name: "John Smith"
Job Title: "Senior Software Engineer`;

  const returnedObjectCode = `{
  "name": "John Smith",
  "jobTitle": "Senior Software Engineer"
}`;

  return (
    <div
      className="w-full h-full p-4 flex flex-col items-center justify-center gap-5"
      ref={ref}
    >
      <div className="pointer-events-none absolute bottom-0 left-0 h-20 w-full bg-gradient-to-t from-background to-transparent z-20" />
      <motion.div
        animate={{
          y: shouldAnimate ? -75 : 0,
        }}
        className="max-w-md mx-auto w-full flex flex-col gap-2"
        transition={{
          damping: 20,
          stiffness: 300,
          type: 'spring',
        }}
      >
        <div className="flex items-end justify-end gap-3">
          <motion.div
            animate={{ opacity: 1, x: 0 }}
            className="max-w-[280px] bg-secondary text-white p-4 rounded-2xl ml-auto shadow-[0_0_10px_rgba(0,0,0,0.05)]"
            initial={{ opacity: 0, x: 20 }}
            transition={{
              duration: 0.3,
              ease: 'easeOut',
            }}
          >
            <p className="text-sm">
              Extract the person's name and job title from this resume text...
            </p>
          </motion.div>
          <div className="flex items-center bg-background rounded-full w-fit border border-border flex-shrink-0">
            <img
              alt="User Avatar"
              className="size-8 rounded-full flex-shrink-0"
              src="https://randomuser.me/api/portraits/men/32.jpg"
            />
          </div>
        </div>

        <div className="flex items-start gap-2">
          <div className="flex items-center bg-background rounded-full size-10 flex-shrink-0 justify-center shadow-[0_0_10px_rgba(0,0,0,0.05)] border border-border">
            <Icons.logo className="size-4" />
          </div>

          <div className="relative">
            <AnimatePresence mode="wait">
              {!shouldAnimate ? (
                <motion.div
                  animate={{ opacity: 1, x: 0 }}
                  className="absolute left-0 top-0 bg-background p-4 rounded-2xl border border-border"
                  exit={{ opacity: 0, x: -10 }}
                  initial={{ opacity: 0, x: -20 }}
                  key="dots"
                  transition={{
                    duration: 0.2,
                    ease: 'easeOut',
                  }}
                >
                  <div className="flex gap-1">
                    {[0, 1, 2].map((index) => (
                      <motion.div
                        animate={{ y: [0, -5, 0] }}
                        className="w-2 h-2 bg-primary/50 rounded-full"
                        key={index}
                        transition={{
                          delay: index * 0.2,
                          duration: 0.6,
                          ease: 'easeInOut',
                          repeat: Number.POSITIVE_INFINITY,
                        }}
                      />
                    ))}
                  </div>
                </motion.div>
              ) : (
                <div className="absolute left-0 top-0 flex flex-col gap-3">
                  {/* First response - Chain of thought */}
                  <motion.div
                    animate={{
                      opacity: 1,
                      x: 0,
                    }}
                    className="md:min-w-[300px] min-w-[220px] p-4 border border-border rounded-xl shadow-[0_0_10px_rgba(0,0,0,0.05)]"
                    exit={{ opacity: 0, x: 20 }}
                    initial={{ opacity: 0, x: 10 }}
                    key="chain-of-thought"
                    layout
                    transition={{
                      duration: 0.3,
                      ease: 'easeOut',
                    }}
                  >
                    <SyntaxTypingAnimation
                      className="text-xs font-mono leading-relaxed"
                      code={chainOfThoughtCode}
                      delay={500}
                      duration={30}
                      language="typescript"
                    />
                  </motion.div>

                  {/* Second response - Returned object */}
                  <motion.div
                    animate={{
                      opacity: 1,
                      x: 0,
                    }}
                    className="md:min-w-[300px] min-w-[220px] p-4 bg-green-50 border border-green-200 rounded-xl shadow-[0_0_10px_rgba(0,0,0,0.05)]"
                    exit={{ opacity: 0, x: 20 }}
                    initial={{ opacity: 0, x: 10 }}
                    key="returned-object"
                    layout
                    transition={{
                      delay: 0.5,
                      duration: 0.3,
                      ease: 'easeOut',
                    }}
                  >
                    <SyntaxTypingAnimation
                      className="text-xs font-mono leading-relaxed"
                      code={returnedObjectCode}
                      delay={1500}
                      duration={20}
                      language="json"
                    />
                  </motion.div>
                </div>
              )}
            </AnimatePresence>
          </div>
        </div>
      </motion.div>
    </div>
  );
}
