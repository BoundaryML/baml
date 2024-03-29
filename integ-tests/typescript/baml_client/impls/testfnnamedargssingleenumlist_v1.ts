// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */


import { GPT35 } from '../client';
import { TestFnNamedArgsSingleEnumList } from '../function';
import { schema } from '../json_schema';
import { Deserializer } from '@boundaryml/baml-core/deserializer/deserializer';


const prompt_template = `\
Print these values back to me:
{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg//}\
`;

const deserializer = new Deserializer<string>(schema, {
  $ref: '#/definitions/TestFnNamedArgsSingleEnumList_output'
});

TestFnNamedArgsSingleEnumList.registerImpl('v1', async (
  args: {
    myArg: NamedArgsSingleEnumList[]
  }
): Promise<string> => {
    const myArg = args.myArg.map(x => x as NamedArgsSingleEnumList);
  
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


