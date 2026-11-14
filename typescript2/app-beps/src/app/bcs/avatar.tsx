"use client";

// A member's GitHub picture, from their login: GitHub serves every account's at github.com/<login>.png.
// If it cannot load, the member's initial in a colored circle, as BEP comments show authors.
import { useEffect, useRef, useState } from "react";

const COLORS = [
  "bg-blue-500",
  "bg-green-500",
  "bg-purple-500",
  "bg-pink-500",
  "bg-indigo-500",
  "bg-teal-500",
  "bg-orange-500",
  "bg-cyan-500",
];

const SIZES = { xs: 16, sm: 20, md: 24 } as const;

export function GitHubAvatar({ login, size = "md" }: { login: string; size?: keyof typeof SIZES }) {
  const [failed, setFailed] = useState(false);
  const image = useRef<HTMLImageElement>(null);
  // An image that failed before React attached onError (during hydration) reports nothing: check.
  useEffect(() => {
    const element = image.current;
    if (element?.complete && element.naturalWidth === 0) setFailed(true);
  }, []);
  const pixels = SIZES[size];
  const style = { width: pixels, height: pixels };
  if (failed || !/^[A-Za-z0-9-]+$/.test(login)) {
    const color = COLORS[[...login].reduce((sum, char) => sum + char.charCodeAt(0), 0) % COLORS.length];
    return (
      <span
        aria-hidden
        style={{ ...style, fontSize: pixels * 0.5 }}
        className={`inline-flex shrink-0 items-center justify-center rounded-full font-medium text-white ${color}`}
      >
        {login[0]?.toUpperCase() ?? "?"}
      </span>
    );
  }
  return (
    // A plain image: GitHub resizes it, so Next's image optimizer would only add a hop.
    // eslint-disable-next-line @next/next/no-img-element
    <img
      ref={image}
      src={`https://github.com/${login}.png?size=${pixels * 2}`}
      alt=""
      aria-hidden
      loading="lazy"
      style={style}
      onError={() => setFailed(true)}
      className="shrink-0 rounded-full bg-muted"
    />
  );
}
