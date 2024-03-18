// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */

import b from '../';

import { FireBamlEvent, traceAsync } from '@boundaryml/baml-core/ffi_layer';


describe('test_case:increasing_crimson', () => {
  const test_fn = traceAsync('increasing_crimson', 'null', [['impl', 'string']], 'positional', async (impl) => {
    FireBamlEvent.tags({
      'test_dataset_name': 'FnClassOptionalOutput2',
      'test_case_name': 'test',
      'test_case_arg_name': `test_increasing_crimson[FnClassOptionalOutput2-${impl}]`,
      'test_cycle_id': process.env.BOUNDARY_PROCESS_ID || 'local-run',
    });
    const test_case = "prop1 is hello\nprop2 is world";
    const result = await b.FnClassOptionalOutput2.getImpl(impl).run(
      test_case
    );
  });

  describe('function:FnClassOptionalOutput2', () => {
    test('impl:v1', async () => {
      await test_fn('v1');
    }, 60000);
  });
});


