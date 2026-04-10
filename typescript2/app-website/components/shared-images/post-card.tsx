/** biome-ignore-all lint/performance/noImgElement: <explanation> */
interface PostCardProps {
  title: string;
  label?: string;
  author?: {
    name: string;
    imageUrl?: string;
  };
}

export function PostCard({
  title,
  label = 'Latest Post',
  author,
}: PostCardProps) {
  return (
    <div
      style={{
        backdropFilter: 'blur(8px)',
        background: 'rgba(0,0,0,0.5)',
        border: '1px solid rgba(255,255,255,0.1)',
        borderRadius: 12,
        color: 'white',
        display: 'flex',
        flexDirection: 'column',
        fontSize: 36,
        gap: '8px',
        maxWidth: '60%',
        padding: '24px 32px',
      }}
    >
      <div
        style={{
          color: '#cccccc',
          display: 'flex',
          fontSize: 24,
        }}
      >
        {label}
      </div>
      <div
        style={{
          display: 'flex',
        }}
      >
        {title}
      </div>
      {author && (
        <div
          style={{
            alignItems: 'center',
            display: 'flex',
            gap: '16px',
            marginTop: '16px',
          }}
        >
          {author.imageUrl && (
            <img
              alt={`Avatar of ${author.name}`}
              height="48"
              src={`https://www.boundaryml.com${author.imageUrl}`}
              style={{
                border: '2px solid rgba(255,255,255,0.2)',
                borderRadius: 24,
              }}
              width="48"
            />
          )}
          <div
            style={{
              color: '#cccccc',
              display: 'flex',
              fontSize: 24,
            }}
          >
            By {author.name}
          </div>
        </div>
      )}
    </div>
  );
}
