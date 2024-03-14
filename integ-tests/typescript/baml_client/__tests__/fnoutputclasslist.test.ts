// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck

import b from '../';

import { traceAsync, FireBamlEvent } from '@boundaryml/baml-core/ffi_layer';


describe('test_case:deep_scarlet', () => {
  const test_fn = traceAsync('deep_scarlet', 'null', [['impl', 'string']], 'positional', async (impl) => {
    FireBamlEvent.tags({
      'test_dataset_name': 'FnOutputClassList',
      'test_case_name': 'test',
      'test_case_arg_name': `deep_scarlet[${impl}]`,
      'test_cycle_id': process.env.BOUNDARY_PROCESS_ID || 'local-run',
    });
    const test_case = "noop";
    const result = await b.FnOutputClassList.getImpl(impl).run(
      test_case
    );
  });

  describe('function:FnOutputClassList', () => {
    test('impl:v1', async () => {
      await test_fn('v1');
    });
  });
});
