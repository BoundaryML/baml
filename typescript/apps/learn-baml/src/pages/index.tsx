import React from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import {
  Compass,
  BookOpen,
  Wrench,
  Lightbulb,
  FileText,
  ChefHat,
} from 'lucide-react';
import styles from './index.module.css';

function HomepageHeader() {
  const { siteConfig } = useDocusaurusContext();
  return (
    <header className={clsx('hero', styles.heroBanner)}>
      <div className="container">
        <h1 className={styles.heroTitle}>{siteConfig.title}</h1>
        <p className={styles.heroSubtitle}>{siteConfig.tagline}</p>
        <div className={styles.buttons}>
          <Link
            className={clsx('button button--lg', styles.ctaButton)}
            to="/tour/hello-baml"
          >
            Start the Tour
          </Link>
        </div>
      </div>
    </header>
  );
}

interface PathCardProps {
  title: string;
  description: string;
  to: string;
  icon: React.ReactNode;
}

function PathCard({ title, description, to, icon }: PathCardProps) {
  return (
    <Link to={to} className={styles.pathCard}>
      <div className={styles.pathIcon}>{icon}</div>
      <h3>{title}</h3>
      <p>{description}</p>
    </Link>
  );
}

const iconProps = {
  size: 28,
  strokeWidth: 1.5,
};

export default function Home(): React.ReactElement {
  const { siteConfig } = useDocusaurusContext();
  return (
    <Layout title="Learn BAML" description={siteConfig.tagline}>
      <HomepageHeader />
      <main>
        <section className={styles.paths}>
          <div className="container">
            <h2>Choose Your Path</h2>
            <div className={styles.pathGrid}>
              <PathCard
                title="Interactive Tour"
                description="Hands-on introduction with live code examples"
                to="/tour/hello-baml"
                icon={<Compass {...iconProps} />}
              />
              <PathCard
                title="Tutorials"
                description="Step-by-step guides to build real projects"
                to="/tutorials/getting-started"
                icon={<BookOpen {...iconProps} />}
              />
              <PathCard
                title="How-to Guides"
                description="Solve specific problems and tasks"
                to="/how-to/add-fallbacks"
                icon={<Wrench {...iconProps} />}
              />
              <PathCard
                title="Concepts"
                description="Understand the core ideas behind BAML"
                to="/concepts/type-system"
                icon={<Lightbulb {...iconProps} />}
              />
              <PathCard
                title="Reference"
                description="Complete API and syntax documentation"
                to="/reference/baml-syntax"
                icon={<FileText {...iconProps} />}
              />
              <PathCard
                title="Cookbook"
                description="Ready-to-use recipes for common patterns"
                to="/cookbook/classification"
                icon={<ChefHat {...iconProps} />}
              />
            </div>
          </div>
        </section>
      </main>
    </Layout>
  );
}
