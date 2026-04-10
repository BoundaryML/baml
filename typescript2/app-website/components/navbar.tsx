'use client';

import { Menu, X } from 'lucide-react';
import { AnimatePresence, motion, useScroll } from 'motion/react';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { siteConfig } from '@/app/_lib/config';
// import { GitHubStarsButtonWrapper } from '@/components/ui/custom/github-stars-button/button-wrapper';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { GithubStars } from './GithubStars';

// import { Icons } from '../todo/(marketing)/_components/icons';

const INITIAL_WIDTH = '70rem';
const MAX_WIDTH = '70rem'; /* keep same so nav never shrinks and clips links */

// Animation variants
const overlayVariants = {
  exit: { opacity: 0 },
  hidden: { opacity: 0 },
  visible: { opacity: 1 },
};

const drawerVariants = {
  exit: {
    opacity: 0,
    transition: { duration: 0.1 },
    y: 100,
  },
  hidden: { opacity: 0, y: 100 },
  visible: {
    opacity: 1,
    rotate: 0,
    transition: {
      damping: 15,
      staggerChildren: 0.03,
      stiffness: 200,
    },
    y: 0,
  },
};

const drawerMenuContainerVariants = {
  hidden: { opacity: 0 },
  visible: { opacity: 1 },
};

const drawerMenuVariants = {
  hidden: { opacity: 0 },
  visible: { opacity: 1 },
};

// Logo component
function Logo() {
  return (
    <Link className="flex items-center gap-1" href="/">
      {/* <Icons.logo className="size-12" /> */}
      <div className="flex items-center gap-2">
        <p className="text-lg font-semibold text-primary">Boundary</p>
        {/* <Badge variant="secondary">Beta</Badge> */}
      </div>
    </Link>
  );
}

// Desktop action buttons component
function DesktopActionButtons() {
  return (
    <div className="flex items-center space-x-4">
      {/* <Link
        className="bg-secondary h-8 hidden md:flex items-center justify-center text-sm font-normal tracking-wide rounded-full text-primary-foreground dark:text-secondary-foreground w-fit px-4 shadow-[inset_0_1px_2px_rgba(255,255,255,0.25),0_3px_3px_-1.5px_rgba(16,24,40,0.06),0_1px_1px_rgba(16,24,40,0.08)] border border-white/[0.12]"
        href="/docs/getting-started?utm_source=marketing-site&utm_medium=navbar-get-started"
      >
        Get Started
      </Link> */}
      <Button asChild className="rounded-full" variant="secondary">
        <Link
          href="https://docs.boundaryml.com/?utm_source=marketing-site&utm_medium=navbar-docs"
          target="_blank"
        >
          Docs
        </Link>
      </Button>
      {/* <Button asChild className="hidden md:flex rounded-full" variant="outline">
        <Link href="https://github.com/boundaryml/baml?utm_source=marketing-site&utm_medium=navbar-github">
          GitHub
        </Link>
      </Button> */}
      <GithubStars />
    </div>
  );
}

// Mobile menu toggle button
function MobileMenuToggle({
  isOpen,
  onToggle,
}: {
  isOpen: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      aria-label="Toggle navigation"
      className="md:hidden border border-border size-8 rounded-md cursor-pointer flex items-center justify-center"
      onClick={onToggle}
      type="button"
    >
      {isOpen ? <X className="size-5" /> : <Menu className="size-5" />}
    </button>
  );
}

// Mobile menu item component
function MobileMenuItem({
  item,
  isActive,
  isAnchor,
  onClose,
}: {
  item: { href: string; id: number; name: string };
  isActive: boolean;
  isAnchor: boolean;
  onClose: () => void;
}) {
  if (isAnchor) {
    return (
      <a
        className={`underline-offset-4 hover:text-primary/80 transition-colors ${
          isActive ? 'text-primary font-medium' : 'text-primary/60'
        }`}
        href={item.href}
        onClick={(e) => {
          e.preventDefault();
          const element = document.getElementById(item.href.substring(1));
          element?.scrollIntoView({ behavior: 'smooth' });
          onClose();
        }}
      >
        {item.name}
      </a>
    );
  }

  return (
    <Link
      className="underline-offset-4 hover:text-primary/80 transition-colors text-primary/60"
      href={item.href}
      onClick={onClose}
    >
      {item.name}
    </Link>
  );
}

// Mobile menu content
function MobileMenuContent({
  isOpen,
  onClose,
  activeSection,
}: {
  isOpen: boolean;
  onClose: () => void;
  activeSection: string;
}) {
  const isAnchorLink = (href: string) => href.startsWith('#');

  return (
    <AnimatePresence>
      {isOpen && (
        <>
          <motion.div
            animate="visible"
            className="fixed inset-0 bg-black/50 backdrop-blur-sm"
            exit="exit"
            initial="hidden"
            onClick={onClose}
            transition={{ duration: 0.2 }}
            variants={overlayVariants}
          />

          <motion.div
            animate="visible"
            className="fixed inset-x-0 w-[95%] mx-auto bottom-3 bg-background border border-border p-4 rounded-xl shadow-lg"
            exit="exit"
            initial="hidden"
            variants={drawerVariants}
          >
            <div className="flex flex-col gap-4">
              <div className="flex items-center justify-between">
                <Link className="flex items-center gap-3" href="/">
                  {/* <Icons.logo className="size-7 md:size-10" /> */}
                  <p className="text-lg font-semibold text-primary">BAML</p>
                </Link>
                <button
                  aria-label="Close navigation menu"
                  className="border border-border rounded-md p-1 cursor-pointer"
                  onClick={onClose}
                  type="button"
                >
                  <X className="size-5" />
                </button>
              </div>

              <motion.ul
                className="flex flex-col text-sm mb-4 border border-border rounded-md"
                variants={drawerMenuContainerVariants}
              >
                <AnimatePresence>
                  {siteConfig.nav.links.map((item) => {
                    const isAnchor = isAnchorLink(item.href);
                    const isActive = false; // Remove anchor-based active state

                    return (
                      <motion.li
                        className="p-2.5 border-b border-border last:border-b-0"
                        key={item.id}
                        variants={drawerMenuVariants}
                      >
                        <MobileMenuItem
                          isActive={isActive}
                          isAnchor={isAnchor}
                          item={item}
                          onClose={onClose}
                        />
                      </motion.li>
                    );
                  })}
                </AnimatePresence>
              </motion.ul>

              <div className="flex flex-col gap-2">
                <Link
                  className="bg-secondary h-8 flex items-center justify-center text-sm font-normal tracking-wide rounded-full text-primary-foreground dark:text-secondary-foreground w-full px-4 shadow-[inset_0_1px_2px_rgba(255,255,255,0.25),0_3px_3px_-1.5px_rgba(16,24,40,0.06),0_1px_1px_rgba(16,24,40,0.08)] border border-white/[0.12] hover:bg-secondary/80 transition-all ease-out active:scale-95"
                  href="https://docs.boundaryml.com/home"
                >
                  Get Started
                </Link>
                <Button asChild className="rounded-full" variant="outline">
                  <a href="https://docs.boundaryml.com/home?utm_source=marketing-site&utm_medium=navbar-docs">
                    Documentation
                  </a>
                </Button>
                <GithubStars />
              </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}

// Right side controls (GitHub stars, theme toggle, mobile menu)
function RightSideControls({
  isDrawerOpen,
  toggleDrawer,
}: {
  isDrawerOpen: boolean;
  toggleDrawer: () => void;
}) {
  return (
    <div className="flex flex-row items-center gap-1 md:gap-3 shrink-0">
      <DesktopActionButtons />
      {/* <GitHubStarsButtonWrapper
        className="rounded-full"
        repo="boundaryml/baml"
      /> */}
      {/* <ThemeToggle className="rounded-full" mode="toggle" /> */}
      <MobileMenuToggle isOpen={isDrawerOpen} onToggle={toggleDrawer} />
    </div>
  );
}

// Main navbar header component
function NavbarHeader({
  hasScrolled,
  isDrawerOpen,
  toggleDrawer,
  activeSection,
}: {
  hasScrolled: boolean;
  isDrawerOpen: boolean;
  toggleDrawer: () => void;
  activeSection: string;
}) {
  return (
    <motion.header
      className={cn(
        'sticky top-0 z-50 mx-4 flex justify-center transition-all duration-300 md:mx-0',
        hasScrolled ? 'top-4' : 'top-0',
      )}
    >
      <motion.div
        animate={{ width: hasScrolled ? MAX_WIDTH : INITIAL_WIDTH }}
        className="min-w-0 max-w-full"
        initial={{ width: INITIAL_WIDTH }}
        transition={{ duration: 0.3, ease: [0.25, 0.1, 0.25, 1] }}
      >
        <div
          className={cn(
            'mx-auto w-full max-w-7xl rounded-2xl transition-all duration-300 xl:px-0',
            hasScrolled
              ? 'px-4 border border-border backdrop-blur-lg bg-background/75'
              : 'px-7 backdrop-blur-md bg-background/80 border border-transparent',
          )}
        >
          <div className="flex h-[56px] min-w-0 items-center justify-between gap-4 pl-1 md:pl-2 pr-4">
            <Logo />
            <NavigationMenuSection />
            <RightSideControls
              isDrawerOpen={isDrawerOpen}
              toggleDrawer={toggleDrawer}
            />
          </div>
        </div>
      </motion.div>

      <MobileMenuContent
        activeSection={activeSection}
        isOpen={isDrawerOpen}
        onClose={toggleDrawer}
      />
    </motion.header>
  );
}

// Main Navbar component
export function Navbar() {
  const { scrollY } = useScroll();
  const [hasScrolled, setHasScrolled] = useState(false);
  const [isDrawerOpen, setIsDrawerOpen] = useState(false);
  const [activeSection, setActiveSection] = useState('hero');

  // Helper to check if a link is an anchor link
  const isAnchorLink = (href: string) => href.startsWith('#');

  // biome-ignore lint/correctness/useExhaustiveDependencies: will cause infinite loop
  useEffect(() => {
    const handleScroll = () => {
      // Only handle scroll for anchor links
      const anchorLinks = siteConfig.nav.links.filter((item) =>
        isAnchorLink(item.href),
      );
      const sections = anchorLinks.map((item) => item.href.substring(1));

      for (const section of sections) {
        const element = document.getElementById(section);
        if (element) {
          const rect = element.getBoundingClientRect();
          if (rect.top <= 150 && rect.bottom >= 150) {
            setActiveSection(section);
            break;
          }
        }
      }
    };

    window.addEventListener('scroll', handleScroll);
    handleScroll();

    return () => window.removeEventListener('scroll', handleScroll);
  }, []);

  useEffect(() => {
    const unsubscribe = scrollY.on('change', (latest) => {
      setHasScrolled(latest > 10);
    });
    return unsubscribe;
  }, [scrollY]);

  const toggleDrawer = () => setIsDrawerOpen((prev) => !prev);

  return (
    <NavbarHeader
      activeSection={activeSection}
      hasScrolled={hasScrolled}
      isDrawerOpen={isDrawerOpen}
      toggleDrawer={toggleDrawer}
    />
  );
}

function NavigationMenuSection() {
  return (
    <nav className="hidden md:flex items-center gap-6 shrink-0">
      {siteConfig.nav.links.map((link) => (
        <Link
          className="text-sm font-medium text-muted-foreground hover:text-primary transition-colors"
          href={link.href}
          key={link.id}
        >
          {link.name}
        </Link>
      ))}
    </nav>
  );
}
