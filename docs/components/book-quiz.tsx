'use client';

import { useId, useState } from 'react';

export type BookQuestion = {
  answer: {
    answer?: string;
    doesCompile?: boolean;
    lineNumber?: number;
  };
  context?: string;
  id: string;
  prompt: {
    distractors?: string[];
    program?: string;
    prompt?: string;
  };
  type: 'MultipleChoice' | 'ShortAnswer' | 'Tracing';
};

function answerFor(question: BookQuestion) {
  if (question.type === 'Tracing') {
    return question.answer.doesCompile ? 'Compiles' : 'Does not compile';
  }
  return question.answer.answer ?? '';
}

function choicesFor(question: BookQuestion) {
  if (question.type === 'Tracing') return ['Compiles', 'Does not compile'];
  const answer = answerFor(question);
  return [answer, ...(question.prompt.distractors ?? [])].sort((a, b) =>
    `${question.id}:${a}`.localeCompare(`${question.id}:${b}`),
  );
}

function QuizQuestion({ question, index }: { question: BookQuestion; index: number }) {
  const group = useId();
  const [selected, setSelected] = useState('');
  const [checked, setChecked] = useState(false);
  const expected = answerFor(question);
  const correct = selected.trim() === expected.trim();
  const isShortAnswer = question.type === 'ShortAnswer';

  return (
    <section className="book-quiz-question">
      <h3>Question {index + 1}</h3>
      {question.prompt.prompt && <p>{question.prompt.prompt}</p>}
      {question.prompt.program && (
        <pre aria-label="BAML program for this question"><code>{question.prompt.program.trim()}</code></pre>
      )}
      {isShortAnswer ? (
        <label>
          <span>Your answer</span>
          <input value={selected} onChange={(event) => setSelected(event.target.value)} />
        </label>
      ) : (
        <div className="book-quiz-choices">
          {choicesFor(question).map((choice) => (
            <label key={choice}>
              <input
                type="radio"
                name={group}
                value={choice}
                checked={selected === choice}
                onChange={(event) => setSelected(event.target.value)}
              />
              <span>{choice}</span>
            </label>
          ))}
        </div>
      )}
      <button type="button" disabled={!selected} onClick={() => setChecked(true)}>
        Check answer
      </button>
      {checked && (
        <div className={`book-quiz-feedback book-quiz-feedback--${correct ? 'correct' : 'incorrect'}`} aria-live="polite">
          <strong>{correct ? 'Correct.' : `The answer is ${expected}.`}</strong>
          {question.type === 'Tracing' && !question.answer.doesCompile && question.answer.lineNumber && (
            <span> The compiler reports the error on line {question.answer.lineNumber}.</span>
          )}
          {question.context && <p>{question.context}</p>}
        </div>
      )}
    </section>
  );
}

export function BookQuiz({ questions }: { questions: BookQuestion[] }) {
  return (
    <section className="book-quiz not-prose" aria-label="Chapter quiz">
      <h2>Check your understanding</h2>
      {questions.map((question, index) => (
        <QuizQuestion key={question.id} question={question} index={index} />
      ))}
    </section>
  );
}
