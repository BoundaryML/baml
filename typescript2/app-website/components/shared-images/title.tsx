interface TitleProps {
  title: string;
  subtitle?: string;
  align?: 'start' | 'center';
}

export function Title({ title, subtitle, align = 'start' }: TitleProps) {
  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: align === 'center' ? 'center' : 'flex-start',
        gap: '24px',
        width: '100%',
      }}
    >
      <div
        style={{
          fontSize: 80,
          fontWeight: 'bold',
          color: 'white',
          lineHeight: 1.2,
          textShadow: '0 2px 4px rgba(0,0,0,0.5)',
          width: '100%',
          textAlign: align === 'center' ? 'center' : 'left',
        }}
      >
        {title}
      </div>
      {subtitle && (
        <div
          style={{
            fontSize: 48,
            lineHeight: 1.2,
            color: '#cccccc',
            textShadow: '0 1px 2px rgba(0,0,0,0.5)',
            textAlign: align === 'center' ? 'center' : 'left',
          }}
        >
          {subtitle}
        </div>
      )}
    </div>
  );
}
