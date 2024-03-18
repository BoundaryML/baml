// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */

import b from '../';

import { FireBamlEvent, traceAsync } from '@boundaryml/baml-core/ffi_layer';


describe('test_case:powerful_coffee', () => {
  const test_fn = traceAsync('powerful_coffee', 'null', [['impl', 'string']], 'positional', async (impl) => {
    FireBamlEvent.tags({
      'test_dataset_name': 'PromptTest',
      'test_case_name': 'test',
      'test_case_arg_name': `test_powerful_coffee[PromptTest-${impl}]`,
      'test_cycle_id': process.env.BOUNDARY_PROCESS_ID || 'local-run',
    });
    const test_case = "mexico";
    const result = await b.PromptTest.getImpl(impl).run(
      test_case
    );
  });

  describe('function:PromptTest', () => {
    test('impl:claude_chat', async () => {
      await test_fn('claude_chat');
    }, 60000);
    test('impl:claude_chat_with_chat_msgs', async () => {
      await test_fn('claude_chat_with_chat_msgs');
    }, 60000);
    test('impl:openai_chat', async () => {
      await test_fn('openai_chat');
    }, 60000);
    test('impl:openai_chat_with_chat_msgs', async () => {
      await test_fn('openai_chat_with_chat_msgs');
    }, 60000);
  });
});


