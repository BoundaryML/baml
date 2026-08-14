import { useEffect, useState } from 'react';

export interface UseMediaQueryProps {
  isSsrMobile?: boolean;
  query: string;
}

export function useMediaQuery({ query, isSsrMobile }: UseMediaQueryProps) {
  const [value, setValue] = useState(isSsrMobile ?? false);

  useEffect(() => {
    function onChange(event: MediaQueryListEvent) {
      setValue(event.matches);
    }

    const result = matchMedia(query);
    result.addEventListener('change', onChange);

    if (result.matches !== value) {
      setValue(result.matches);
    }

    return () => result.removeEventListener('change', onChange);
  }, [query, value]);

  return value;
}

export function useIsMobile(props?: { isSsrMobile?: boolean }) {
  return useMediaQuery({
    isSsrMobile: props?.isSsrMobile,
    query: '(max-width: 640px)',
  });
}

export function useIsDesktop(props?: { isSsrMobile?: boolean }) {
  return useMediaQuery({
    isSsrMobile: props?.isSsrMobile,
    query: '(min-width: 1024px)',
  });
}

export function useIsTablet(props?: { isSsrMobile?: boolean }) {
  return useMediaQuery({
    isSsrMobile: props?.isSsrMobile,
    query: '(min-width: 641px) and (max-width: 1023px)',
  });
}
