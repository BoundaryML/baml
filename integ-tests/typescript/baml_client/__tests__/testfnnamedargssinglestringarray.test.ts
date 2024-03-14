// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck

import b from '../';

import { FireBamlEvent, traceAsync } from '@boundaryml/baml-core/ffi_layer';


describe('test_case:ministerial_beige', () => {
  const test_fn = traceAsync('ministerial_beige', 'null', [['impl', 'string']], 'positional', async (impl) => {
    FireBamlEvent.tags({
      'test_dataset_name': 'TestFnNamedArgsSingleStringArray',
      'test_case_name': 'test',
      'test_case_arg_name': `ministerial_beige[${impl}]`,
      'test_cycle_id': process.env.BOUNDARY_PROCESS_ID || 'local-run',
    });
    const test_case = { "myStringArray": ["hello there!\n\nhow are you.", "im doing fine\'"] };
    const result = await b.TestFnNamedArgsSingleStringArray.getImpl(impl).run(
      test_case
    );
  });

  describe('function:TestFnNamedArgsSingleStringArray', () => {
    test('impl:v1', async () => {
      await test_fn('v1');
    });
  });
});
