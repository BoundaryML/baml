// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */

import b from '../';

import { FireBamlEvent, traceAsync } from '@boundaryml/baml-core/ffi_layer';


describe('test_case:skinny_lime', () => {
  const test_fn = traceAsync('skinny_lime', 'null', [['impl', 'string']], 'positional', async (impl) => {
    FireBamlEvent.tags({
      'test_dataset_name': 'FnUnionStringBoolWithArrayOutput',
      'test_case_name': 'test',
      'test_case_arg_name': `test_skinny_lime[FnUnionStringBoolWithArrayOutput-${impl}]`,
      'test_cycle_id': process.env.BOUNDARY_PROCESS_ID || 'local-run',
    });
    const test_case = "noop";
    const result = await b.FnUnionStringBoolWithArrayOutput.getImpl(impl).run(
      test_case
    );
  });

  describe('function:FnUnionStringBoolWithArrayOutput', () => {
    test('impl:v1', async () => {
      await test_fn('v1');
    }, 60000);
  });
});


