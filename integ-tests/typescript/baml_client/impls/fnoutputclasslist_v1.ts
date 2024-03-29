// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */


import { GPT35 } from '../client';
import { FnOutputClassList } from '../function';
import { schema } from '../json_schema';
import { Deserializer } from '@boundaryml/baml-core/deserializer/deserializer';


const prompt_template = `\
Return a JSON array that follows this schema: 
{
  "prop1": string,
  "prop2": int
}[]

JSON:\
`;

const deserializer = new Deserializer<TestOutputClass[]>(schema, {
  $ref: '#/definitions/FnOutputClassList_output'
});

FnOutputClassList.registerImpl('v1', async (
  arg: string
): Promise<TestOutputClass[]> => {
  
    const result = await GPT35.run_prompt_template(
      prompt_template,
      [],
      {
      }
    );

    return deserializer.coerce(result.generated);
  }
);


