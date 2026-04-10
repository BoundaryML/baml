export const size = {
  height: 630,
  width: 1200,
};

export const contentType = 'image/png';

interface BaseLayoutProps {
  children: React.ReactNode;
  backgroundImage?: string;
  bamlImage?: string;
  featuredImage?: string | null;
  baseUrl?: string;
}

export function BaseLayout({
  children,
  backgroundImage = 'baml-og-background.png',
  bamlImage = 'baml-logo-with-lamb.png',
  featuredImage,
  baseUrl: baseUrlProp,
}: BaseLayoutProps) {
  const baseUrl = baseUrlProp ??
    process.env.NEXT_PUBLIC_BASE_URL ??
    (process.env.VERCEL_PROJECT_PRODUCTION_URL
      ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
      : 'http://localhost:3000');

  // If a background image is provided (not the default), return it directly without overlays
  if (backgroundImage !== 'baml-og-background.png') {
    return (
      <img
        alt="Background"
        height="630px"
        src={`${baseUrl}/${backgroundImage}`}
        style={{
          height: '100%',
          objectFit: 'cover',
          width: '100%',
        }}
        width="1200px"
      />
    );
  }

  return (
    <div
      style={{
        backgroundColor: '#0E0E0E',
        display: 'flex',
        height: '100%',
        position: 'relative',
        width: '100%',
      }}
    >
      <img
        alt="Background"
        height="630px"
        src={`${baseUrl}/${backgroundImage}`}
        style={{
          backgroundColor: 'transparent',
          left: 0,
          objectFit: 'cover',
          position: 'absolute',
          top: 0,
        }}
        width="1200px"
      />
      {/* Add a semi-transparent overlay to ensure text readability */}
      <div
        style={{
          background: 'linear-gradient(45deg, rgba(30,13,46,0.3), transparent)',
          bottom: 0,
          left: 0,
          position: 'absolute',
          right: 0,
          top: 0,
          zIndex: 1,
        }}
      />
      {/* BAML Logo in bottom right */}
      <img
        alt="BAML Logo"
        height="125px"
        src={`${baseUrl}/${bamlImage}`}
        style={{
          bottom: '40px',
          position: 'absolute',
          right: '40px',
          zIndex: 3,
        }}
        width="385px"
      />
      {/* Featured image on the right side */}
      {featuredImage && (
        <div
          style={{
            alignItems: 'center',
            display: 'flex',
            height: '100%',
            justifyContent: 'center',
            padding: '40px',
            position: 'absolute',
            right: 0,
            top: 0,
            width: '45%',
            zIndex: 2,
          }}
        >
          <img
            alt="Featured"
            src={featuredImage.startsWith('http') ? featuredImage : `${baseUrl}${featuredImage}`}
            style={{
              border: '4px solid rgba(255,255,255,0.15)',
              borderRadius: 16,
              boxShadow: '0 20px 40px rgba(0,0,0,0.4)',
              height: 'auto',
              maxHeight: '85%',
              maxWidth: '100%',
              objectFit: 'contain',
            }}
          />
        </div>
      )}
      {/* Content wrapper */}
      <div
        style={{
          alignItems: 'flex-start',
          display: 'flex',
          flexDirection: 'column',
          height: '100%',
          maxWidth: featuredImage ? '50%' : '90%',
          padding: '40px 40px 40px 40px',
          position: 'relative',
          width: '100%',
          zIndex: 2,
        }}
      >
        {children}
      </div>
    </div>
  );
}
