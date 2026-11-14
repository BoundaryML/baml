import { useState } from 'react';
import type { RegistrationQuestion } from '@/lib/luma';
import { questionOptions } from '@/lib/luma-answers';

import styles from './form-controls.module.css';

export default function SessionQuestion({
  question,
  inputClass,
}: {
  question: RegistrationQuestion;
  inputClass: string;
}) {
  const [text, setText] = useState('');
  const [checked, setChecked] = useState(false);
  const [choices, setChoices] = useState<string[]>([]);
  const name = `luma-${question.id}`;
  if (question.question_type === 'terms') return null;
  if (question.question_type === 'company' && !question.collect_job_title)
    return null;

  if (question.question_type === 'agree-check') {
    return (
      <label className="flex items-start gap-2 text-sm text-head">
        <input
          checked={checked}
          className={`mt-0.5 ${styles.choice}`}
          name={name}
          onChange={(event) => setChecked(event.target.checked)}
          required={question.required}
          type="checkbox"
        />
        {question.label}
      </label>
    );
  }

  if (question.question_type === 'select') {
    return (
      <label className="block text-sm font-medium text-head">
        {question.label}
        <select
          className={inputClass}
          multiple={Boolean(question.multiple)}
          name={name}
          onChange={(event) =>
            setChoices(
              Array.from(event.target.selectedOptions, ({ value }) => value),
            )
          }
          required={question.required}
          value={question.multiple ? choices : (choices[0] ?? '')}
        >
          {!question.multiple && <option value="">Select an answer</option>}
          {questionOptions(question).map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
      </label>
    );
  }

  return (
    <label className="block text-sm font-medium text-head">
      {question.question_type === 'company'
        ? question.job_title_label || 'Job title'
        : question.label}
      <input
        className={inputClass}
        maxLength={600}
        name={name}
        onChange={(event) => setText(event.target.value)}
        required={question.required}
        type={question.question_type === 'phone-number' ? 'tel' : 'text'}
        value={text}
      />
    </label>
  );
}
