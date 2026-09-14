import { ImageResponse } from 'next/og';

export const alt = 'BAML Developer';
export const contentType = 'image/png';
export const dynamic = 'force-static';
export const size = {
  height: 180,
  width: 180,
};

export default function AppleIcon() {
  return new ImageResponse(
    <div
      style={{
        alignItems: 'center',
        background: '#0a0a0a',
        borderRadius: 36,
        color: '#a78bfa',
        display: 'flex',
        fontFamily: 'monospace',
        fontSize: 88,
        fontWeight: 700,
        height: '100%',
        justifyContent: 'center',
        letterSpacing: '-0.12em',
        paddingRight: 10,
        width: '100%',
      }}
    >
      {'{}'}
    </div>,
    size,
  );
}
