import { useEffect, useState } from 'react';

export function useHideOnScroll(props: {
  scrollHeight: number;
  scrollUpHeight?: number;
}) {
  const [scrollDelta, setScrollDelta] = useState({
    lastY: 0,
    y: 0,
  });
  const [hidden, setHidden] = useState(false);
  const [scrollUpDistance, setScrollUpDistance] = useState(0);

  useEffect(() => {
    const handleScroll = () => {
      setScrollDelta((previous) => {
        const newY = window.scrollY;
        const isScrollingUp = newY < previous.y;

        if (isScrollingUp) {
          setScrollUpDistance((previous_) => previous_ + (previous.y - newY));
        } else {
          setScrollUpDistance(0);
        }

        return {
          lastY: previous.y,
          y: newY,
        };
      });
    };

    window.addEventListener('scroll', handleScroll);

    return () => {
      window.removeEventListener('scroll', handleScroll);
    };
  }, []);

  useEffect(() => {
    const scrollUpHeight = props.scrollUpHeight ?? 0;

    if (scrollDelta.y > props.scrollHeight) {
      if (scrollDelta.y > scrollDelta.lastY) {
        setHidden(true);
      } else if (scrollUpDistance >= scrollUpHeight) {
        setHidden(false);
      }
    } else {
      setHidden(false);
    }
  }, [scrollDelta, scrollUpDistance, props.scrollHeight, props.scrollUpHeight]);

  return { hidden };
}
