'use client';

import { AnimatePresence, motion } from 'framer-motion';
import { AlignJustify, XIcon } from 'lucide-react';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { buttonVariants } from '@/components/ui/button';
import { cn } from '@/lib/utils';

const menuItem = [
  {
    href: '/features',
    id: 1,
    label: 'Features',
  },
  {
    href: '#',
    id: 2,
    label: 'Pricing',
  },
  {
    href: '#',
    id: 3,
    label: 'Careers',
  },
  {
    href: '#',
    id: 4,
    label: 'Contact Us',
  },
];

export function SiteHeader() {
  const mobilenavbarVariant = {
    animate: {
      opacity: 1,
      scale: 1,
      transition: {
        duration: 0.2,
        ease: 'easeOut',
      },
    },
    exit: {
      opacity: 0,
      transition: {
        delay: 0.2,
        duration: 0.2,
        ease: 'easeOut',
      },
    },
    initial: {
      opacity: 0,
      scale: 1,
    },
  } as const;

  const mobileLinkVar = {
    initial: {
      opacity: 0,
      y: '-20px',
    },
    open: {
      opacity: 1,
      transition: {
        duration: 0.3,
        ease: 'easeOut',
      },
      y: 0,
    },
  } as const;

  const containerVariants = {
    open: {
      transition: {
        staggerChildren: 0.06,
      },
    },
  } as const;

  const [hamburgerMenuIsOpen, setHamburgerMenuIsOpen] = useState(false);

  useEffect(() => {
    const html = document.querySelector('html');
    if (html) html.classList.toggle('overflow-hidden', hamburgerMenuIsOpen);
  }, [hamburgerMenuIsOpen]);

  useEffect(() => {
    const closeHamburgerNavigation = () => setHamburgerMenuIsOpen(false);
    window.addEventListener('orientationchange', closeHamburgerNavigation);
    window.addEventListener('resize', closeHamburgerNavigation);

    return () => {
      window.removeEventListener('orientationchange', closeHamburgerNavigation);
      window.removeEventListener('resize', closeHamburgerNavigation);
    };
  }, [setHamburgerMenuIsOpen]);

  return (
    <>
      <header className="fixed left-0 top-0 z-50 w-full px-4 animate-fade-in border-b opacity-0 backdrop-blur-[12px] [--animation-delay:600ms]">
        <div className="container mx-auto flex h-[var(--navigation-height)] w-full items-center justify-between">
          <Link className="text-md flex items-center justify-center" href="/">
            Magic UI
          </Link>

          <div className="ml-auto flex h-full items-center">
            <Link className="mr-6 text-sm" href="/signin">
              Log in
            </Link>
            <Link
              className={cn(
                buttonVariants({ variant: 'secondary' }),
                'mr-6 text-sm',
              )}
              href="/signup"
            >
              Sign up
            </Link>
          </div>
          <button
            className="ml-6 md:hidden"
            onClick={() => setHamburgerMenuIsOpen((open) => !open)}
          >
            <span className="sr-only">Toggle menu</span>
            {hamburgerMenuIsOpen ? <XIcon /> : <AlignJustify />}
          </button>
        </div>
      </header>
      <AnimatePresence>
        <motion.nav
          animate={hamburgerMenuIsOpen ? 'animate' : 'exit'}
          className={cn(
            'fixed left-0 top-0 z-50 h-screen w-full overflow-auto bg-background/70 backdrop-blur-[12px] ',
            {
              'pointer-events-none': !hamburgerMenuIsOpen,
            },
          )}
          exit="exit"
          initial="initial"
          variants={mobilenavbarVariant}
        >
          <div className="container mx-auto flex h-[var(--navigation-height)] items-center justify-between">
            <Link className="text-md flex items-center" href="/">
              Magic UI
            </Link>

            <button
              className="ml-6 md:hidden"
              onClick={() => setHamburgerMenuIsOpen((open) => !open)}
            >
              <span className="sr-only">Toggle menu</span>
              {hamburgerMenuIsOpen ? <XIcon /> : <AlignJustify />}
            </button>
          </div>
          <motion.ul
            animate={hamburgerMenuIsOpen ? 'open' : 'exit'}
            className={
              'flex flex-col md:flex-row md:items-center uppercase md:normal-case ease-in'
            }
            initial="initial"
            variants={containerVariants}
          >
            {menuItem.map((item) => (
              <motion.li
                className="border-grey-dark pl-6 py-0.5 border-b md:border-none"
                key={item.id}
                variants={mobileLinkVar}
              >
                <Link
                  className={`hover:text-grey flex h-[var(--navigation-height)] w-full items-center text-xl transition-[color,transform] duration-300 md:translate-y-0 md:text-sm md:transition-colors ${
                    hamburgerMenuIsOpen ? '[&_a]:translate-y-0' : ''
                  }`}
                  href={item.href}
                >
                  {item.label}
                </Link>
              </motion.li>
            ))}
          </motion.ul>
        </motion.nav>
      </AnimatePresence>
    </>
  );
}
