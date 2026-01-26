// Light mode colors (darker for visibility on light background)
export const kBaseMarkerColorLight = '#0f172a'; // slate-900 (much darker)
export const kYesMarkerColorLight = '#16a34a'; // green-600 (darker green)
export const kNoMarkerColorLight = '#dc2626'; // red-600 (darker red)
export const kBaseMarkerColorsLight = ['#6d28d9', '#db2777', '#d97706', '#2563eb']; // darker vibrant colors

// Dark mode colors (lighter for visibility on dark background)
export const kBaseMarkerColorDark = '#cbd5e1'; // slate-300
export const kYesMarkerColorDark = '#4ade80'; // green-400
export const kNoMarkerColorDark = '#f87171'; // red-400
export const kBaseMarkerColorsDark = ['#a78bfa', '#f472b6', '#fbbf24', '#60a5fa']; // lighter vibrant colors

// Helper to get current theme colors
export const getMarkerColors = () => {
  const isDark = document.documentElement.classList.contains('dark');
  return {
    base: isDark ? kBaseMarkerColorDark : kBaseMarkerColorLight,
    yes: isDark ? kYesMarkerColorDark : kYesMarkerColorLight,
    no: isDark ? kNoMarkerColorDark : kNoMarkerColorLight,
    colors: isDark ? kBaseMarkerColorsDark : kBaseMarkerColorsLight,
  };
};

// Export all colors for marker definitions
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

export const ColorfulMarkerDefinitions = () => {
  return (
    <svg style={{ position: 'absolute', top: 0, left: 0 }}>
      <defs>
        {kAllMarkerColors.map((color) => (
          <Marker color={color} id={color.replace('#', '')} key={color} />
        ))}
      </defs>
    </svg>
  );
};

const Marker = ({
  id,
  color,
  strokeWidth = 1,
  width = 12.5,
  height = 12.5,
  markerUnits = 'strokeWidth',
  orient = 'auto-start-reverse',
}: any) => {
  return (
    <marker
      id={id}
      markerHeight={`${height}`}
      markerUnits={markerUnits}
      markerWidth={`${width}`}
      orient={orient}
      refX="0"
      refY="0"
      viewBox="-10 -10 20 20"
    >
      <polyline
        points="-5,-4 0,0 -5,4 -5,-4"
        strokeLinecap="round"
        strokeLinejoin="round"
        style={{
          stroke: color,
          fill: color,
          strokeWidth,
        }}
      />
    </marker>
  );
};
