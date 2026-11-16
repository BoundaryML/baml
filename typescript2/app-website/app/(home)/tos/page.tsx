import { createMetadata } from '@/app/_lib/metadata';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';

export const metadata = createMetadata({
  description: 'The terms governing your use of BAML and Boundary services.',
  eyebrow: 'Legal',
  path: '/tos',
  title: 'Terms of Service',
});

// Legal text, set as a reading column in the homepage's BAML lane: Lexend headings, body in ink-2.
// It isn't linked from the nav or the footer.

/** The terms of service, as a single reading column. */
export default function TermsOfServicePage() {
  return (
    <PageShell>
      <section
        {...section('tos_body')}
        className={`${PAGE_WRAP} pt-14 md:pt-20 pb-16 md:pb-24`}
      >
        <div className="max-w-[46rem]">
          <h1 className="v4-h1">Terms of Service</h1>
          <p className="mt-6 v4-caption text-muted">
            Effective date: January 23, 2024
          </p>

          <div className="mt-12 md:mt-14 prose blog-prose">
            <h2>1. Agreement to Terms</h2>
            <p>
              By accessing or using the BAML platform and services provided by
              Gloo Chat, Inc. ("Boundary," "we," "us," or "our"), you agree to
              be bound by these Terms of Service ("Terms"). If you disagree with
              any part of these terms, you may not access our services.
            </p>

            <h2>2. Description of Service</h2>
            <p>
              BAML is a domain-specific language and development platform for
              building type-safe AI applications. Our services include:
            </p>
            <ul>
              <li>BAML language and compiler</li>
              <li>Development tools and IDE extensions</li>
              <li>Cloud infrastructure for AI applications</li>
              <li>Documentation and support services</li>
            </ul>

            <h2>3. Account Registration</h2>
            <p>
              To use certain features, you must register for an account. You
              agree to:
            </p>
            <ul>
              <li>Provide accurate and complete information</li>
              <li>Maintain the security of your account credentials</li>
              <li>Promptly update any changes to your information</li>
              <li>
                Accept responsibility for all activities under your account
              </li>
            </ul>

            <h2>4. Acceptable Use</h2>
            <p>You agree not to use our services to:</p>
            <ul>
              <li>Violate any laws or regulations</li>
              <li>Infringe on intellectual property rights</li>
              <li>Transmit malicious code or interfere with our services</li>
              <li>Engage in unauthorized access or data mining</li>
              <li>Harass, abuse, or harm others</li>
              <li>
                Create competing products using our proprietary technology
              </li>
            </ul>

            <h2>5. Intellectual Property</h2>
            <h3>5.1 Our Property</h3>
            <p>
              The BAML platform, including all software, documentation, and
              content, is owned by Boundary and protected by intellectual
              property laws. Open source components are licensed under their
              respective licenses.
            </p>

            <h3>5.2 Your Content</h3>
            <p>
              You retain ownership of content you create using our services. By
              using our services, you grant us a license to host, store, and
              display your content as necessary to provide the services.
            </p>

            <h2>6. Payment Terms</h2>
            <p>For paid plans:</p>
            <ul>
              <li>Payment is due upon subscription and renewal</li>
              <li>All fees are non-refundable except as required by law</li>
              <li>We may change prices with 30 days notice</li>
              <li>You're responsible for all applicable taxes</li>
            </ul>

            <h2>7. Privacy and Data Protection</h2>
            <p>
              Your use of our services is subject to our Privacy Policy. We are
              committed to protecting your data and maintaining appropriate
              security measures.
            </p>

            <h2>8. Service Level Agreement</h2>
            <p>For paid plans, we strive to maintain:</p>
            <ul>
              <li>99.9% uptime for Enterprise plans</li>
              <li>Regular backups and disaster recovery</li>
              <li>Security updates and patches</li>
              <li>Technical support as per your plan</li>
            </ul>

            <h2>9. Disclaimers and Limitations</h2>
            <p>
              OUR SERVICES ARE PROVIDED "AS IS" WITHOUT WARRANTIES OF ANY KIND.
              WE DISCLAIM ALL WARRANTIES, EXPRESS OR IMPLIED, INCLUDING
              MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE.
            </p>
            <p>
              IN NO EVENT SHALL BOUNDARY BE LIABLE FOR ANY INDIRECT, INCIDENTAL,
              SPECIAL, CONSEQUENTIAL, OR PUNITIVE DAMAGES, OR ANY LOSS OF
              PROFITS OR REVENUES.
            </p>

            <h2>10. Indemnification</h2>
            <p>
              You agree to indemnify and hold Boundary harmless from any claims,
              damages, or expenses arising from your use of our services or
              violation of these Terms.
            </p>

            <h2>11. Termination</h2>
            <p>
              Either party may terminate this agreement at any time. Upon
              termination, your right to use our services will cease
              immediately. Provisions that should survive termination will
              remain in effect.
            </p>

            <h2>12. Modifications to Terms</h2>
            <p>
              We may modify these Terms at any time. We will notify you of
              material changes via email or through our services. Continued use
              after changes constitutes acceptance.
            </p>

            <h2>13. Governing Law</h2>
            <p>
              These Terms are governed by the laws of Delaware, United States,
              without regard to conflict of law principles. Any disputes shall
              be resolved in the courts of Delaware.
            </p>

            <h2>14. Export Compliance</h2>
            <p>
              You agree to comply with all applicable export and import laws and
              regulations, including U.S. export controls and sanctions.
            </p>

            <h2>15. Entire Agreement</h2>
            <p>
              These Terms, together with our Privacy Policy and any other
              agreements, constitute the entire agreement between you and
              Boundary regarding our services.
            </p>

            <h2>16. Contact Information</h2>
            <p>For questions about these Terms, please contact us at:</p>
            <ul>
              <li>Email: legal@boundaryml.com</li>
              <li>Address: Gloo Chat, Inc., Seattle, WA</li>
            </ul>
          </div>

          <p className="mt-14 pt-8 border-t border-rule text-[16px] leading-[1.5] text-body">
            For legal inquiries, please contact us at{' '}
            <a
              className="text-head underline decoration-accent underline-offset-[3px] hover:text-accent transition-colors"
              href="mailto:legal@boundaryml.com"
            >
              legal@boundaryml.com
            </a>
          </p>
        </div>
      </section>
    </PageShell>
  );
}
