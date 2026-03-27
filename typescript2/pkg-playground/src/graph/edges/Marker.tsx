// Arrow marker colors for light and dark modes
const kBaseMarkerColorDark = '#cbd5e1'; // slate-300
const kYesMarkerColorDark = '#4ade80';  // green-400
const kNoMarkerColorDark = '#f87171';   // red-400

const kBaseMarkerColorLight = '#0f172a'; // slate-900
const kYesMarkerColorLight = '#16a34a';  // green-600
const kNoMarkerColorLight = '#dc2626';   // red-600

const kBaseMarkerColorsDark = ['#a78bfa', '#f472b6', '#fbbf24', '#60a5fa'];
const kBaseMarkerColorsLight = ['#6d28d9', '#db2777', '#d97706', '#2563eb'];

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
  const isDark = typeof document === 'undefined' || !document.body?.dataset?.vscodeThemeKind?.includes('light');
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
