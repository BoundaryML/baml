// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */


import { GPT35 } from '../client';
import { TestFnNamedArgsSingleInt } from '../function';
import { schema } from '../json_schema';
import { LLMResponseStream } from '@boundaryml/baml-core/client_manager';
import { Deserializer } from '@boundaryml/baml-core/deserializer/deserializer';


const prompt_template = `\
Return this value back to me: {//BAML_CLIENT_REPLACE_ME_MAGIC_input.myInt//}\
`;

const deserializer = new Deserializer<string>(schema, {
  $ref: '#/definitions/TestFnNamedArgsSingleInt_output'
});

const v1 = async (
  args: {
    myInt: number
  }
): Promise<string> => {
  const myInt = args.myInt;
  
  const result = await GPT35.run_prompt_template(
    prompt_template,
    [
      "{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myInt//}",
    ],
    {
      "{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myInt//}": myInt,
    }
  );

  return deserializer.coerce(result.generated);
};

const v1_stream = async (
  args: {
    myInt: number
  }
): LLMResponseStream<string> => {
  const myInt = args.myInt;
  
  const stream = GPT35.run_prompt_template_stream(
    prompt_template,
    [
      "{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myInt//}",
    ],
    {
      "{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myInt//}": myInt,
    }
  );

  return new LLMResponseStream<string>(
    stream,
    (partial) => null,
    deserializer.coerce,
  );
};

TestFnNamedArgsSingleInt.registerImpl('v1', v1, v1_stream);


