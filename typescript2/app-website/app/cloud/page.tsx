import { Navbar } from '@/components/navbar';
import { FooterSection } from '@/components/footer-section';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import Link from 'next/link';
import { Cloud, Shield, Zap, Globe, Lock, Settings } from 'lucide-react';

export default function CloudPage() {
  return (
    <div className="max-w-7xl mx-auto border-x relative">
      <Navbar />
      <main className="flex flex-col items-center justify-center divide-y divide-border min-h-screen w-full">
        {/* Hero Section */}
        <section className="relative flex w-full items-center justify-center px-4 py-20 md:py-32">
          <div className="flex flex-col items-center justify-center space-y-8 text-center">
            <div className="flex items-center gap-2 rounded-full bg-primary/10 px-4 py-2 text-sm font-medium">
              <Cloud className="h-4 w-4" />
              Cloud Solutions
            </div>
            <h1 className="text-4xl font-bold tracking-tight sm:text-5xl md:text-6xl lg:text-7xl max-w-4xl">
              AI Infrastructure That Scales With You
            </h1>
            <p className="max-w-2xl text-lg text-muted-foreground">
              Deploy and manage your AI applications in the cloud with
              enterprise-grade security, automatic scaling, and seamless
              integration with your existing infrastructure.
            </p>
            <div className="flex gap-4">
              <Button asChild>
                <Link href="/docs/cloud/getting-started">Start Free Trial</Link>
              </Button>
              <Button variant="outline" asChild>
                <Link href="/pricing">View Pricing</Link>
              </Button>
            </div>
          </div>
        </section>

        {/* Features Grid */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <div className="text-center mb-12">
              <h2 className="text-3xl font-bold mb-4">
                Enterprise-Ready Cloud Features
              </h2>
              <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
                Everything you need to run AI applications at scale with
                confidence
              </p>
            </div>
            <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-6">
              <Card className="p-6">
                <Shield className="h-10 w-10 mb-4 text-primary" />
                <h3 className="text-xl font-semibold mb-2">
                  Security & Compliance
                </h3>
                <p className="text-muted-foreground">
                  SOC 2 Type II certified with end-to-end encryption, HIPAA
                  compliance, and GDPR ready infrastructure.
                </p>
              </Card>
              <Card className="p-6">
                <Zap className="h-10 w-10 mb-4 text-primary" />
                <h3 className="text-xl font-semibold mb-2">Auto-Scaling</h3>
                <p className="text-muted-foreground">
                  Automatically scale your AI workloads based on demand. Pay
                  only for what you use with no idle costs.
                </p>
              </Card>
              <Card className="p-6">
                <Globe className="h-10 w-10 mb-4 text-primary" />
                <h3 className="text-xl font-semibold mb-2">
                  Global Deployment
                </h3>
                <p className="text-muted-foreground">
                  Deploy to multiple regions worldwide with automatic failover
                  and edge optimization for low latency.
                </p>
              </Card>
              <Card className="p-6">
                <Lock className="h-10 w-10 mb-4 text-primary" />
                <h3 className="text-xl font-semibold mb-2">
                  Private Deployments
                </h3>
                <p className="text-muted-foreground">
                  Run BAML in your own VPC or on-premises with full control over
                  your data and infrastructure.
                </p>
              </Card>
              <Card className="p-6">
                <Settings className="h-10 w-10 mb-4 text-primary" />
                <h3 className="text-xl font-semibold mb-2">Managed Services</h3>
                <p className="text-muted-foreground">
                  Fully managed infrastructure with automatic updates,
                  monitoring, and 24/7 support from our team.
                </p>
              </Card>
              <Card className="p-6">
                <Cloud className="h-10 w-10 mb-4 text-primary" />
                <h3 className="text-xl font-semibold mb-2">
                  Multi-Cloud Support
                </h3>
                <p className="text-muted-foreground">
                  Deploy on AWS, Azure, GCP, or your preferred cloud provider
                  with consistent performance and features.
                </p>
              </Card>
            </div>
          </div>
        </section>

        {/* Architecture Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <div className="grid lg:grid-cols-2 gap-12 items-center">
              <div>
                <h2 className="text-3xl font-bold mb-6">
                  Built for Scale and Reliability
                </h2>
                <div className="space-y-4">
                  <div>
                    <h3 className="text-xl font-semibold mb-2">
                      High Availability
                    </h3>
                    <p className="text-muted-foreground">
                      99.99% uptime SLA with automatic failover and redundancy
                      across multiple availability zones.
                    </p>
                  </div>
                  <div>
                    <h3 className="text-xl font-semibold mb-2">
                      Performance Optimization
                    </h3>
                    <p className="text-muted-foreground">
                      Intelligent caching, GPU optimization, and edge computing
                      for blazing-fast AI inference.
                    </p>
                  </div>
                  <div>
                    <h3 className="text-xl font-semibold mb-2">
                      Enterprise Integration
                    </h3>
                    <p className="text-muted-foreground">
                      Native integrations with your existing tools, SSO/SAML
                      support, and comprehensive APIs.
                    </p>
                  </div>
                </div>
              </div>
              <div className="bg-muted/20 rounded-lg p-8 border border-border">
                <div className="space-y-4">
                  <div className="h-4 bg-primary/20 rounded w-3/4"></div>
                  <div className="h-4 bg-primary/20 rounded w-full"></div>
                  <div className="h-4 bg-primary/20 rounded w-5/6"></div>
                  <div className="h-32 bg-primary/10 rounded mt-6"></div>
                  <div className="grid grid-cols-3 gap-4 mt-6">
                    <div className="h-20 bg-primary/10 rounded"></div>
                    <div className="h-20 bg-primary/10 rounded"></div>
                    <div className="h-20 bg-primary/10 rounded"></div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* CTA Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-4xl text-center">
            <h2 className="text-3xl font-bold mb-4">
              Ready to Scale Your AI Applications?
            </h2>
            <p className="text-lg text-muted-foreground mb-8 max-w-2xl mx-auto">
              Get started with BAML Cloud today. Free tier available with no
              credit card required.
            </p>
            <div className="flex flex-col sm:flex-row gap-4 justify-center">
              <Button size="lg" asChild>
                <Link href="/docs/cloud/getting-started">Start Free Trial</Link>
              </Button>
              <Button size="lg" variant="outline" asChild>
                <Link href="https://cal.com/boundaryml/30min">
                  Schedule a Demo
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
