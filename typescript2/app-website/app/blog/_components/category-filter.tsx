'use client';

import { Button } from '@/components/ui/button';

const categories = [
  'All',
  'Announcements',
  'Tutorials',
  'Research',
  'Engineering',
  'LaunchWeek',
];

const getCategoryStyles = (category: string) => {
  // Convert tag to camelCase format for style lookup
  const normalizeCategory = (cat: string) => {
    if (cat.toLowerCase() === 'launch week') return 'LaunchWeek';
    return cat.charAt(0).toUpperCase() + cat.slice(1).toLowerCase();
  };

  const normalizedCategory = normalizeCategory(category);

  const styles = {
    All: { backgroundColor: '#f1f5f9', color: '#1e293b' },
    Announcements: { backgroundColor: '#dbeafe', color: '#1e40af' },
    Engineering: { backgroundColor: '#fed7aa', color: '#ea580c' },
    LaunchWeek: { backgroundColor: '#fce7f3', color: '#be185d' },
    Research: { backgroundColor: '#f3e8ff', color: '#7c3aed' },
    Tutorials: { backgroundColor: '#dcfce7', color: '#166534' },
  };
  return styles[normalizedCategory as keyof typeof styles] || styles['All'];
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
    <div className="w-full">
      <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
        {categories.map((category) => (
          <Button
            className="rounded-lg whitespace-nowrap border transition-all duration-200 hover:scale-105"
            key={category}
            onClick={() => onCategoryChange(category)}
            size="sm"
            style={
              category === selectedCategory ? getCategoryStyles(category) : {}
            }
            variant="ghost"
          >
            {formatCategoryForDisplay(category)}
          </Button>
        ))}
      </div>
    </div>
  );
}

export { getCategoryStyles, formatCategoryForDisplay };
