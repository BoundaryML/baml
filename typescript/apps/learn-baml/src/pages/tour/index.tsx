import React from 'react';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import { tourChapters, getTotalDuration } from './_data/tourData';

export default function TourIndex() {
  return (
    <Layout title="Interactive Tour" description="Hands-on introduction to BAML">
      <div className="tour-index">
        {/* Hero Section */}
        <div className="tour-index__hero">
          <h1>Interactive BAML Tour</h1>
          <p className="tour-index__subtitle">
            Learn by doing. Go from zero to production-ready in {getTotalDuration()}.
          </p>
        </div>

        {/* Chapters */}
        <div className="tour-index__chapters">
          {tourChapters.map((chapter, chapterIndex) => (
            <div key={chapter.id} className="tour-chapter">
              <div className="tour-chapter__header">
                <div className="tour-chapter__number">{chapterIndex + 1}</div>
                <div className="tour-chapter__info">
                  <h2 className="tour-chapter__title">{chapter.title}</h2>
                  <p className="tour-chapter__description">{chapter.description}</p>
                </div>
              </div>

              <div className="tour-chapter__modules">
                {chapter.modules.map((module, moduleIndex) => (
                  <Link
                    key={module.slug}
                    to={`/tour/${module.slug}`}
                    className="tour-module-card"
                  >
                    <div className="tour-module-card__content">
                      <div className="tour-module-card__indicator" />
                      <div className="tour-module-card__text">
                        <span className="tour-module-card__title">{module.shortTitle}</span>
                        <span className="tour-module-card__description">{module.description}</span>
                      </div>
                      <span className="tour-module-card__duration">{module.duration}</span>
                    </div>
                  </Link>
                ))}
              </div>

              {/* CTA on first chapter */}
              {chapterIndex === 0 && (
                <div className="tour-chapter__cta">
                  <Link to="/tour/hello-baml" className="button button--primary">
                    Start Here →
                  </Link>
                </div>
              )}
            </div>
          ))}
        </div>

        {/* Bottom CTA */}
        <div className="tour-index__footer">
          <p>Ready to dive in?</p>
          <Link to="/tour/hello-baml" className="button button--primary button--lg">
            Begin the Tour
          </Link>
        </div>
      </div>
    </Layout>
  );
}
