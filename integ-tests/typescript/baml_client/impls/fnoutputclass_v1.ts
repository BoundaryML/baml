// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */


import { GPT35 } from '../client';
import { FnOutputClass } from '../function';
import { schema } from '../json_schema';
import { LLMResponseStream } from '@boundaryml/baml-core/client_manager';
import { Deserializer } from '@boundaryml/baml-core/deserializer/deserializer';


const prompt_template = `\
Return a JSON blob with this schema: 
{
  "prop1": string,
  "prop2": int
}

JSON:\
`;

const deserializer = new Deserializer<TestOutputClass>(schema, {
  $ref: '#/definitions/FnOutputClass_output'
});

const v1 = async (
  arg: string
): Promise<TestOutputClass> => {
  
  const result = await GPT35.run_prompt_template(
    prompt_template,
    [],
    {
    }
  );

  return deserializer.coerce(result.generated);
};

const v1_stream = async (
  arg: string
): LLMResponseStream<TestOutputClass> => {
  
  const stream = GPT35.run_prompt_template_stream(
    prompt_template,
    [],
    {
    }
  );

  return new LLMResponseStream<TestOutputClass>(
    stream,
    (partial) => null,
    deserializer.coerce,
  );
};

FnOutputClass.registerImpl('v1', v1, v1_stream);


