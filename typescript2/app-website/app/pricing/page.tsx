'use client';

import { Check, HelpCircle, Minus } from 'lucide-react';
import Link from 'next/link';
import { useState } from 'react';
import { siteConfig } from '@/app/_lib/config';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';

const faqs = [
  {
    answer:
      'Yes, you can upgrade or downgrade your plan at any time. Changes take effect immediately and are prorated.',
    question: 'Can I change plans later?',
  },
  {
    answer:
      'We accept all major credit cards, ACH transfers for annual plans, and can work with procurement teams for enterprise contracts.',
    question: 'What payment methods do you accept?',
  },
  {
    answer:
      'Yes, all paid plans come with a 14-day free trial. No credit card required to start.',
    question: 'Is there a free trial for paid plans?',
  },
  {
    answer:
      "We'll notify you when you're approaching limits. You can upgrade anytime, and we never shut down your service unexpectedly.",
    question: 'What happens when I exceed my limits?',
  },
  {
    answer:
      'Yes! We offer 50% off for qualified startups and free Team plans for educational institutions.',
    question: 'Do you offer discounts for startups or education?',
  },
  {
    answer:
      'Enterprise support includes 24/7 phone support, dedicated Slack channel, custom SLAs, and quarterly business reviews.',
    question: "What's included in enterprise support?",
  },
];

const allFeatures = [
  { enterprise: true, free: true, name: 'CLI & Editor Extensions', team: true },
  { enterprise: true, free: true, name: 'Unlimited BAML Schemas', team: true },
  { enterprise: true, free: true, name: 'TypeScript Generation', team: true },
  { enterprise: true, free: true, name: 'Basic Schema Validation', team: true },
  { enterprise: true, free: true, name: 'Local Development', team: true },
  { enterprise: true, free: true, name: 'Community Support', team: true },
  {
    enterprise: true,
    free: false,
    name: 'Advanced Type Generation',
    team: true,
  },
  { enterprise: true, free: false, name: 'Runtime Validation', team: true },
  { enterprise: true, free: false, name: 'Team Collaboration', team: true },
  { enterprise: true, free: false, name: 'Private Schemas', team: true },
  { enterprise: true, free: false, name: 'Custom Transformations', team: true },
  { enterprise: true, free: false, name: 'Priority Support', team: true },
  { enterprise: true, free: false, name: 'On-Premise Deployment', team: false },
  { enterprise: true, free: false, name: 'SSO/SAML Integration', team: false },
  { enterprise: true, free: false, name: 'Custom Rate Limits', team: false },
  { enterprise: true, free: false, name: 'Audit Logs', team: false },
  { enterprise: true, free: false, name: '99.9% Uptime SLA', team: false },
  {
    enterprise: true,
    free: false,
    name: 'Dedicated Account Manager',
    team: false,
  },
];

export default function PricingPage() {
  const [billingCycle, setBillingCycle] = useState<'monthly' | 'yearly'>(
    'monthly',
  );

  return (
    <div className="max-w-7xl mx-auto border-x relative">
      <Navbar />
      <main className="flex flex-col items-center justify-center divide-y divide-border min-h-screen w-full">
        {/* Hero Section */}
        <section className="relative flex w-full items-center justify-center px-4 py-20 md:py-32">
          <div className="flex flex-col items-center justify-center space-y-8 text-center">
            <h1 className="text-4xl font-bold tracking-tight sm:text-5xl md:text-6xl lg:text-7xl max-w-4xl">
              {siteConfig.pricing.title}
            </h1>
            <p className="max-w-2xl text-lg text-muted-foreground">
              {siteConfig.pricing.description}
            </p>

            {/* Billing Toggle */}
            <div className="flex items-center gap-4 p-1 bg-muted rounded-full">
              <button
                className={`px-4 py-2 rounded-full text-sm font-medium transition-all ${
                  billingCycle === 'monthly'
                    ? 'bg-background text-foreground shadow-sm'
                    : 'text-muted-foreground'
                }`}
                onClick={() => setBillingCycle('monthly')}
              >
                Monthly
              </button>
              <button
                className={`px-4 py-2 rounded-full text-sm font-medium transition-all ${
                  billingCycle === 'yearly'
                    ? 'bg-background text-foreground shadow-sm'
                    : 'text-muted-foreground'
                }`}
                onClick={() => setBillingCycle('yearly')}
              >
                Yearly
                <span className="ml-2 text-xs bg-primary text-primary-foreground px-2 py-0.5 rounded-full">
                  Save 20%
                </span>
              </button>
            </div>
          </div>
        </section>

        {/* Pricing Cards */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <div className="grid md:grid-cols-3 gap-8">
              {siteConfig.pricing.pricingItems.map((plan) => (
                <Card
                  className={`relative p-8 ${plan.isPopular ? 'border-primary shadow-lg' : ''}`}
                  key={plan.name}
                >
                  {plan.isPopular && (
                    <div className="absolute -top-4 left-1/2 -translate-x-1/2">
                      <span className="bg-primary text-primary-foreground text-sm font-medium px-4 py-1 rounded-full">
                        Most Popular
                      </span>
                    </div>
                  )}

                  <div className="mb-8">
                    <h3 className="text-2xl font-bold mb-2">{plan.name}</h3>
                    <p className="text-muted-foreground text-sm mb-6">
                      {plan.description}
                    </p>
                    <div className="flex items-baseline gap-1">
                      <span className="text-4xl font-bold">
                        {billingCycle === 'yearly' && plan.yearlyPrice
                          ? plan.yearlyPrice
                          : plan.price}
                      </span>
                      {plan.price !== 'Custom' && (
                        <span className="text-muted-foreground">
                          /{billingCycle === 'yearly' ? 'mo' : plan.period}
                        </span>
                      )}
                    </div>
                    {billingCycle === 'yearly' &&
                      plan.price !== 'Custom' &&
                      plan.price !== '$0' && (
                        <p className="text-sm text-muted-foreground mt-1">
                          billed annually
                        </p>
                      )}
                  </div>

                  <Button
                    asChild
                    className={`w-full mb-8 ${
                      plan.isPopular ? 'bg-primary' : ''
                    }`}
                    variant={plan.isPopular ? 'default' : 'outline'}
                  >
                    <Link href={plan.href}>{plan.buttonText}</Link>
                  </Button>

                  <div className="space-y-3">
                    <p className="text-sm font-medium text-muted-foreground mb-4">
                      Everything in{' '}
                      {plan.name === 'Free' ? 'this plan' : 'previous plans'},
                      plus:
                    </p>
                    {plan.features.map((feature, idx) => (
                      <div className="flex items-start gap-3" key={idx}>
                        <Check className="h-5 w-5 text-primary flex-shrink-0 mt-0.5" />
                        <span className="text-sm">{feature}</span>
                      </div>
                    ))}
                  </div>
                </Card>
              ))}
            </div>
          </div>
        </section>

        {/* Feature Comparison */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <div className="text-center mb-12">
              <h2 className="text-3xl font-bold mb-4">Compare Plans</h2>
              <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
                All plans include core BAML features. Choose based on your team
                size and needs.
              </p>
            </div>

            <div className="overflow-x-auto">
              <table className="w-full">
                <thead>
                  <tr className="border-b">
                    <th className="text-left py-4 px-4 font-medium">Feature</th>
                    <th className="text-center py-4 px-4 font-medium">Free</th>
                    <th className="text-center py-4 px-4 font-medium">Team</th>
                    <th className="text-center py-4 px-4 font-medium">
                      Enterprise
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {allFeatures.map((feature, idx) => (
                    <tr className="border-b" key={idx}>
                      <td className="py-4 px-4 text-sm">{feature.name}</td>
                      <td className="text-center py-4 px-4">
                        {feature.free ? (
                          <Check className="h-5 w-5 text-primary mx-auto" />
                        ) : (
                          <Minus className="h-5 w-5 text-muted-foreground mx-auto" />
                        )}
                      </td>
                      <td className="text-center py-4 px-4">
                        {feature.team ? (
                          <Check className="h-5 w-5 text-primary mx-auto" />
                        ) : (
                          <Minus className="h-5 w-5 text-muted-foreground mx-auto" />
                        )}
                      </td>
                      <td className="text-center py-4 px-4">
                        {feature.enterprise ? (
                          <Check className="h-5 w-5 text-primary mx-auto" />
                        ) : (
                          <Minus className="h-5 w-5 text-muted-foreground mx-auto" />
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </section>

        {/* FAQ Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-4xl">
            <div className="text-center mb-12">
              <h2 className="text-3xl font-bold mb-4">
                Frequently Asked Questions
              </h2>
              <p className="text-lg text-muted-foreground">
                Got questions? We've got answers.
              </p>
            </div>

            <div className="space-y-6">
              {faqs.map((faq, idx) => (
                <Card className="p-6" key={idx}>
                  <h3 className="text-lg font-semibold mb-2 flex items-center gap-2">
                    <HelpCircle className="h-5 w-5 text-primary" />
                    {faq.question}
                  </h3>
                  <p className="text-muted-foreground">{faq.answer}</p>
                </Card>
              ))}
            </div>
          </div>
        </section>

        {/* CTA Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-4xl text-center">
            <h2 className="text-3xl font-bold mb-4">Ready to Get Started?</h2>
            <p className="text-lg text-muted-foreground mb-8 max-w-2xl mx-auto">
              Join thousands of developers building reliable AI applications
              with BAML. Start free and upgrade as you grow.
            </p>
            <div className="flex flex-col sm:flex-row gap-4 justify-center">
              <Button asChild size="lg">
                <Link href="/docs/getting-started">Start Free</Link>
              </Button>
              <Button asChild size="lg" variant="outline">
                <Link href="https://cal.com/boundaryml/30min">
                  Talk to Sales
                </Link>
              </Button>
            </div>
          </div>
        </section>

        <FooterSection />
      </main>
    </div>
  );
}
