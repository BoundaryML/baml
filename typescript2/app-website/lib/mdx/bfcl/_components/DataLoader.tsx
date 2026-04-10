'use server';

import { promises as fs } from 'node:fs';
import path from 'node:path';
import type { Question, TestResult } from '../_loader/types';
import { parseJSONL } from '../_loader/utils';

export default async function DataLoader() {
  // Read data directly from filesystem instead of fetching from localhost
  const dataDir = path.join(process.cwd(), 'lib/mdx/bfcl/data');

  // Read merged.jsonl file
  const mergedData = await fs.readFile(
    path.join(dataDir, 'merged.jsonl'),
    'utf-8',
  );
  const parsedResults = parseJSONL<TestResult>(mergedData);

  const files = [
    // "executable_multiple_function",
    // "executable_parallel_function",
    // "executable_simple",
    // "executable_parallel_multiple_function",
    'multiple_function',
    'parallel_function',
    'simple',
    'parallel_multiple_function',
    // "java",
    // "javascript",
    'relevance',
  ];

  const questions = await Promise.all(
    files.map(async (file) => {
      const questionPath = path.join(
        dataDir,
        'questions',
        `gorilla_openfunctions_v1_test_${file}.jsonl`,
      );
      const questionData = await fs.readFile(questionPath, 'utf-8');
      const parsedQuestions =
        parseJSONL<Omit<Question, 'idx' | 'test_type'>>(questionData);
      return parsedQuestions.map((question, idx) => ({
        idx,
        test_type: file,
        ...question,
      }));
    }),
  );

  return { parsedResults, questions: questions.flat() };
}
