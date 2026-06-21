import { type GraphTheme, resolveGraphTheme } from '../theme';

// Arrow marker colors for light and dark modes
const kBaseMarkerColorDark = '#cbd5e1'; // slate-300
const kYesMarkerColorDark = '#4ade80'; // green-400
const kNoMarkerColorDark = '#f87171'; // red-400

// Light mode is warm ink on paper — matches the playground panel theme.
const kBaseMarkerColorLight = '#4A443B';
const kYesMarkerColorLight = '#047857';
const kNoMarkerColorLight = '#B42318';

const kBaseMarkerColorsDark = ['#a78bfa', '#f472b6', '#fbbf24', '#60a5fa'];
const kBaseMarkerColorsLight = ['#6D28D9', '#BE185D', '#B45309', '#1D4ED8'];

export const kAllMarkerColors = [
  kBaseMarkerColorLight,
  kBaseMarkerColorDark,
  kYesMarkerColorLight,
  kYesMarkerColorDark,
  kNoMarkerColorLight,
  kNoMarkerColorDark,
  ...kBaseMarkerColorsLight,
  ...kBaseMarkerColorsDark,
];

export const getMarkerColors = (theme?: GraphTheme) => {
  // Follow the shared graph theme resolver: VS Code's theme kind when present,
  // otherwise the resolved panel background (the browser playground defaults
  // to a dark surface), then the OS preference. Callers that already have the
  // theme (edge render, convert) pass it to avoid a per-edge DOM probe.
  const isDark = (theme ?? resolveGraphTheme()) === 'dark';
  return {
    base: isDark ? kBaseMarkerColorDark : kBaseMarkerColorLight,
    yes: isDark ? kYesMarkerColorDark : kYesMarkerColorLight,
    no: isDark ? kNoMarkerColorDark : kNoMarkerColorLight,
    colors: isDark ? kBaseMarkerColorsDark : kBaseMarkerColorsLight,
  };
};

const MarkerDef = ({ id, color }: { id: string; color: string }) => (
  <marker
    id={id}
    markerHeight="12.5"
    markerUnits="strokeWidth"
    markerWidth="12.5"
    orient="auto-start-reverse"
    refX="0"
    refY="0"
    viewBox="-10 -10 20 20"
  >
    <polyline
      points="-5,-4 0,0 -5,4 -5,-4"
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ stroke: color, fill: color, strokeWidth: 1 }}
    />
  </marker>
);

export const ColorfulMarkerDefinitions = () => (
  <svg style={{ position: 'absolute', top: 0, left: 0 }}>
    <defs>
      {kAllMarkerColors.map((color) => (
        <MarkerDef color={color} id={color.replace('#', '')} key={color} />
      ))}
    </defs>
  </svg>
);
