import type { ClassValue } from 'clsx';
import { clsx } from 'clsx';
import { createTwc } from 'react-twc';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

// https://react-twc.vercel.app/docs/guides/class-name-prop#solve-class-conflicts
// By default classes are not merged. If you want to merge classes, you can use twc with tailwind-merge
export const twx = createTwc({ compose: cn });
