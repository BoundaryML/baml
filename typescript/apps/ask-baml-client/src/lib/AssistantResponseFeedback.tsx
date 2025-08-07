import type { SendFeedbackRequest } from '@baml/sage-interface';
import { useAtomValue } from 'jotai';
import { ThumbsDown, ThumbsUp } from 'lucide-react';
import type React from 'react';
import { useState } from 'react';
import { messagesAtom, sessionIdAtom } from '../store';
import { FEEDBACK_ENDPOINT } from '../constants';

interface AssistantResponseFeedbackProps {
  messageId: string;
}

export const AssistantResponseFeedback: React.FC<AssistantResponseFeedbackProps> = ({
  messageId,
}) => {
  const sessionId = useAtomValue(sessionIdAtom);
  const messages = useAtomValue(messagesAtom);
  const [feedbackModal, setFeedbackModal] = useState<{
    isOpen: boolean;
    feedbackType: 'thumbs_up' | 'thumbs_down';
  } | null>(null);
  const [feedbackComment, setFeedbackComment] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const handleFeedbackClick = (feedbackType: 'thumbs_up' | 'thumbs_down') => {
    setFeedbackModal({
      isOpen: true,
      feedbackType,
    });
    setFeedbackComment('');
  };

  const closeFeedbackModal = () => {
    setFeedbackModal(null);
    setFeedbackComment('');
    setErrorMessage(null);
  };

  const submitFeedback = async () => {
    if (!feedbackModal) return;

    setIsSubmitting(true);
    setErrorMessage(null);

    try {
      // Find the assistant message with the specified messageId
      const assistantMessageIndex = messages.findIndex(
        (m) => m.role === 'assistant' && m.message_id === messageId,
      );

      if (assistantMessageIndex === -1) {
        throw new Error('Could not find the assistant message');
      }

      // Get the assistant message and its preceding user message
      const messagesToSend = [];
      if (assistantMessageIndex > 0) {
        messagesToSend.push(messages[assistantMessageIndex - 1]);
      }
      messagesToSend.push(messages[assistantMessageIndex]);

      // Filter and transform messages to match the expected type
      const filteredMessages = messagesToSend
        .filter(
          (msg): msg is Extract<typeof msg, { role: 'user' | 'assistant' }> =>
            msg !== undefined && (msg.role === 'user' || msg.role === 'assistant'),
        )
        .map((msg) => {
          if (msg.role === 'user') {
            return {
              role: 'user' as const,
              text: msg.text,
              language_preference: msg.language_preference,
            };
          } else {
            // msg.role === 'assistant'
            return {
              role: 'assistant' as const,
              message_id: msg.message_id,
              ranked_docs: msg.ranked_docs,
              text: msg.text,
              suggested_messages: msg.suggested_messages,
            };
          }
        });

      const feedbackRequest: SendFeedbackRequest = {
        session_id: sessionId,
        feedback_type: feedbackModal.feedbackType,
        comment: feedbackComment || undefined,
        messages: filteredMessages,
      };

      const response = await fetch(FEEDBACK_ENDPOINT, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(feedbackRequest),
      });

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }

      // Close modal on success
      closeFeedbackModal();

      // Optional: Show a brief success indicator
      console.log('Feedback submitted successfully');
    } catch (error) {
      console.error('Error submitting feedback:', error);
      setErrorMessage(
        error instanceof Error ? error.message : 'Failed to submit feedback. Please try again.',
      );
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <>
      {/* Feedback buttons */}
      <div
        style={{
          display: 'flex',
          gap: '8px',
          marginTop: '8px',
          alignItems: 'center',
        }}
      >
        <button
          onClick={() => handleFeedbackClick('thumbs_up')}
          style={{
            background: 'none',
            border: '1px solid #e5e7eb',
            borderRadius: '6px',
            padding: '4px 8px',
            cursor: 'pointer',
            color: '#9ca3af',
            display: 'flex',
            alignItems: 'center',
            gap: '4px',
            transition: 'all 0.2s ease',
          }}
          onMouseOver={(e) => {
            e.currentTarget.style.backgroundColor = '#f0fdf4';
            e.currentTarget.style.borderColor = '#10b981';
            e.currentTarget.style.color = '#10b981';
          }}
          onMouseOut={(e) => {
            e.currentTarget.style.backgroundColor = 'transparent';
            e.currentTarget.style.borderColor = '#e5e7eb';
            e.currentTarget.style.color = '#9ca3af';
          }}
        >
          <ThumbsUp size={14} />
        </button>
        <button
          onClick={() => handleFeedbackClick('thumbs_down')}
          style={{
            background: 'none',
            border: '1px solid #e5e7eb',
            borderRadius: '6px',
            padding: '4px 8px',
            cursor: 'pointer',
            color: '#9ca3af',
            display: 'flex',
            alignItems: 'center',
            gap: '4px',
            transition: 'all 0.2s ease',
          }}
          onMouseOver={(e) => {
            e.currentTarget.style.backgroundColor = '#fef2f2';
            e.currentTarget.style.borderColor = '#ef4444';
            e.currentTarget.style.color = '#ef4444';
          }}
          onMouseOut={(e) => {
            e.currentTarget.style.backgroundColor = 'transparent';
            e.currentTarget.style.borderColor = '#e5e7eb';
            e.currentTarget.style.color = '#9ca3af';
          }}
        >
          <ThumbsDown size={14} />
        </button>
      </div>

      {/* Feedback Modal */}
      {feedbackModal && (
        <div
          style={{
            position: 'fixed',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            backgroundColor: 'rgba(0, 0, 0, 0.5)',
            zIndex: 3000,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            padding: '20px',
          }}
          onClick={(e) => {
            if (e.target === e.currentTarget) {
              closeFeedbackModal();
            }
          }}
        >
          <div
            style={{
              backgroundColor: '#ffffff',
              borderRadius: '12px',
              padding: '24px',
              maxWidth: '400px',
              width: '100%',
              boxShadow:
                '0 20px 25px -5px rgba(0, 0, 0, 0.1), 0 10px 10px -5px rgba(0, 0, 0, 0.04)',
              fontFamily: 'Inter, system-ui, -apple-system, BlinkMacSystemFont, sans-serif',
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <div style={{ marginBottom: '16px' }}>
              <h3
                style={{
                  margin: '0 0 8px 0',
                  fontSize: '18px',
                  fontWeight: '600',
                  color: '#111827',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '8px',
                }}
              >
                {feedbackModal.feedbackType === 'thumbs_up' ? (
                  <>
                    <ThumbsUp size={20} color="#10b981" />
                    Helpful Response
                  </>
                ) : (
                  <>
                    <ThumbsDown size={20} color="#ef4444" />
                    Not Helpful
                  </>
                )}
              </h3>
              <p
                style={{
                  margin: '0',
                  fontSize: '14px',
                  color: '#6b7280',
                  lineHeight: '1.5',
                }}
              >
                {feedbackModal.feedbackType === 'thumbs_up'
                  ? 'Thanks for the feedback! What made this response helpful?'
                  : "Sorry the response wasn't helpful. What could be improved?"}
              </p>
            </div>

            <div style={{ marginBottom: '20px' }}>
              <textarea
                value={feedbackComment}
                onChange={(e) => setFeedbackComment(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && !isSubmitting) {
                    e.preventDefault();
                    submitFeedback();
                  }
                }}
                placeholder="Your feedback... (optional)"
                rows={3}
                style={{
                  width: '100%',
                  padding: '12px',
                  border: '1px solid #e5e7eb',
                  borderRadius: '8px',
                  fontSize: '14px',
                  fontFamily: 'inherit',
                  resize: 'vertical',
                  minHeight: '80px',
                  outline: 'none',
                  transition: 'border-color 0.2s ease',
                }}
                onFocus={(e) => {
                  e.currentTarget.style.borderColor = '#7d47e3';
                }}
                onBlur={(e) => {
                  e.currentTarget.style.borderColor = '#e5e7eb';
                }}
              />
            </div>

            <div
              style={{
                display: 'flex',
                gap: '12px',
                justifyContent: 'flex-end',
              }}
            >
              <button
                onClick={closeFeedbackModal}
                disabled={isSubmitting}
                style={{
                  padding: '10px 16px',
                  border: '1px solid #e5e7eb',
                  borderRadius: '8px',
                  backgroundColor: '#ffffff',
                  color: '#6b7280',
                  fontSize: '14px',
                  fontWeight: '500',
                  cursor: isSubmitting ? 'not-allowed' : 'pointer',
                  opacity: isSubmitting ? 0.5 : 1,
                  transition: 'all 0.2s ease',
                }}
                onMouseOver={(e) => {
                  if (!isSubmitting) {
                    e.currentTarget.style.backgroundColor = '#f9fafb';
                    e.currentTarget.style.borderColor = '#d1d5db';
                  }
                }}
                onMouseOut={(e) => {
                  e.currentTarget.style.backgroundColor = '#ffffff';
                  e.currentTarget.style.borderColor = '#e5e7eb';
                }}
              >
                Cancel
              </button>
              <button
                onClick={submitFeedback}
                disabled={isSubmitting}
                style={{
                  padding: '10px 16px',
                  border: 'none',
                  borderRadius: '8px',
                  backgroundColor: '#7d47e3',
                  color: '#ffffff',
                  fontSize: '14px',
                  fontWeight: '600',
                  cursor: isSubmitting ? 'not-allowed' : 'pointer',
                  opacity: isSubmitting ? 0.5 : 1,
                  transition: 'all 0.2s ease',
                }}
                onMouseOver={(e) => {
                  if (!isSubmitting) {
                    e.currentTarget.style.backgroundColor = '#6b3bc9';
                  }
                }}
                onMouseOut={(e) => {
                  e.currentTarget.style.backgroundColor = '#7d47e3';
                }}
              >
                {isSubmitting ? 'Submitting...' : 'Submit Feedback'}
              </button>
            </div>
            {errorMessage && (
              <div
                style={{
                  marginTop: '16px',
                  padding: '12px',
                  backgroundColor: '#fef2f2',
                  border: '1px solid #fecaca',
                  borderRadius: '8px',
                  color: '#991b1b',
                  fontSize: '14px',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '8px',
                }}
              >
                <span style={{ fontWeight: '500' }}>Error:</span>
                {errorMessage}
              </div>
            )}
          </div>
        </div>
      )}

      {/* Error Modal */}
      {errorMessage && !feedbackModal && (
        <div
          style={{
            position: 'fixed',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            backgroundColor: 'rgba(0, 0, 0, 0.5)',
            zIndex: 3000,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            padding: '20px',
          }}
          onClick={() => setErrorMessage(null)}
        >
          <div
            style={{
              backgroundColor: '#ffffff',
              borderRadius: '12px',
              padding: '24px',
              maxWidth: '400px',
              width: '100%',
              boxShadow:
                '0 20px 25px -5px rgba(0, 0, 0, 0.1), 0 10px 10px -5px rgba(0, 0, 0, 0.04)',
              fontFamily: 'Inter, system-ui, -apple-system, BlinkMacSystemFont, sans-serif',
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <h3
              style={{
                margin: '0 0 16px 0',
                fontSize: '18px',
                fontWeight: '600',
                color: '#991b1b',
              }}
            >
              Feedback Failed
            </h3>
            <p
              style={{
                margin: '0 0 20px 0',
                fontSize: '14px',
                color: '#6b7280',
                lineHeight: '1.5',
              }}
            >
              {errorMessage}
            </p>
            <button
              onClick={() => setErrorMessage(null)}
              style={{
                padding: '10px 16px',
                border: 'none',
                borderRadius: '8px',
                backgroundColor: '#ef4444',
                color: '#ffffff',
                fontSize: '14px',
                fontWeight: '600',
                cursor: 'pointer',
                width: '100%',
                transition: 'all 0.2s ease',
              }}
              onMouseOver={(e) => {
                e.currentTarget.style.backgroundColor = '#dc2626';
              }}
              onMouseOut={(e) => {
                e.currentTarget.style.backgroundColor = '#ef4444';
              }}
            >
              Close
            </button>
          </div>
        </div>
      )}
    </>
  );
};
