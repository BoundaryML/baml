export function formatPercentage(value: number): string {
  return new Intl.NumberFormat('en-US', {
    maximumFractionDigits: 2,
    minimumFractionDigits: 0,
    style: 'percent',
  }).format(value);
}
