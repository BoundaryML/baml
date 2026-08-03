'use client';

const categories = [
  'All',
  'Announcements',
  'Tutorials',
  'Research',
  'Engineering',
  'LaunchWeek',
];

const INK = '#1A1612';
const MUTED = '#5C5852';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const CARD_BG = '#FBF8F1';

const getCategoryStyles = (category: string) => {
  // Retained for legacy callers — returns subtle accent style.
  return { backgroundColor: ACCENT, color: '#ffffff' };
};

const formatCategoryForDisplay = (category: string) => {
  if (category.toLowerCase() === 'launch week' || category === 'LaunchWeek')
    return 'Launch Week';
  return category.charAt(0).toUpperCase() + category.slice(1).toLowerCase();
};

interface CategoryFilterProps {
  selectedCategory: string;
  onCategoryChange: (category: string) => void;
}

export function CategoryFilter({
  selectedCategory,
  onCategoryChange,
}: CategoryFilterProps) {
  return (
    <div
      style={{
        display: 'flex',
        flexWrap: 'wrap',
        gap: 8,
      }}
    >
      {categories.map((category) => {
        const active = category === selectedCategory;
        return (
          <button
            type="button"
            key={category}
            onClick={() => onCategoryChange(category)}
            style={{
              padding: '8px 14px',
              fontSize: 13,
              fontWeight: 500,
              letterSpacing: '0.01em',
              color: active ? '#ffffff' : INK,
              background: active ? ACCENT : '#ffffff',
              border: `1px solid ${active ? ACCENT : BORDER}`,
              borderRadius: 999,
              cursor: 'pointer',
              transition:
                'background-color 200ms ease, color 200ms ease, border-color 200ms ease',
            }}
            onMouseEnter={(e) => {
              if (!active) {
                e.currentTarget.style.backgroundColor = CARD_BG;
                e.currentTarget.style.color = INK;
              }
            }}
            onMouseLeave={(e) => {
              if (!active) {
                e.currentTarget.style.backgroundColor = '#ffffff';
                e.currentTarget.style.color = INK;
              }
            }}
          >
            {formatCategoryForDisplay(category)}
          </button>
        );
      })}
    </div>
  );
}

export { getCategoryStyles, formatCategoryForDisplay };
