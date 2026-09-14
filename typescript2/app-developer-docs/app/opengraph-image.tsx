import { ImageResponse } from 'next/og';

export const dynamic = 'force-static';
export const alt = 'BAML Developer Documentation';
export const contentType = 'image/png';
export const size = {
  height: 630,
  width: 1200,
};

export default function OpenGraphImage() {
  return new ImageResponse(
    <div
      style={{
        alignItems: 'center',
        background: '#0a0a0a',
        color: '#fafafa',
        display: 'flex',
        height: '100%',
        justifyContent: 'center',
        width: '100%',
      }}
    >
      <div
        style={{
          alignItems: 'center',
          display: 'flex',
          flexDirection: 'column',
          gap: 28,
          maxWidth: 920,
          textAlign: 'center',
        }}
      >
        <div style={{ color: '#9b6cff', display: 'flex', fontSize: 64 }}>
          {'{ }'}
        </div>
        <div style={{ display: 'flex', fontSize: 72, fontWeight: 700 }}>
          BAML Developer
        </div>
        <div style={{ color: '#a3a3a3', display: 'flex', fontSize: 32 }}>
          Build reliable AI applications with BAML.
        </div>
      </div>
    </div>,
    size,
  );
}
