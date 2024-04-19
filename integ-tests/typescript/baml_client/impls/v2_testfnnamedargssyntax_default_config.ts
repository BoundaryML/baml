// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */


import { GPT35 } from '../client';
import { V2_TestFnNamedArgsSyntax } from '../function';
import { schema } from '../json_schema';
import { Deserializer } from '@boundaryml/baml-core/deserializer/deserializer';


// Impl: default_config
// Client: GPT35
// An implementation for V2_TestFnNamedArgsSyntax


const prompt_template = `prompt`;
const output_format = `string`;

const template_macros = [
]

const deserializer = new Deserializer<string>(schema, {
  $ref: '#/definitions/V2_TestFnNamedArgsSyntax_output'
});

V2_TestFnNamedArgsSyntax.registerImpl('default_config', async (
  args: {
    var: string, var_with_underscores: string
  }
): Promise<string> => {
    const result = await GPT35.run_jinja_template(
      prompt_template,
      args,
      output_format,
      template_macros,
    );

    return deserializer.coerce(result.generated);
  }
);


