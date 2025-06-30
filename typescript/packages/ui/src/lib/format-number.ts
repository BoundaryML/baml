export function formatNumber(
  value: number,
  {
    maximumFractionDigits = 0,
    minimumFractionDigits = 0,
  }: {
    maximumFractionDigits?: number;
    minimumFractionDigits?: number;
  } = {},
): string {
  return new Intl.NumberFormat('en-US', {
    compactDisplay: 'short',
    maximumFractionDigits,
    minimumFractionDigits,
    notation: 'compact',
    style: 'decimal',
  }).format(value);
}
