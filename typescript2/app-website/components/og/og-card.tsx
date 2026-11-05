/* eslint-disable @next/next/no-img-element */

// The shared link-preview card, in the site's dark brand: the Boundary logo and
// a Lexend headline beside a brand painting. `avatars` adds host avatars under
// the headline (/podcast), and `photo` replaces the painting (/who-are-we).
// Rendered to PNG via `next/og`.

export const OG_SIZE = { height: 630, width: 1200 };
export const OG_CONTENT_TYPE = 'image/png';

const BLACK = '#0F0F0F';
const HEAD = '#FEFEFE';
const BODY = '#ABABA6';
const MUTED = '#828280';

export interface OgCardProps {
  /** Headline, set in Lexend. */
  title: string;
  /** Optional supporting line. Use only for a real fact, not flavor. */
  description?: string;
  /** Bottom line, e.g. "boundaryml.com" or "Author · boundaryml.com". */
  footer: string;
  /** White Boundary logo as a data URI. */
  logo: string;
  /** Painting beside the text, as a data URI. */
  painting: string;
  /** Pre-computed headline size so long titles stay on the card. */
  titleFontSize: number;
  /** Host avatars with handles, shown under the headline (/podcast). */
  avatars?: { src: string; handle: string }[];
  /** A photo that replaces the painting (/who-are-we). */
  photo?: string;
}

export function OgCard({
  title,
  description,
  footer,
  logo,
  painting,
  titleFontSize,
  avatars,
  photo,
}: OgCardProps) {
  const imageWidth = photo ? 620 : 400;

  return (
    <div
      style={{
        backgroundColor: BLACK,
        display: 'flex',
        fontFamily: 'Geist',
        height: '100%',
        width: '100%',
      }}
    >
      <div
        style={{
          display: 'flex',
          flex: 1,
          flexDirection: 'column',
          padding: '60px 64px',
        }}
      >
        <img alt="Boundary" height={40} src={logo} width={234} />
        <div
          style={{
            display: 'flex',
            flex: 1,
            flexDirection: 'column',
            justifyContent: 'center',
          }}
        >
          <div
            style={{
              color: HEAD,
              display: 'flex',
              fontFamily: 'Lexend',
              fontSize: titleFontSize,
              fontWeight: 500,
              letterSpacing: -1,
              lineHeight: 1.12,
            }}
          >
            {title}
          </div>
          {description ? (
            <div
              style={{
                color: BODY,
                display: 'flex',
                fontSize: 26,
                lineHeight: 1.4,
                marginTop: 24,
              }}
            >
              {description}
            </div>
          ) : null}
          {avatars?.length ? (
            <div style={{ display: 'flex', gap: 36, marginTop: 36 }}>
              {avatars.map((a) => (
                <div
                  key={a.handle}
                  style={{ alignItems: 'center', display: 'flex', gap: 16 }}
                >
                  <img
                    alt={a.handle}
                    height={72}
                    src={a.src}
                    style={{ borderRadius: 999 }}
                    width={72}
                  />
                  <div style={{ color: BODY, fontSize: 22 }}>{a.handle}</div>
                </div>
              ))}
            </div>
          ) : null}
        </div>
        <div style={{ color: MUTED, display: 'flex', fontSize: 22 }}>
          {footer}
        </div>
      </div>
      <img
        alt=""
        height={630}
        src={photo ?? painting}
        style={{ height: 630, objectFit: 'cover', width: imageWidth }}
        width={imageWidth}
      />
    </div>
  );
}
