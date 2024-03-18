// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */


import { GPT35 } from '../client';
import { TestFnNamedArgsSingleStringList } from '../function';
import { schema } from '../json_schema';
import { Deserializer } from '@boundaryml/baml-core/deserializer/deserializer';


const prompt_template = `\
Return this same value back: {//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg//}\
`;

const deserializer = new Deserializer<string>(schema, {
  $ref: '#/definitions/TestFnNamedArgsSingleStringList_output'
});

TestFnNamedArgsSingleStringList.registerImpl('v1', async (
  args: {
    myArg: string[]
  }
): Promise<string> => {
    const myArg = args.myArg.map(x => x);
  
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


