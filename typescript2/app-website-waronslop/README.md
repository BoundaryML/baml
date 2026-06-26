# app-website-waronslop

The marketing site for **waronslop.com** — a campaign against the great unwashed
tide of AI slop, led by a standing legion of Roman troops.

The centerpiece is `src/components/BattleScene.tsx`: an animated SVG of
legionaries (LEGIO XII) marching east to meet the Slopmonsters. It's adapted
from the "Animated Roman Troops" Figma Make file and respects
`prefers-reduced-motion`.

## Stack

- Next.js 14 (App Router)
- React 18
- Tailwind CSS v4 (via `@tailwindcss/postcss`)

## Develop

```bash
pnpm install        # from the typescript2 workspace root
pnpm --filter app-website-waronslop dev
```

Then open http://localhost:3000.

## Build

```bash
pnpm --filter app-website-waronslop build
pnpm --filter app-website-waronslop start
```
