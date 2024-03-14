// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck


import { GPT35 } from '../client';
import { TestFnNamedArgsSingleEnum } from '../function';
import { schema } from '../json_schema';
import { Deserializer } from '@boundaryml/baml-core/deserializer/deserializer';


const prompt_template = `\
Print these values back to me:
{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg//}\
`;

const deserializer = new Deserializer<string>(schema, {
  $ref: '#/definitions/TestFnNamedArgsSingleEnum_output'
});

TestFnNamedArgsSingleEnum.registerImpl('v1', async (
  args: {
    myArg: NamedArgsSingleEnum
  }
): Promise<string> => {
    const myArg = args.myArg as NamedArgsSingleEnum;
  
    const result = await GPT35.run_prompt_template(
      prompt_template,
      [
        "{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg//}",
      ],
      {
        "{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg//}": myArg,
      }
    );


    return deserializer.coerce(result.generated);
  }
);
