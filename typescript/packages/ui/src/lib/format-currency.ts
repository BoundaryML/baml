export const formatCurrency = ({
  amount,
  currency = 'USD',
  locale = 'en-US',
  compact = true,
}: {
  amount?: number | null;
  currency?: string;
  locale?: string;
  compact?: boolean;
}): string => {
  if (amount === null || amount === undefined) {
    return 'N/A';
  }

  return new Intl.NumberFormat(locale, {
    compactDisplay: compact ? 'short' : 'long',
    currency,
    maximumFractionDigits: compact ? 0 : 2,
    minimumFractionDigits: 0,
    notation: compact ? 'compact' : 'standard',
    style: 'currency',
  }).format(amount);
};
