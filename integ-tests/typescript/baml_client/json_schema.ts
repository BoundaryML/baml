// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */


import { registerEnumDeserializer, registerObjectDeserializer } from '@boundaryml/baml-core/deserializer/deserializer';
import { JSONSchema7 } from 'json-schema';


const schema: JSONSchema7 = {
  "definitions": {
    "EnumInClass": {
      "title": "EnumInClass",
      "enum": [
        {
          "const": "ONE"
        },
        {
          "const": "TWO"
        }
      ]
    },
    "EnumOutput": {
      "title": "EnumOutput",
      "enum": [
        {
          "const": "ONE"
        },
        {
          "const": "TWO"
        },
        {
          "const": "THREE"
        }
      ]
    },
    "NamedArgsSingleEnum": {
      "title": "NamedArgsSingleEnum",
      "enum": [
        {
          "const": "ONE"
        },
        {
          "const": "TWO"
        }
      ]
    },
    "NamedArgsSingleEnumList": {
      "title": "NamedArgsSingleEnumList",
      "enum": [
        {
          "const": "ONE"
        },
        {
          "const": "TWO"
        }
      ]
    },
    "OverrideEnum": {
      "title": "OverrideEnum",
      "enum": [
        {
          "const": "ONE"
        },
        {
          "const": "TWO"
        }
      ]
    },
    "TestEnum": {
      "title": "TestEnum",
      "enum": [
        {
          "const": "A"
        },
        {
          "const": "B"
        },
        {
          "const": "C"
        },
        {
          "const": "D"
        },
        {
          "const": "E"
        }
      ]
    },
    "Blah": {
      "title": "Blah",
      "type": "object",
      "properties": {
        "prop4": {
          "type": [
            "string",
            "null"
          ],
          "default": null
        }
      },
      "required": []
    },
    "ClassOptionalFields": {
      "title": "ClassOptionalFields",
      "type": "object",
      "properties": {
        "prop1": {
          "type": [
            "string",
            "null"
          ],
          "default": null
        },
        "prop2": {
          "type": [
            "string",
            "null"
          ],
          "default": null
        }
      },
      "required": []
    },
    "ClassOptionalOutput": {
      "title": "ClassOptionalOutput",
      "type": "object",
      "properties": {
        "prop1": {
          "type": "string"
        },
        "prop2": {
          "type": "string"
        }
      },
      "required": [
        "prop1",
        "prop2"
      ]
    },
    "ClassOptionalOutput2": {
      "title": "ClassOptionalOutput2",
      "type": "object",
      "properties": {
        "prop1": {
          "type": [
            "string",
            "null"
          ],
          "default": null
        },
        "prop2": {
          "type": [
            "string",
            "null"
          ],
          "default": null
        },
        "prop3": {
          "anyOf": [
            {
              "$ref": "#/definitions/Blah",
              "title": "Blah"
            },
            {
              "type": "null",
              "title": "null"
            }
          ],
          "default": null
        }
      },
      "required": []
    },
    "ModifiedOutput": {
      "title": "ModifiedOutput",
      "type": "object",
      "properties": {
        "reasoning": {
          "type": "string"
        },
        "answer": {
          "type": "string"
        }
      },
      "required": [
        "reasoning",
        "answer"
      ]
    },
    "NamedArgsSingleClass": {
      "title": "NamedArgsSingleClass",
      "type": "object",
      "properties": {
        "key": {
          "type": "string"
        },
        "key_two": {
          "type": "boolean"
        },
        "key_three": {
          "type": "integer"
        }
      },
      "required": [
        "key",
        "key_two",
        "key_three"
      ]
    },
    "OptionalClass": {
      "title": "OptionalClass",
      "type": "object",
      "properties": {
        "prop1": {
          "type": "string"
        },
        "prop2": {
          "type": "string"
        }
      },
      "required": [
        "prop1",
        "prop2"
      ]
    },
    "OverrideClass": {
      "title": "OverrideClass",
      "type": "object",
      "properties": {
        "prop1": {
          "type": "string"
        },
        "prop2": {
          "type": "string"
        }
      },
      "required": [
        "prop1",
        "prop2"
      ]
    },
    "TestClassAlias": {
      "title": "TestClassAlias",
      "type": "object",
      "properties": {
        "key": {
          "type": "string"
        },
        "key2": {
          "type": "string"
        },
        "key3": {
          "type": "string"
        },
        "key4": {
          "type": "string"
        },
        "key5": {
          "type": "string"
        }
      },
      "required": [
        "key",
        "key2",
        "key3",
        "key4",
        "key5"
      ]
    },
    "TestClassWithEnum": {
      "title": "TestClassWithEnum",
      "type": "object",
      "properties": {
        "prop1": {
          "type": "string"
        },
        "prop2": {
          "$ref": "#/definitions/EnumInClass"
        }
      },
      "required": [
        "prop1",
        "prop2"
      ]
    },
    "TestOutputClass": {
      "title": "TestOutputClass",
      "type": "object",
      "properties": {
        "prop1": {
          "type": "string"
        },
        "prop2": {
          "type": "integer"
        }
      },
      "required": [
        "prop1",
        "prop2"
      ]
    },
    "FnClassOptional_input": {
      "anyOf": [
        {
          "$ref": "#/definitions/OptionalClass",
          "title": "OptionalClass"
        },
        {
          "type": "null",
          "title": "null"
        }
      ],
      "default": null,
      "title": "FnClassOptional input"
    },
    "FnClassOptional2_input": {
      "$ref": "#/definitions/ClassOptionalFields",
      "title": "FnClassOptional2 input"
    },
    "FnClassOptionalOutput_input": {
      "type": "string",
      "title": "FnClassOptionalOutput input"
    },
    "FnClassOptionalOutput2_input": {
      "type": "string",
      "title": "FnClassOptionalOutput2 input"
    },
    "FnEnumListOutput_input": {
      "type": "string",
      "title": "FnEnumListOutput input"
    },
    "FnEnumOutput_input": {
      "type": "string",
      "title": "FnEnumOutput input"
    },
    "FnNamedArgsSingleStringOptional_input": {
      "type": "object",
      "properties": {
        "myString": {
          "type": [
            "string",
            "null"
          ],
          "default": null
        }
      },
      "required": [
        "myString"
      ],
      "title": "FnNamedArgsSingleStringOptional input"
    },
    "FnOutputBool_input": {
      "type": "string",
      "title": "FnOutputBool input"
    },
    "FnOutputClass_input": {
      "type": "string",
      "title": "FnOutputClass input"
    },
    "FnOutputClassList_input": {
      "type": "string",
      "title": "FnOutputClassList input"
    },
    "FnOutputClassWithEnum_input": {
      "type": "string",
      "title": "FnOutputClassWithEnum input"
    },
    "FnOutputStringList_input": {
      "type": "string",
      "title": "FnOutputStringList input"
    },
    "FnStringOptional_input": {
      "type": [
        "string",
        "null"
      ],
      "default": null,
      "title": "FnStringOptional input"
    },
    "FnTestAliasedEnumOutput_input": {
      "type": "string",
      "title": "FnTestAliasedEnumOutput input"
    },
    "FnTestClassAlias_input": {
      "type": "string",
      "title": "FnTestClassAlias input"
    },
    "FnTestClassOverride_input": {
      "type": "string",
      "title": "FnTestClassOverride input"
    },
    "FnTestEnumOverride_input": {
      "type": "string",
      "title": "FnTestEnumOverride input"
    },
    "FnTestNamedArgsSingleEnum_input": {
      "type": "object",
      "properties": {
        "myArg": {
          "$ref": "#/definitions/NamedArgsSingleEnum"
        }
      },
      "required": [],
      "title": "FnTestNamedArgsSingleEnum input"
    },
    "FnTestOutputAdapter_input": {
      "type": "string",
      "title": "FnTestOutputAdapter input"
    },
    "FnUnionStringBoolWithArrayOutput_input": {
      "type": "string",
      "title": "FnUnionStringBoolWithArrayOutput input"
    },
    "PromptTest_input": {
      "type": "string",
      "title": "PromptTest input"
    },
    "TestFnNamedArgsSingleBool_input": {
      "type": "object",
      "properties": {
        "myBool": {
          "type": "boolean"
        }
      },
      "required": [],
      "title": "TestFnNamedArgsSingleBool input"
    },
    "TestFnNamedArgsSingleClass_input": {
      "type": "object",
      "properties": {
        "myArg": {
          "$ref": "#/definitions/NamedArgsSingleClass"
        }
      },
      "required": [],
      "title": "TestFnNamedArgsSingleClass input"
    },
    "TestFnNamedArgsSingleEnumList_input": {
      "type": "object",
      "properties": {
        "myArg": {
          "type": "array",
          "items": {
            "$ref": "#/definitions/NamedArgsSingleEnumList"
          }
        }
      },
      "required": [],
      "title": "TestFnNamedArgsSingleEnumList input"
    },
    "TestFnNamedArgsSingleFloat_input": {
      "type": "object",
      "properties": {
        "myFloat": {
          "type": "number"
        }
      },
      "required": [],
      "title": "TestFnNamedArgsSingleFloat input"
    },
    "TestFnNamedArgsSingleInt_input": {
      "type": "object",
      "properties": {
        "myInt": {
          "type": "integer"
        }
      },
      "required": [],
      "title": "TestFnNamedArgsSingleInt input"
    },
    "TestFnNamedArgsSingleString_input": {
      "type": "object",
      "properties": {
        "myString": {
          "type": "string"
        }
      },
      "required": [],
      "title": "TestFnNamedArgsSingleString input"
    },
    "TestFnNamedArgsSingleStringArray_input": {
      "type": "object",
      "properties": {
        "myStringArray": {
          "type": "array",
          "items": {
            "type": "string"
          }
        }
      },
      "required": [],
      "title": "TestFnNamedArgsSingleStringArray input"
    },
    "TestFnNamedArgsSingleStringList_input": {
      "type": "object",
      "properties": {
        "myArg": {
          "type": "array",
          "items": {
            "type": "string"
          }
        }
      },
      "required": [],
      "title": "TestFnNamedArgsSingleStringList input"
    },
    "TestFnNamedArgsSyntax_input": {
      "type": "object",
      "properties": {
        "var": {
          "type": "string"
        },
        "var_with_underscores": {
          "type": "string"
        }
      },
      "required": [],
      "title": "TestFnNamedArgsSyntax input"
    },
    "FnClassOptional_output": {
      "type": "string",
      "title": "FnClassOptional output"
    },
    "FnClassOptional2_output": {
      "type": "string",
      "title": "FnClassOptional2 output"
    },
    "FnClassOptionalOutput_output": {
      "anyOf": [
        {
          "$ref": "#/definitions/ClassOptionalOutput",
          "title": "ClassOptionalOutput"
        },
        {
          "type": "null",
          "title": "null"
        }
      ],
      "default": null,
      "title": "FnClassOptionalOutput output"
    },
    "FnClassOptionalOutput2_output": {
      "anyOf": [
        {
          "$ref": "#/definitions/ClassOptionalOutput2",
          "title": "ClassOptionalOutput2"
        },
        {
          "type": "null",
          "title": "null"
        }
      ],
      "default": null,
      "title": "FnClassOptionalOutput2 output"
    },
    "FnEnumListOutput_output": {
      "type": "array",
      "items": {
        "$ref": "#/definitions/EnumOutput"
      },
      "title": "FnEnumListOutput output"
    },
    "FnEnumOutput_output": {
      "$ref": "#/definitions/EnumOutput",
      "title": "FnEnumOutput output"
    },
    "FnNamedArgsSingleStringOptional_output": {
      "type": "string",
      "title": "FnNamedArgsSingleStringOptional output"
    },
    "FnOutputBool_output": {
      "type": "boolean",
      "title": "FnOutputBool output"
    },
    "FnOutputClass_output": {
      "$ref": "#/definitions/TestOutputClass",
      "title": "FnOutputClass output"
    },
    "FnOutputClassList_output": {
      "type": "array",
      "items": {
        "$ref": "#/definitions/TestOutputClass"
      },
      "title": "FnOutputClassList output"
    },
    "FnOutputClassWithEnum_output": {
      "$ref": "#/definitions/TestClassWithEnum",
      "title": "FnOutputClassWithEnum output"
    },
    "FnOutputStringList_output": {
      "type": "array",
      "items": {
        "type": "string"
      },
      "title": "FnOutputStringList output"
    },
    "FnStringOptional_output": {
      "type": "string",
      "title": "FnStringOptional output"
    },
    "FnTestAliasedEnumOutput_output": {
      "$ref": "#/definitions/TestEnum",
      "title": "FnTestAliasedEnumOutput output"
    },
    "FnTestClassAlias_output": {
      "$ref": "#/definitions/TestClassAlias",
      "title": "FnTestClassAlias output"
    },
    "FnTestClassOverride_output": {
      "$ref": "#/definitions/OverrideClass",
      "title": "FnTestClassOverride output"
    },
    "FnTestEnumOverride_output": {
      "$ref": "#/definitions/OverrideEnum",
      "title": "FnTestEnumOverride output"
    },
    "FnTestNamedArgsSingleEnum_output": {
      "type": "string",
      "title": "FnTestNamedArgsSingleEnum output"
    },
    "FnTestOutputAdapter_output": {
      "type": "string",
      "title": "FnTestOutputAdapter output"
    },
    "FnUnionStringBoolWithArrayOutput_output": {
      "type": "number",
      "title": "FnUnionStringBoolWithArrayOutput output"
    },
    "PromptTest_output": {
      "type": "string",
      "title": "PromptTest output"
    },
    "TestFnNamedArgsSingleBool_output": {
      "type": "string",
      "title": "TestFnNamedArgsSingleBool output"
    },
    "TestFnNamedArgsSingleClass_output": {
      "type": "string",
      "title": "TestFnNamedArgsSingleClass output"
    },
    "TestFnNamedArgsSingleEnumList_output": {
      "type": "string",
      "title": "TestFnNamedArgsSingleEnumList output"
    },
    "TestFnNamedArgsSingleFloat_output": {
      "type": "string",
      "title": "TestFnNamedArgsSingleFloat output"
    },
    "TestFnNamedArgsSingleInt_output": {
      "type": "string",
      "title": "TestFnNamedArgsSingleInt output"
    },
    "TestFnNamedArgsSingleString_output": {
      "type": "string",
      "title": "TestFnNamedArgsSingleString output"
    },
    "TestFnNamedArgsSingleStringArray_output": {
      "type": "string",
      "title": "TestFnNamedArgsSingleStringArray output"
    },
    "TestFnNamedArgsSingleStringList_output": {
      "type": "string",
      "title": "TestFnNamedArgsSingleStringList output"
    },
    "TestFnNamedArgsSyntax_output": {
      "type": "string",
      "title": "TestFnNamedArgsSyntax output"
    }
  }
};

registerEnumDeserializer(schema.definitions.EnumInClass, {

});

registerEnumDeserializer(schema.definitions.EnumOutput, {

});

registerEnumDeserializer(schema.definitions.NamedArgsSingleEnum, {

});

registerEnumDeserializer(schema.definitions.NamedArgsSingleEnumList, {

});

registerEnumDeserializer(schema.definitions.OverrideEnum, {

});

registerEnumDeserializer(schema.definitions.TestEnum, {
  "k1": "A",
  "k1: User is angry": "A",
  "k22": "B",
  "k22: User is happy": "B",
  "k11": "C",
  "k11: User is sad": "C",
  "k44": "D",
  "k44: User is confused": "D"
});

registerObjectDeserializer(schema.definitions.Blah, { });

registerObjectDeserializer(schema.definitions.ClassOptionalFields, { });

registerObjectDeserializer(schema.definitions.ClassOptionalOutput, { });

registerObjectDeserializer(schema.definitions.ClassOptionalOutput2, { });

registerObjectDeserializer(schema.definitions.ModifiedOutput, { });

registerObjectDeserializer(schema.definitions.NamedArgsSingleClass, { });

registerObjectDeserializer(schema.definitions.OptionalClass, { });

registerObjectDeserializer(schema.definitions.OverrideClass, { });

registerObjectDeserializer(schema.definitions.TestClassAlias, { });

registerObjectDeserializer(schema.definitions.TestClassWithEnum, { });

registerObjectDeserializer(schema.definitions.TestOutputClass, { });


export { schema }

