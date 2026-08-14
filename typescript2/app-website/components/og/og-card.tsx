/* eslint-disable @next/next/no-img-element */

// The shared, on-brand link-preview card. A purple rail (cream lamb badge +
// BAML wordmark) beside a cream panel. Most pages show a single centered
// Instrument Serif headline. A few earn a distinctive body:
//   - `timeline`  -> the computing-eras timeline (home + /explore)
//   - `avatars`   -> host avatars under the headline (/podcast)
//   - `photo`     -> a photo that fills the panel (/who-are-we)
// Rendered to PNG via `next/og`.

export const OG_SIZE = { height: 630, width: 1200 };
export const OG_CONTENT_TYPE = 'image/png';

const CREAM = '#FBF7ED';
const INK = '#1A1612';
const MUTED = '#5C5852';
const SOFT = '#8A8580';
const PURPLE = '#7C3AED';
const RULE = '#E2DAC8';

// Each computing paradigm got the language that fit it; agents get BAML.
const ERAS = [
  { era: 'Hardware', lang: 'Assembly', opacity: 0.38 },
  { era: 'Operating Systems', lang: 'Java', opacity: 0.56 },
  { era: 'Web', lang: 'JavaScript', opacity: 0.78 },
  { era: 'Agents', lang: 'BAML', opacity: 1 },
];

export interface OgCardProps {
  /** Optional small uppercase kicker. Most cards leave this empty. */
  eyebrow: string;
  /** Headline, set in Instrument Serif. */
  title: string;
  /** Optional supporting line in mono. Use only for a real fact, not flavor. */
  description?: string;
  /** Bottom-left line, e.g. "boundaryml.com" or "Author · boundaryml.com". */
  footer: string;
  /** Ink lamb as a data URI, for the cream rail badge. */
  lamb: string;
  /** Pre-computed headline size so long titles stay on the card. */
  titleFontSize: number;
  /** Render the horizontal computing-eras timeline. */
  timeline?: boolean;
  /** Host avatars with handles, shown under the headline (/podcast). */
  avatars?: { src: string; handle: string }[];
  /** A photo that fills the panel; headline/kicker are omitted (/who-are-we). */
  photo?: string;
}

export function OgCard({
  eyebrow,
  title,
  description,
  footer,
  lamb,
  titleFontSize,
  timeline,
  avatars,
  photo,
}: OgCardProps) {
  const hasAvatars = !!avatars && avatars.length > 0;
  // Headline (with or without a short subtext) is centered so it owns the
  // frame. Only the tall bodies (timeline, avatars, photo) top-align.
  const centered = !timeline && !photo && !hasAvatars;

  return (
    <div
      style={{
        backgroundColor: CREAM,
        display: 'flex',
        flexDirection: 'row',
        fontFamily: 'IBM Plex Mono',
        height: '100%',
        width: '100%',
      }}
    >
      {/* Purple rail */}
      <div
        style={{
          alignItems: 'center',
          backgroundColor: PURPLE,
          display: 'flex',
          flexDirection: 'column',
          height: '100%',
          justifyContent: 'space-between',
          padding: '56px 0',
          width: 176,
        }}
      >
        <div
          style={{
            alignItems: 'center',
            backgroundColor: CREAM,
            borderRadius: 999,
            display: 'flex',
            height: 108,
            justifyContent: 'center',
            width: 108,
          }}
        >
          <img alt="BAML" height={70} src={lamb} width={70} />
        </div>
        <div
          style={{
            color: CREAM,
            fontSize: 24,
            fontWeight: 600,
            letterSpacing: 8,
            paddingLeft: 8,
          }}
        >
          BAML
        </div>
      </div>

      {/* Editorial panel */}
      <div
        style={{
          display: 'flex',
          flex: 1,
          flexDirection: 'column',
          padding: '64px 76px',
        }}
      >
        {photo ? (
          <div
            style={{
              borderRadius: 18,
              display: 'flex',
              flex: 1,
              marginBottom: 24,
              overflow: 'hidden',
            }}
          >
            <img
              alt="The BAML team"
              height={430}
              src={photo}
              style={{ height: '100%', objectFit: 'cover', width: '100%' }}
              width={872}
            />
          </div>
        ) : (
          <div
            style={{
              display: 'flex',
              flex: 1,
              flexDirection: 'column',
              justifyContent: centered ? 'center' : 'flex-start',
            }}
          >
            {eyebrow ? (
              <div
                style={{
                  color: PURPLE,
                  fontSize: 23,
                  fontWeight: 600,
                  letterSpacing: 3,
                  marginBottom: 20,
                  textTransform: 'uppercase',
                }}
              >
                {eyebrow}
              </div>
            ) : null}
            <div
              style={{
                color: INK,
                display: 'flex',
                fontFamily: 'Instrument Serif',
                fontSize: titleFontSize,
                letterSpacing: -1,
                lineHeight: 1.04,
                maxWidth: 872,
              }}
            >
              {title}
            </div>
            {description ? (
              <div
                style={{
                  color: MUTED,
                  display: 'flex',
                  fontSize: 27,
                  lineHeight: 1.42,
                  marginTop: 26,
                  maxWidth: 820,
                }}
              >
                {description}
              </div>
            ) : null}
            {hasAvatars ? (
              <div
                style={{ display: 'flex', flexDirection: 'row', marginTop: 40 }}
              >
                {avatars.map((a, i) => (
                  <div
                    key={a.handle}
                    style={{
                      alignItems: 'center',
                      display: 'flex',
                      flexDirection: 'row',
                      marginRight: i < avatars.length - 1 ? 44 : 0,
                    }}
                  >
                    <img
                      alt={a.handle}
                      height={92}
                      src={a.src}
                      style={{ borderRadius: 999 }}
                      width={92}
                    />
                    <div
                      style={{
                        color: MUTED,
                        fontSize: 24,
                        marginLeft: 18,
                      }}
                    >
                      {a.handle}
                    </div>
                  </div>
                ))}
              </div>
            ) : null}
            {timeline ? (
              <div
                style={{
                  display: 'flex',
                  height: 118,
                  marginTop: 46,
                  position: 'relative',
                  width: 916,
                }}
              >
                {/* rail: fades in from the past, brightening to the present */}
                <div
                  style={{
                    backgroundImage:
                      'linear-gradient(90deg, rgba(138,133,128,0) 0%, rgba(138,133,128,0.5) 26%, rgba(124,58,237,0.55) 82%, #7C3AED 100%)',
                    borderRadius: 2,
                    height: 3,
                    left: 96,
                    position: 'absolute',
                    right: 96,
                    top: 35,
                  }}
                />
                {/* soft glow under the present node */}
                <div
                  style={{
                    backgroundImage:
                      'radial-gradient(circle, rgba(124,58,237,0.22) 0%, rgba(124,58,237,0) 70%)',
                    borderRadius: 999,
                    height: 120,
                    position: 'absolute',
                    right: 40,
                    top: -24,
                    width: 120,
                  }}
                />
                <div
                  style={{
                    display: 'flex',
                    flexDirection: 'row',
                    justifyContent: 'space-between',
                    position: 'relative',
                    width: '100%',
                  }}
                >
                  {ERAS.map((e, i) => {
                    const last = i === ERAS.length - 1;
                    return (
                      <div
                        key={e.era}
                        style={{
                          alignItems: 'center',
                          display: 'flex',
                          flexDirection: 'column',
                          width: 192,
                        }}
                      >
                        <div
                          style={{
                            alignItems: 'flex-end',
                            color: last ? PURPLE : SOFT,
                            display: 'flex',
                            fontSize: 15,
                            height: 22,
                            letterSpacing: 1.5,
                            opacity: e.opacity,
                            textTransform: 'uppercase',
                          }}
                        >
                          {e.era}
                        </div>
                        <div
                          style={{
                            alignItems: 'center',
                            display: 'flex',
                            height: 28,
                            justifyContent: 'center',
                            width: 28,
                          }}
                        >
                          {last ? (
                            <div
                              style={{
                                alignItems: 'center',
                                backgroundColor: 'rgba(124,58,237,0.18)',
                                borderRadius: 999,
                                display: 'flex',
                                height: 28,
                                justifyContent: 'center',
                                width: 28,
                              }}
                            >
                              <div
                                style={{
                                  backgroundColor: PURPLE,
                                  borderRadius: 999,
                                  height: 14,
                                  width: 14,
                                }}
                              />
                            </div>
                          ) : (
                            <div
                              style={{
                                backgroundColor: SOFT,
                                borderRadius: 999,
                                height: 11,
                                opacity: e.opacity,
                                width: 11,
                              }}
                            />
                          )}
                        </div>
                        <div
                          style={{
                            color: last ? PURPLE : INK,
                            fontSize: last ? 29 : 25,
                            fontWeight: 600,
                            marginTop: 13,
                            opacity: e.opacity,
                          }}
                        >
                          {e.lang}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            ) : null}
          </div>
        )}

        {/* Bottom: hairline + footer */}
        <div style={{ display: 'flex', flexDirection: 'column' }}>
          <div style={{ backgroundColor: RULE, height: 1, width: '100%' }} />
          <div
            style={{
              color: SOFT,
              fontSize: 22,
              letterSpacing: 1,
              marginTop: 22,
            }}
          >
            {footer}
          </div>
        </div>
      </div>
    </div>
  );
}
