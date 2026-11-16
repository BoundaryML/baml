import type { RegistrationAnswer, RegistrationQuestion } from './luma';

export function questionOptions(question: RegistrationQuestion) {
  return (question.options ?? []).map((option) =>
    typeof option === 'string' ? option : option.label,
  );
}

export function readRegistrationAnswers(
  questions: RegistrationQuestion[],
  form: FormData,
  company: string,
): { answers: RegistrationAnswer[]; notes: string[] } | { message: string } {
  const answers: RegistrationAnswer[] = [];
  const notes: string[] = [];

  for (const question of questions) {
    if (question.question_type === 'terms') continue;
    const field = `luma-${question.id}`;
    const values = form
      .getAll(field)
      .map(String)
      .map((value) => value.trim())
      .filter(Boolean);
    let value: RegistrationAnswer['value'] | undefined = values[0];
    let summary = values.join(', ');
    if (question.question_type === 'agree-check') {
      value = values[0] === 'on' ? true : undefined;
      summary = 'Yes';
    }
    if (question.question_type === 'select' && values.length) {
      if (
        values.some((option) => !questionOptions(question).includes(option))
      ) {
        return { message: `Choose a valid answer for “${question.label}”.` };
      }
      value = question.multiple ? values : values[0];
    }
    if (question.question_type === 'company') {
      if (question.required && question.collect_job_title && !values[0]) {
        return { message: 'Enter your job title for this session.' };
      }
      value = { company, job_title: values[0] || null };
      summary = [company, values[0]].filter(Boolean).join(' · ');
    }
    if (values.some((answer) => answer.length > 600)) {
      return { message: `Shorten your answer to “${question.label}”.` };
    }
    if (value === undefined) {
      if (question.required)
        return {
          message: `Answer “${question.label}” to register for this session.`,
        };
      continue;
    }
    answers.push({ question_id: question.id, value });
    notes.push(`${question.label}: ${summary}`);
  }
  return { answers, notes };
}
