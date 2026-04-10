interface AuthorCardProps {
  name: string;
  imageUrl?: string;
  date?: string;
  label?: string;
}

export function AuthorCard({ name, imageUrl, date, label }: AuthorCardProps) {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'stretch',
        background: 'rgba(0,0,0,0.5)',
        paddingRight: '32px',
        maxWidth: '66%',
        borderRadius: 12,
        backdropFilter: 'blur(8px)',
        border: '1px solid rgba(255,255,255,0.1)',
      }}
    >
      {imageUrl && (
        <div
          style={{
            display: 'flex',
            width: '140px',
            borderTopLeftRadius: 11,
            borderBottomLeftRadius: 11,
            overflow: 'hidden',
            flexShrink: 0,
          }}
        >
          <img
            src={`https://www.boundaryml.com${imageUrl}`}
            alt={`Avatar of ${name}`}
            style={{
              width: '100%',
              height: '100%',
              objectFit: 'cover',
            }}
          />
        </div>
      )}
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: '8px',
          padding: '24px 0 24px 32px',
        }}
      >
        {label && (
          <div
            style={{
              fontSize: 24,
              color: '#cccccc',
            }}
          >
            {label}
          </div>
        )}
        <div
          style={{
            fontSize: 36,
            color: 'white',
          }}
        >
          {name}
        </div>
        {date && (
          <div
            style={{
              fontSize: 24,
              color: '#cccccc',
            }}
          >
            {date}
          </div>
        )}
      </div>
    </div>
  );
}
