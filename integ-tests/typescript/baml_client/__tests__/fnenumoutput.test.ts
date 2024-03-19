// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */

import b from '../';

import { FireBamlEvent, traceAsync } from '@boundaryml/baml-core/ffi_layer';


describe('test_case:dependent_tomato', () => {
  const test_fn = traceAsync('dependent_tomato', 'null', [['impl', 'string']], 'positional', async (impl) => {
    FireBamlEvent.tags({
      'test_dataset_name': 'FnEnumOutput',
      'test_case_name': 'test',
      'test_case_arg_name': `test_dependent_tomato[FnEnumOutput-${impl}]`,
      'test_cycle_id': process.env.BOUNDARY_PROCESS_ID || 'local-run',
    });
    const test_case = "noop";
    const result = await b.FnEnumOutput.getImpl(impl).run(
      test_case
    );
  });

  describe('function:FnEnumOutput', () => {
    test('impl:v1', async () => {
      await test_fn('v1');
    }, 60000);
  });
});

describe('test_case:open_bronze', () => {
  const test_fn = traceAsync('open_bronze', 'null', [['impl', 'string']], 'positional', async (impl) => {
    FireBamlEvent.tags({
      'test_dataset_name': 'FnEnumOutput',
      'test_case_name': 'test',
      'test_case_arg_name': `test_open_bronze[FnEnumOutput-${impl}]`,
      'test_cycle_id': process.env.BOUNDARY_PROCESS_ID || 'local-run',
    });
    const test_case = "pick the first one";
    const result = await b.FnEnumOutput.getImpl(impl).run(
      test_case
    );
  });

  describe('function:FnEnumOutput', () => {
    test('impl:v1', async () => {
      await test_fn('v1');
    }, 60000);
  });
});

describe('test_case:zestful_lavender', () => {
  const test_fn = traceAsync('zestful_lavender', 'null', [['impl', 'string']], 'positional', async (impl) => {
    FireBamlEvent.tags({
      'test_dataset_name': 'FnEnumOutput',
      'test_case_name': 'test',
      'test_case_arg_name': `test_zestful_lavender[FnEnumOutput-${impl}]`,
      'test_cycle_id': process.env.BOUNDARY_PROCESS_ID || 'local-run',
    });
    const test_case = "pick the last one";
    const result = await b.FnEnumOutput.getImpl(impl).run(
      test_case
    );
  });

  describe('function:FnEnumOutput', () => {
    test('impl:v1', async () => {
      await test_fn('v1');
    }, 60000);
  });
});


