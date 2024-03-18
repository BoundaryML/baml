// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */


import { GPT35 } from '../client';
import { FnOutputClassWithEnum } from '../function';
import { schema } from '../json_schema';
import { Deserializer } from '@boundaryml/baml-core/deserializer/deserializer';


const prompt_template = `\
Return a made up json blob that matches this schema:
{
  "prop1": string,
  "prop2": "EnumInClass as string"
}

Here are the values to use for enum:
EnumInClass
---
ONE
TWO
---

JSON:\
`;

const deserializer = new Deserializer<TestClassWithEnum>(schema, {
  $ref: '#/definitions/FnOutputClassWithEnum_output'
});

FnOutputClassWithEnum.registerImpl('v1', async (
  arg: string
): Promise<TestClassWithEnum> => {
  
    const result = await GPT35.run_prompt_template(
      prompt_template,
      [],
      {
      }
    );

    return deserializer.coerce(result.generated);
  }
);


