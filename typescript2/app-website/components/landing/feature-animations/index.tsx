'use client';

import { AnimatePresence, motion } from 'framer-motion';
import { DescribeAnimation } from './describe-animation';
import { GenericsAnimation } from './generics-animation';
import { ParserAnimation } from './parser-animation';
import { StreamingAnimation } from './streaming-animation';
import { TestsAnimation } from './tests-animation';
import { TypedErrorsAnimation } from './typed-errors-animation';

type FeatureId =
  | 'describe'
  | 'generics'
  | 'parser'
  | 'streaming'
  | 'tests'
  | 'typed-errors';

export function FeatureAnimation({
  featureId,
  tint,
}: {
  featureId: string;
  tint: string;
}) {
  return (
    <AnimatePresence mode="wait">
      <motion.div
        animate={{ opacity: 1, scale: 1 }}
        className="h-full w-full"
        exit={{ opacity: 0, scale: 0.98 }}
        initial={{ opacity: 0, scale: 0.98 }}
        key={featureId}
        transition={{ duration: 0.25, ease: [0.22, 0.61, 0.36, 1] }}
      >
        {renderAnimation(featureId as FeatureId, tint)}
      </motion.div>
    </AnimatePresence>
  );
}

function renderAnimation(id: FeatureId, tint: string) {
  switch (id) {
    case 'parser':
      return <ParserAnimation tint={tint} />;
    case 'generics':
      return <GenericsAnimation tint={tint} />;
    case 'describe':
      return <DescribeAnimation tint={tint} />;
    case 'streaming':
      return <StreamingAnimation tint={tint} />;
    case 'tests':
      return <TestsAnimation tint={tint} />;
    case 'typed-errors':
      return <TypedErrorsAnimation tint={tint} />;
    default:
      return <ParserAnimation tint={tint} />;
  }
}
