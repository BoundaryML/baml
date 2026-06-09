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

export const getMarkerColors = () => {
  // Outside VS Code (no theme dataset) the panel is the light paper theme,
  // so default to LIGHT; inside VS Code, follow the editor's theme kind.
  const themeKind =
    typeof document === 'undefined'
      ? undefined
      : document.body?.dataset?.vscodeThemeKind;
  const isDark = themeKind ? !themeKind.includes('light') : false;
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
