import Link from 'next/link';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';

export default function PrivacyPolicyPage() {
  return (
    <div className="max-w-7xl mx-auto border-x relative">
      <Navbar />
      <main className="flex flex-col items-center justify-center divide-y divide-border min-h-screen w-full">
        <section className="w-full px-4 py-20 md:py-32">
          <div className="mx-auto max-w-4xl">
            <h1 className="text-4xl font-bold mb-8">Privacy Policy</h1>
            <p className="text-sm text-muted-foreground mb-8">
              Last updated: January 23, 2024
            </p>

            <div className="prose prose-gray dark:prose-invert max-w-none">
              <h2>1. Introduction</h2>
              <p>
                Boundary ML, Inc. ("Boundary," "we," "us," or "our") is
                committed to protecting your privacy. This Privacy Policy
                explains how we collect, use, disclose, and safeguard your
                information when you use our BAML platform, website, and
                services.
              </p>

              <h2>2. Information We Collect</h2>
              <h3>2.1 Information You Provide</h3>
              <ul>
                <li>Account information (name, email, company)</li>
                <li>Payment information (processed securely via Stripe)</li>
                <li>Communications with our support team</li>
                <li>Feedback and survey responses</li>
              </ul>

              <h3>2.2 Information Collected Automatically</h3>
              <ul>
                <li>Usage data (features used, frequency of use)</li>
                <li>Device information (browser type, OS, IP address)</li>
                <li>Log data (access times, pages viewed, crashes)</li>
                <li>Cookies and similar tracking technologies</li>
              </ul>

              <h3>2.3 Information from Third Parties</h3>
              <ul>
                <li>GitHub profile information (if you sign in with GitHub)</li>
                <li>Analytics providers (Google Analytics, Mixpanel)</li>
              </ul>

              <h2>3. How We Use Your Information</h2>
              <p>We use the collected information to:</p>
              <ul>
                <li>Provide, maintain, and improve our services</li>
                <li>Process transactions and send related information</li>
                <li>Send technical notices and support messages</li>
                <li>Respond to your comments and questions</li>
                <li>Monitor and analyze usage trends</li>
                <li>Detect and prevent fraudulent activity</li>
                <li>Comply with legal obligations</li>
              </ul>

              <h2>4. How We Share Your Information</h2>
              <p>We may share your information with:</p>
              <ul>
                <li>
                  <strong>Service Providers:</strong> Third parties that help us
                  operate our business
                </li>
                <li>
                  <strong>Legal Requirements:</strong> When required by law or
                  to protect rights
                </li>
                <li>
                  <strong>Business Transfers:</strong> In connection with
                  mergers or acquisitions
                </li>
                <li>
                  <strong>Consent:</strong> With your explicit consent
                </li>
              </ul>

              <h2>5. Data Security</h2>
              <p>
                We implement appropriate technical and organizational measures
                to protect your information, including:
              </p>
              <ul>
                <li>Encryption in transit and at rest</li>
                <li>Regular security assessments</li>
                <li>Access controls and authentication</li>
                <li>Employee training on data protection</li>
              </ul>

              <h2>6. Your Rights and Choices</h2>
              <p>You have the right to:</p>
              <ul>
                <li>Access and receive a copy of your data</li>
                <li>Correct inaccurate information</li>
                <li>Request deletion of your data</li>
                <li>Object to or restrict processing</li>
                <li>Data portability</li>
                <li>Opt-out of marketing communications</li>
              </ul>

              <h2>7. International Data Transfers</h2>
              <p>
                Your information may be transferred to and processed in
                countries other than your country of residence. We ensure
                appropriate safeguards are in place for such transfers.
              </p>

              <h2>8. Children's Privacy</h2>
              <p>
                Our services are not directed to individuals under 13. We do not
                knowingly collect personal information from children under 13.
              </p>

              <h2>9. Changes to This Policy</h2>
              <p>
                We may update this Privacy Policy from time to time. We will
                notify you of any changes by posting the new Privacy Policy on
                this page and updating the "Last updated" date.
              </p>

              <h2>10. Contact Us</h2>
              <p>
                If you have questions about this Privacy Policy, please contact
                us at:
              </p>
              <ul>
                <li>Email: privacy@boundaryml.com</li>
                <li>Address: Boundary ML, Inc., San Francisco, CA</li>
              </ul>

              <h2>11. Additional Rights for Specific Regions</h2>
              <h3>California Residents</h3>
              <p>
                Under the California Consumer Privacy Act (CCPA), California
                residents have additional rights including the right to know,
                delete, and opt-out of the sale of personal information.
              </p>

              <h3>European Economic Area</h3>
              <p>
                Under the General Data Protection Regulation (GDPR), EEA
                residents have additional rights including the right to lodge a
                complaint with a supervisory authority.
              </p>
            </div>

            <div className="mt-12 pt-8 border-t">
              <p className="text-sm text-muted-foreground">
                For questions about this privacy policy, please contact us at{' '}
                <Link
                  className="underline"
                  href="mailto:privacy@boundaryml.com"
                >
                  privacy@boundaryml.com
                </Link>
              </p>
            </div>
          </div>
        </section>

        <FooterSection />
      </main>
    </div>
  );
}
