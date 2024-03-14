// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck


import { Claude } from '../client';
import { PromptTest } from '../function';
import { schema } from '../json_schema';
import { Deserializer } from '@boundaryml/baml-core/deserializer/deserializer';


const prompt_template = `\
Tell me a haiku about {//BAML_CLIENT_REPLACE_ME_MAGIC_input//}\
`;

const deserializer = new Deserializer<string>(schema, {
  $ref: '#/definitions/PromptTest_output'
});

PromptTest.registerImpl('claude_chat', async (
  arg: string
): Promise<string> => {
  
    const result = await Claude.run_prompt_template(
      prompt_template,
      [
        "{//BAML_CLIENT_REPLACE_ME_MAGIC_input//}",
      ],
      {
        "{//BAML_CLIENT_REPLACE_ME_MAGIC_input//}": arg,
      }
    );


    return deserializer.coerce(result.generated);
  }
);
