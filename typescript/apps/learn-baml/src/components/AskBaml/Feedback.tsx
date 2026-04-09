import React, { useState } from 'react';
import { trackFeedback } from '../../lib/analytics';
import styles from './styles.module.css';

interface FeedbackProps {
  query: string;
  answer: string;
}

export function Feedback({ query, answer }: FeedbackProps) {
  const [submitted, setSubmitted] = useState<'positive' | 'negative' | null>(null);
  const [showTextInput, setShowTextInput] = useState(false);
  const [feedbackText, setFeedbackText] = useState('');

  const handleFeedback = (rating: 'positive' | 'negative') => {
    trackFeedback({ query, answer, rating });
    setSubmitted(rating);

    // Show text input for negative feedback
    if (rating === 'negative') {
      setShowTextInput(true);
    }
  };

  const submitDetailedFeedback = () => {
    if (feedbackText.trim()) {
      trackFeedback({
        query,
        answer,
        rating: 'negative',
        feedbackText: feedbackText.trim(),
      });
    }
    setShowTextInput(false);
  };

  if (submitted && !showTextInput) {
    return (
      <div className={styles.feedbackThanks}>
        Thanks for your feedback!
      </div>
    );
  }

  return (
    <div className={styles.feedback}>
      {!submitted && (
        <>
          <span className={styles.feedbackLabel}>Was this helpful?</span>
          <button
            onClick={() => handleFeedback('positive')}
            className={styles.feedbackBtn}
            aria-label="Yes, helpful"
          >
            +1
          </button>
          <button
            onClick={() => handleFeedback('negative')}
            className={styles.feedbackBtn}
            aria-label="No, not helpful"
          >
            -1
          </button>
        </>
      )}

      {showTextInput && (
        <div className={styles.feedbackDetail}>
          <textarea
            value={feedbackText}
            onChange={e => setFeedbackText(e.target.value)}
            placeholder="What could be improved?"
            className={styles.feedbackTextarea}
            rows={2}
          />
          <button onClick={submitDetailedFeedback} className={styles.feedbackSubmit}>
            Submit
          </button>
        </div>
      )}
    </div>
  );
}
