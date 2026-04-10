/* eslint-disable @next/next/no-img-element */
import { ImageResponse } from 'next/og';
import type { NextRequest } from 'next/server';
import { getPost } from '@/app/blog/_lib/get-posts';
import { size } from '@/components/shared-images/base-layout';
import { Title } from '@/components/shared-images/title';

export const runtime = 'nodejs';

export async function GET(request: NextRequest) {
  const { searchParams, origin } = request.nextUrl;
  const slug = searchParams.get('slug');

  // Use production URL or request origin for images
  const baseUrl = process.env.NEXT_PUBLIC_BASE_URL
    || (process.env.VERCEL_URL ? `https://${process.env.VERCEL_URL}` : origin);

  const bgImage = `${baseUrl}/baml-og-background.png`;
  const logoImage = `${baseUrl}/baml-logo-with-lamb.png`;

  if (!slug) {
    return new ImageResponse(
      <div
        style={{
          alignItems: 'center',
          backgroundColor: '#0E0E0E',
          display: 'flex',
          height: '100%',
          justifyContent: 'center',
          width: '100%',
        }}
      >
        <Title subtitle="BAML Blog" title="Boundary" />
      </div>,
      { ...size },
    );
  }

  const post = await getPost(slug);

  if (!post) {
    return new ImageResponse(
      <div
        style={{
          alignItems: 'center',
          backgroundColor: '#0E0E0E',
          display: 'flex',
          height: '100%',
          justifyContent: 'center',
          width: '100%',
        }}
      >
        <Title subtitle="Post not found" title="BAML Blog" />
      </div>,
      { ...size },
    );
  }

  // Priority: og.image > firstImage > null
  const featuredImagePath = post.og?.image || post.firstImage || null;
  const featuredImage = featuredImagePath
    ? `${baseUrl}${featuredImagePath.startsWith('/') ? '' : '/'}${featuredImagePath}`
    : null;

  // Layout with featured image as large background
  if (featuredImage) {
    return new ImageResponse(
      <div
        style={{
          backgroundColor: '#0E0E0E',
          display: 'flex',
          height: '100%',
          position: 'relative',
          width: '100%',
        }}
      >
        {/* White background for transparent PNGs */}
        <div
          style={{
            backgroundColor: 'white',
            height: '100%',
            position: 'absolute',
            width: '100%',
          }}
        />
        {/* Featured image as large background */}
        <img
          alt="Featured"
          src={featuredImage}
          style={{
            height: '100%',
            objectFit: 'cover',
            opacity: 0.6,
            position: 'absolute',
            width: '100%',
          }}
        />
        {/* Dark gradient overlay for text readability */}
        <div
          style={{
            background: 'linear-gradient(to top, rgba(0,0,0,0.95) 0%, rgba(0,0,0,0.7) 40%, rgba(0,0,0,0.3) 100%)',
            bottom: 0,
            display: 'flex',
            left: 0,
            position: 'absolute',
            right: 0,
            top: 0,
          }}
        />
        {/* BAML Logo in top right */}
        {logoImage && (
          <img
            alt="BAML Logo"
            height={80}
            src={logoImage}
            style={{
              position: 'absolute',
              right: 40,
              top: 40,
            }}
            width={246}
          />
        )}
        {/* Content at bottom */}
        <div
          style={{
            bottom: 0,
            display: 'flex',
            flexDirection: 'column',
            gap: 16,
            left: 0,
            padding: 48,
            position: 'absolute',
            right: 0,
          }}
        >
          <div
            style={{
              color: 'white',
              display: 'flex',
              fontSize: 56,
              fontWeight: 'bold',
              lineHeight: 1.1,
              textShadow: '0 2px 10px rgba(0,0,0,0.5)',
            }}
          >
            {post.title}
          </div>
          {post.author && (
            <div
              style={{
                color: '#cccccc',
                display: 'flex',
                fontSize: 28,
              }}
            >
              {post.author.name}
            </div>
          )}
        </div>
      </div>,
      { ...size },
    );
  }

  // Layout without featured image (original purple background)
  return new ImageResponse(
    <div
      style={{
        backgroundColor: '#0E0E0E',
        display: 'flex',
        height: '100%',
        position: 'relative',
        width: '100%',
      }}
    >
      {/* Background image */}
      {bgImage && (
        <img
          alt="Background"
          height={630}
          src={bgImage}
          style={{
            left: 0,
            objectFit: 'cover',
            position: 'absolute',
            top: 0,
          }}
          width={1200}
        />
      )}
      {/* Gradient overlay */}
      <div
        style={{
          background: 'linear-gradient(45deg, rgba(30,13,46,0.3), transparent)',
          bottom: 0,
          left: 0,
          position: 'absolute',
          right: 0,
          top: 0,
        }}
      />
      {/* BAML Logo in bottom right */}
      {logoImage && (
        <img
          alt="BAML Logo"
          height={125}
          src={logoImage}
          style={{
            bottom: 40,
            position: 'absolute',
            right: 40,
          }}
          width={385}
        />
      )}
      {/* Content wrapper */}
      <div
        style={{
          alignItems: 'flex-start',
          display: 'flex',
          flexDirection: 'column',
          height: '100%',
          justifyContent: 'center',
          maxWidth: '90%',
          padding: 40,
          position: 'relative',
          width: '100%',
        }}
      >
        <Title subtitle={post.description} title={post.title} />
        {post.author && (
          <div
            style={{
              alignItems: 'center',
              display: 'flex',
              gap: 16,
              marginTop: 32,
            }}
          >
            <div
              style={{
                color: '#cccccc',
                display: 'flex',
                fontSize: 28,
              }}
            >
              {post.author.name}
            </div>
          </div>
        )}
      </div>
    </div>,
    { ...size },
  );
}
