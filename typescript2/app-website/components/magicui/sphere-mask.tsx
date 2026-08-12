import { motion } from 'framer-motion';
import Image from 'next/image';
import { cn } from '@/lib/utils';

export const SphereMask = ({ reverse = false }: { reverse?: boolean }) => {
  return (
    <div
      className={cn(
        // color
        '[--color:var(--secondary)]',
        'pointer-events-none relative -z-[2] mx-auto overflow-hidden',

        // smaller height and gentler overlap so hero/company-showcase don't get covered
        'h-[28rem]',
        reverse
          ? 'my-[-6rem] rotate-180 md:my-[-8rem]'
          : 'my-[-4rem] md:my-[-6rem]',

        // sphere mask
        '[mask-image:radial-gradient(ellipse_at_center_center,#000,transparent_50%)]',

        // before
        'before:absolute before:inset-0 before:h-full before:w-full before:opacity-40 before:[background-image:radial-gradient(circle_at_bottom_center,var(--color),transparent_70%)]',

        // after - sphere line with higher z-index
        'after:absolute after:-left-1/2 after:top-1/2 after:aspect-[1/0.7] after:w-[200%] after:rounded-[50%] after:border-t after:border-[hsl(var(--border))] after:bg-background after:z-50',
      )}
    >
      {/* BAML Meaning Section positioned inside sphere mask */}
      <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-auto">
        {/* Lamb Logo - positioned to start behind the sphere line and animate upward */}
        <motion.div
          animate={{ opacity: 1, y: -100 }}
          className="relative z-0"
          initial={{ opacity: 0, y: 200 }}
          transition={{
            delay: 1,
            duration: 1.7,
            ease: 'backInOut',
          }}
        >
          <div className="flex size-32 w-full flex-col items-center justify-center rounded-full">
            <Image
              alt="BAML Lamb Logo"
              className="size-24 object-contain"
              height={96}
              priority
              src="/lamb.png"
              width={96}
            />
          </div>
        </motion.div>

        {/* Text Content - positioned below the sphere line */}
        <motion.div
          animate={{ opacity: 1, y: 0 }}
          className="relative z-60 mt-8 md:mt-12"
          initial={{ opacity: 0, y: 100 }}
          transition={{
            delay: 1.4,
            duration: 0.8,
            ease: 'easeOut',
          }}
        >
          <div className="text-center px-4">
            <div className="md:text-lg mb-1 md:mb-2 text-primary">
              Baaaaaaaaaaml
            </div>
            <p className="text-sm md:text-lg text-primary tracking-wide">
              <span className="text-secondary font-bold">B</span>asically{' '}
              <span className="text-secondary font-bold">A</span>{' '}
              <span className="text-secondary font-bold">M</span>ade-Up{' '}
              <span className="text-secondary font-bold">L</span>anguage
            </p>
          </div>
        </motion.div>
      </div>
    </div>
  );
};
