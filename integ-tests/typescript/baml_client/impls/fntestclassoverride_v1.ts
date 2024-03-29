// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */


import { GPT35 } from '../client';
import { FnTestClassOverride } from '../function';
import { schema } from '../json_schema';
import { Deserializer } from '@boundaryml/baml-core/deserializer/deserializer';


const prompt_template = `\
Return a json blob with made up fields using this schema:
{
  "prop-one": string,
  "prop-two": string
}

JSON:\
`;

const deserializer = new Deserializer<OverrideClass>(schema, {
  $ref: '#/definitions/FnTestClassOverride_output'
});
deserializer.overload("OverrideClass", {
  "prop-one": "prop1",
  "prop-two": "prop2",
});

FnTestClassOverride.registerImpl('v1', async (
  arg: string
): Promise<OverrideClass> => {
  
    const result = await GPT35.run_prompt_template(
      prompt_template,
      [],
      {
      }
    );

    return deserializer.coerce(result.generated);
  }
);


