/* eslint-disable @next/next/no-img-element */

// The shared, on-brand link-preview card. One template for every page:
// a purple rail (cream lamb badge + BAML wordmark) beside an editorial
// cream panel with a mono kicker, an Instrument Serif headline, a muted
// description, and a hairline footer. Rendered to PNG via `next/og`.

export const OG_SIZE = { height: 630, width: 1200 };
export const OG_CONTENT_TYPE = 'image/png';

const CREAM = '#FBF7ED';
const INK = '#1A1612';
const MUTED = '#5C5852';
const SOFT = '#8A8580';
const PURPLE = '#7C3AED';
const RULE = '#E2DAC8';

export interface OgCardProps {
  /** Small uppercase kicker, e.g. "Quickstart" or "Blog · Tutorials". */
  eyebrow: string;
  /** Headline, set in Instrument Serif. */
  title: string;
  /** Optional supporting line in mono. */
  description?: string;
  /** Bottom-left line, e.g. "boundaryml.com" or "Author · boundaryml.com". */
  footer: string;
  /** Ink lamb as a data URI, for the cream rail badge. */
  lamb: string;
  /** Pre-computed headline size so long titles stay on the card. */
  titleFontSize: number;
}

export function OgCard({
  eyebrow,
  title,
  description,
  footer,
  lamb,
  titleFontSize,
}: OgCardProps) {
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
          justifyContent: 'space-between',
          padding: '68px 76px',
        }}
      >
        {/* Top: kicker + headline + description */}
        <div style={{ display: 'flex', flexDirection: 'column' }}>
          <div
            style={{
              color: PURPLE,
              fontSize: 23,
              fontWeight: 600,
              letterSpacing: 3,
              textTransform: 'uppercase',
            }}
          >
            {eyebrow}
          </div>
          <div
            style={{
              backgroundColor: PURPLE,
              height: 3,
              marginTop: 22,
              width: 72,
            }}
          />
          <div
            style={{
              color: INK,
              display: 'flex',
              fontFamily: 'Instrument Serif',
              fontSize: titleFontSize,
              letterSpacing: -1,
              lineHeight: 1.04,
              marginTop: 30,
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
                marginTop: 30,
                maxWidth: 820,
              }}
            >
              {description}
            </div>
          ) : null}
        </div>

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
