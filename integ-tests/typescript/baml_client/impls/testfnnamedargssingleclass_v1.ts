// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck


import { GPT35 } from '../client';
import { TestFnNamedArgsSingleClass } from '../function';
import { schema } from '../json_schema';
import { InternalNamedArgsSingleClass } from '../types_internal';
import { Deserializer } from '@boundaryml/baml-core/deserializer/deserializer';


const prompt_template = `\
Print these values back to me:
{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg.key//}
{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg.key_two//}
{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg.key_three//}\
`;

const deserializer = new Deserializer<string>(schema, {
  $ref: '#/definitions/TestFnNamedArgsSingleClass_output'
});

TestFnNamedArgsSingleClass.registerImpl('v1', async (
  args: {
    myArg: NamedArgsSingleClass
  }
): Promise<string> => {
    const myArg = InternalNamedArgsSingleClass.from(args.myArg);
  
    const result = await GPT35.run_prompt_template(
      prompt_template,
      [
        "{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg.key_two//}",
      
        "{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg.key//}",
      
        "{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg.key_three//}",
      ],
      {
        "{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg.key_two//}": myArg.key_two,
        "{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg.key//}": myArg.key,
        "{//BAML_CLIENT_REPLACE_ME_MAGIC_input.myArg.key_three//}": myArg.key_three,
      }
    );


    return deserializer.coerce(result.generated);
  }
);
