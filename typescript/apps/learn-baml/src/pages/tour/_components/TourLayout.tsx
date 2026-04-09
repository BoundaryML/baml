import React, { useState, useRef, useEffect } from 'react';
import Head from '@docusaurus/Head';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import {
  tourChapters,
  getAllModules,
  getModuleIndex,
  getChapterForModule,
  getAdjacentModules,
} from '../_data/tourData';

interface TourLayoutProps {
  children: React.ReactNode;
  currentSlug: string;
  title: string;
  description: string;
}

export default function TourLayout({
  children,
  currentSlug,
  title,
  description
}: TourLayoutProps) {
  const { siteConfig } = useDocusaurusContext();
  const [isNavOpen, setIsNavOpen] = useState(false);
  const navRef = useRef<HTMLDivElement>(null);
  
  const modules = getAllModules();
  const currentIndex = getModuleIndex(currentSlug);
  const currentChapter = getChapterForModule(currentSlug);
  const { prev: prevStep, next: nextStep } = getAdjacentModules(currentSlug);
  const totalModules = modules.length;

  // Close nav when clicking outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (navRef.current && !navRef.current.contains(event.target as Node)) {
        setIsNavOpen(false);
      }
    }
    
    if (isNavOpen) {
      document.addEventListener('mousedown', handleClickOutside);
      return () => document.removeEventListener('mousedown', handleClickOutside);
    }
  }, [isNavOpen]);

  // Close nav on escape key
  useEffect(() => {
    function handleEscape(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        setIsNavOpen(false);
      }
    }
    
    if (isNavOpen) {
      document.addEventListener('keydown', handleEscape);
      return () => document.removeEventListener('keydown', handleEscape);
    }
  }, [isNavOpen]);

  const pageTitle = title ? `${title} | ${siteConfig.title}` : siteConfig.title;

  return (
    <>
      <Head>
        <title>{pageTitle}</title>
        <meta name="description" content={description} />
      </Head>
      <div className="tour-fullscreen">
        {/* Minimal Progress Bar */}
        <div className="tour-progress-bar" ref={navRef}>
          <div className="tour-progress-bar__content">
            {/* Progress dots */}
            <div className="tour-progress-dots">
              {modules.map((_, i) => (
                <div
                  key={i}
                  className={`tour-progress-dot ${
                    i < currentIndex ? 'tour-progress-dot--completed' :
                    i === currentIndex ? 'tour-progress-dot--current' : ''
                  }`}
                />
              ))}
            </div>

            {/* Current step info */}
            <button
              className="tour-progress-info"
              onClick={() => setIsNavOpen(!isNavOpen)}
              aria-expanded={isNavOpen}
              aria-haspopup="true"
            >
              <span className="tour-progress-info__step">
                Step {currentIndex + 1} of {totalModules}
              </span>
              <span className="tour-progress-info__title">
                {modules[currentIndex]?.shortTitle}
              </span>
              <span className={`tour-progress-info__chevron ${isNavOpen ? 'tour-progress-info__chevron--open' : ''}`}>
                ▾
              </span>
            </button>
          </div>

          {/* Expanded Navigation Dropdown */}
          {isNavOpen && (
            <div className="tour-nav-dropdown">
              {tourChapters.map((chapter) => {
                const isCurrentChapter = chapter.id === currentChapter?.id;
                return (
                  <div key={chapter.id} className="tour-nav-chapter">
                    <div className={`tour-nav-chapter__header ${isCurrentChapter ? 'tour-nav-chapter__header--active' : ''}`}>
                      {chapter.title}
                    </div>
                    <div className="tour-nav-chapter__modules">
                      {chapter.modules.map((module) => {
                        const moduleIdx = getModuleIndex(module.slug);
                        const isCurrent = module.slug === currentSlug;
                        const isCompleted = moduleIdx < currentIndex;
                        
                        return (
                          <Link
                            key={module.slug}
                            to={`/tour/${module.slug}`}
                            className={`tour-nav-module ${
                              isCurrent ? 'tour-nav-module--current' :
                              isCompleted ? 'tour-nav-module--completed' : ''
                            }`}
                            onClick={() => setIsNavOpen(false)}
                          >
                            <span className="tour-nav-module__indicator">
                              {isCompleted ? '✓' : isCurrent ? '→' : '○'}
                            </span>
                            <span className="tour-nav-module__title">{module.shortTitle}</span>
                            <span className="tour-nav-module__duration">{module.duration}</span>
                          </Link>
                        );
                      })}
                    </div>
                  </div>
                );
              })}
              
              <div className="tour-nav-dropdown__footer">
                <Link to="/tour" onClick={() => setIsNavOpen(false)}>
                  ← Back to Tour Overview
                </Link>
              </div>
            </div>
          )}
        </div>

        {/* Main content area */}
        <div className="tour-container">
          {children}

          {/* Navigation */}
          <div className="tour-nav">
          {prevStep ? (
            <Link to={`/tour/${prevStep.slug}`} className="button button--secondary">
              ← {prevStep.shortTitle}
            </Link>
          ) : (
            <Link to="/tour" className="button button--secondary">
              ← Tour Overview
            </Link>
          )}
          {nextStep ? (
            <Link to={`/tour/${nextStep.slug}`} className="button button--primary">
              {nextStep.shortTitle} →
            </Link>
          ) : (
            <Link to="/tutorials/getting-started" className="button button--primary">
              Continue to Tutorials →
            </Link>
          )}
          </div>
        </div>
      </div>
    </>
  );
}
