/*************************************************************************************************

Welcome to Baml! To use this generated code, please run one of the following:

$ npm install @boundaryml/baml
$ yarn add @boundaryml/baml
$ pnpm add @boundaryml/baml

*************************************************************************************************/

// This file was generated by BAML: do not edit it. Instead, edit the BAML
// files and re-generate this code.
//
// tslint:disable
// @ts-nocheck
// biome-ignore format: autogenerated code
/* eslint-disable */
import { BamlRuntimePy, FunctionResult, BamlCtxManager, BamlStream } from "@boundaryml/baml"
import {Blah, ClassOptionalOutput, ClassOptionalOutput2, ClassWithImage, Education, Email, Event, FakeImage, NamedArgsSingleClass, OptionalTest_Prop1, OptionalTest_ReturnType, OrderInfo, RaysData, Resume, SearchParams, TestClassAlias, TestClassWithEnum, TestOutputClass, TestOutputClassNested, UnionTest_ReturnType, WithReasoning, Category, Category2, Category3, DataType, EnumInClass, EnumOutput, NamedArgsSingleEnum, NamedArgsSingleEnumList, OptionalTest_CategoryType, OrderStatus, Tag, TestEnum} from "./types"

export class BamlClient {
  private stream_client: BamlStreamClient

  constructor(private runtime: BamlRuntimePy, private ctx_manager: BamlCtxManager) {
    this.stream_client = new BamlStreamClient(runtime, ctx_manager)
  }

  get stream() {
    return this.stream_client
  }  

  
  async ClassifyMessage(
      input: string
  ): Promise<Category> {
    const raw = await this.runtime.callFunction(
      "ClassifyMessage",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as Category
  }
  
  async ClassifyMessage2(
      input: string
  ): Promise<Category> {
    const raw = await this.runtime.callFunction(
      "ClassifyMessage2",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as Category
  }
  
  async ClassifyMessage3(
      input: string
  ): Promise<Category> {
    const raw = await this.runtime.callFunction(
      "ClassifyMessage3",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as Category
  }
  
  async DescribeImage(
      img: baml_ts.Image
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "DescribeImage",
      {
        "img": img,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async DescribeImage2(
      classWithImage: ClassWithImage,img2: baml_ts.Image
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "DescribeImage2",
      {
        "classWithImage": classWithImage,"img2": img2,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async DescribeImage3(
      classWithImage: ClassWithImage,img2: baml_ts.Image
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "DescribeImage3",
      {
        "classWithImage": classWithImage,"img2": img2,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async DescribeImage4(
      classWithImage: ClassWithImage,img2: baml_ts.Image
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "DescribeImage4",
      {
        "classWithImage": classWithImage,"img2": img2,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async ExtractNames(
      input: string
  ): Promise<string[]> {
    const raw = await this.runtime.callFunction(
      "ExtractNames",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string[]
  }
  
  async ExtractResume(
      resume: string,img: baml_ts.Image
  ): Promise<Resume> {
    const raw = await this.runtime.callFunction(
      "ExtractResume",
      {
        "resume": resume,"img": img,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as Resume
  }
  
  async ExtractResume2(
      resume: string
  ): Promise<Resume> {
    const raw = await this.runtime.callFunction(
      "ExtractResume2",
      {
        "resume": resume,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as Resume
  }
  
  async FnClassOptionalOutput(
      input: string
  ): Promise<ClassOptionalOutput | null> {
    const raw = await this.runtime.callFunction(
      "FnClassOptionalOutput",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as ClassOptionalOutput | null
  }
  
  async FnClassOptionalOutput2(
      input: string
  ): Promise<ClassOptionalOutput2 | null> {
    const raw = await this.runtime.callFunction(
      "FnClassOptionalOutput2",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as ClassOptionalOutput2 | null
  }
  
  async FnEnumListOutput(
      input: string
  ): Promise<EnumOutput[]> {
    const raw = await this.runtime.callFunction(
      "FnEnumListOutput",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as EnumOutput[]
  }
  
  async FnEnumOutput(
      input: string
  ): Promise<EnumOutput> {
    const raw = await this.runtime.callFunction(
      "FnEnumOutput",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as EnumOutput
  }
  
  async FnNamedArgsSingleStringOptional(
      myString: string | null
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "FnNamedArgsSingleStringOptional",
      {
        "myString": myString,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async FnOutputBool(
      input: string
  ): Promise<boolean> {
    const raw = await this.runtime.callFunction(
      "FnOutputBool",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as boolean
  }
  
  async FnOutputClass(
      input: string
  ): Promise<TestOutputClass> {
    const raw = await this.runtime.callFunction(
      "FnOutputClass",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as TestOutputClass
  }
  
  async FnOutputClassList(
      input: string
  ): Promise<TestOutputClass[]> {
    const raw = await this.runtime.callFunction(
      "FnOutputClassList",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as TestOutputClass[]
  }
  
  async FnOutputClassWithEnum(
      input: string
  ): Promise<TestClassWithEnum> {
    const raw = await this.runtime.callFunction(
      "FnOutputClassWithEnum",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as TestClassWithEnum
  }
  
  async FnOutputNestedClass(
      input: string
  ): Promise<TestOutputClassNested> {
    const raw = await this.runtime.callFunction(
      "FnOutputNestedClass",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as TestOutputClassNested
  }
  
  async FnOutputStringList(
      input: string
  ): Promise<string[]> {
    const raw = await this.runtime.callFunction(
      "FnOutputStringList",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string[]
  }
  
  async FnTestAliasedEnumOutput(
      input: string
  ): Promise<TestEnum> {
    const raw = await this.runtime.callFunction(
      "FnTestAliasedEnumOutput",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as TestEnum
  }
  
  async FnTestClassAlias(
      input: string
  ): Promise<TestClassAlias> {
    const raw = await this.runtime.callFunction(
      "FnTestClassAlias",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as TestClassAlias
  }
  
  async FnTestNamedArgsSingleEnum(
      myArg: NamedArgsSingleEnum
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "FnTestNamedArgsSingleEnum",
      {
        "myArg": myArg,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async GetDataType(
      text: string
  ): Promise<RaysData> {
    const raw = await this.runtime.callFunction(
      "GetDataType",
      {
        "text": text,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as RaysData
  }
  
  async GetOrderInfo(
      email: Email
  ): Promise<OrderInfo> {
    const raw = await this.runtime.callFunction(
      "GetOrderInfo",
      {
        "email": email,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as OrderInfo
  }
  
  async GetQuery(
      query: string
  ): Promise<SearchParams> {
    const raw = await this.runtime.callFunction(
      "GetQuery",
      {
        "query": query,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as SearchParams
  }
  
  async OptionalTest_Function(
      input: string
  ): Promise<(OptionalTest_ReturnType | null)[]> {
    const raw = await this.runtime.callFunction(
      "OptionalTest_Function",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as (OptionalTest_ReturnType | null)[]
  }
  
  async PromptTestClaude(
      input: string
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "PromptTestClaude",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async PromptTestClaudeChat(
      input: string
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "PromptTestClaudeChat",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async PromptTestClaudeChatNoSystem(
      input: string
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "PromptTestClaudeChatNoSystem",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async PromptTestOpenAI(
      input: string
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "PromptTestOpenAI",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async PromptTestOpenAIChat(
      input: string
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "PromptTestOpenAIChat",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async PromptTestOpenAIChatNoSystem(
      input: string
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "PromptTestOpenAIChatNoSystem",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async TestFnNamedArgsSingleBool(
      myBool: boolean
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "TestFnNamedArgsSingleBool",
      {
        "myBool": myBool,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async TestFnNamedArgsSingleClass(
      myArg: NamedArgsSingleClass
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "TestFnNamedArgsSingleClass",
      {
        "myArg": myArg,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async TestFnNamedArgsSingleEnumList(
      myArg: NamedArgsSingleEnumList[]
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "TestFnNamedArgsSingleEnumList",
      {
        "myArg": myArg,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async TestFnNamedArgsSingleFloat(
      myFloat: number
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "TestFnNamedArgsSingleFloat",
      {
        "myFloat": myFloat,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async TestFnNamedArgsSingleInt(
      myInt: number
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "TestFnNamedArgsSingleInt",
      {
        "myInt": myInt,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async TestFnNamedArgsSingleString(
      myString: string
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "TestFnNamedArgsSingleString",
      {
        "myString": myString,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async TestFnNamedArgsSingleStringArray(
      myStringArray: string[]
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "TestFnNamedArgsSingleStringArray",
      {
        "myStringArray": myStringArray,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async TestFnNamedArgsSingleStringList(
      myArg: string[]
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "TestFnNamedArgsSingleStringList",
      {
        "myArg": myArg,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async TestImageInput(
      img: baml_ts.Image
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "TestImageInput",
      {
        "img": img,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async TestMulticlassNamedArgs(
      myArg: NamedArgsSingleClass,myArg2: NamedArgsSingleClass
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "TestMulticlassNamedArgs",
      {
        "myArg": myArg,"myArg2": myArg2,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async TestOllama(
      input: string
  ): Promise<string> {
    const raw = await this.runtime.callFunction(
      "TestOllama",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as string
  }
  
  async UnionTest_Function(
      input: string | boolean
  ): Promise<UnionTest_ReturnType> {
    const raw = await this.runtime.callFunction(
      "UnionTest_Function",
      {
        "input": input,
      },
      this.ctx_manager.get(),
    )
    return raw.parsed() as UnionTest_ReturnType
  }
  
}

class BamlStreamClient {
  constructor(private runtime: BamlRuntimePy, private ctx_manager: BamlCtxManager) {}

  
  ClassifyMessage(
      input: string
  ): BamlStream<(Category | null), Category> {
    const raw = this.runtime.streamFunction(
      "ClassifyMessage",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Category | null), Category>(
      raw,
      (a): a is (Category | null) => a,
      (a): a is Category => a,
      this.ctx_manager.get(),
    )
  }
  
  ClassifyMessage2(
      input: string
  ): BamlStream<(Category | null), Category> {
    const raw = this.runtime.streamFunction(
      "ClassifyMessage2",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Category | null), Category>(
      raw,
      (a): a is (Category | null) => a,
      (a): a is Category => a,
      this.ctx_manager.get(),
    )
  }
  
  ClassifyMessage3(
      input: string
  ): BamlStream<(Category | null), Category> {
    const raw = this.runtime.streamFunction(
      "ClassifyMessage3",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Category | null), Category>(
      raw,
      (a): a is (Category | null) => a,
      (a): a is Category => a,
      this.ctx_manager.get(),
    )
  }
  
  DescribeImage(
      img: baml_ts.Image
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "DescribeImage",
      {
        "img": img,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  DescribeImage2(
      classWithImage: ClassWithImage,img2: baml_ts.Image
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "DescribeImage2",
      {
        "classWithImage": classWithImage,
        "img2": img2,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  DescribeImage3(
      classWithImage: ClassWithImage,img2: baml_ts.Image
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "DescribeImage3",
      {
        "classWithImage": classWithImage,
        "img2": img2,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  DescribeImage4(
      classWithImage: ClassWithImage,img2: baml_ts.Image
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "DescribeImage4",
      {
        "classWithImage": classWithImage,
        "img2": img2,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  ExtractNames(
      input: string
  ): BamlStream<(string | null)[], string[]> {
    const raw = this.runtime.streamFunction(
      "ExtractNames",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null)[], string[]>(
      raw,
      (a): a is (string | null)[] => a,
      (a): a is string[] => a,
      this.ctx_manager.get(),
    )
  }
  
  ExtractResume(
      resume: string,img: baml_ts.Image
  ): BamlStream<(Partial<Resume> | null), Resume> {
    const raw = this.runtime.streamFunction(
      "ExtractResume",
      {
        "resume": resume,
        "img": img,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Partial<Resume> | null), Resume>(
      raw,
      (a): a is (Partial<Resume> | null) => a,
      (a): a is Resume => a,
      this.ctx_manager.get(),
    )
  }
  
  ExtractResume2(
      resume: string
  ): BamlStream<(Partial<Resume> | null), Resume> {
    const raw = this.runtime.streamFunction(
      "ExtractResume2",
      {
        "resume": resume,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Partial<Resume> | null), Resume>(
      raw,
      (a): a is (Partial<Resume> | null) => a,
      (a): a is Resume => a,
      this.ctx_manager.get(),
    )
  }
  
  FnClassOptionalOutput(
      input: string
  ): BamlStream<((Partial<ClassOptionalOutput> | null) | null), ClassOptionalOutput | null> {
    const raw = this.runtime.streamFunction(
      "FnClassOptionalOutput",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<((Partial<ClassOptionalOutput> | null) | null), ClassOptionalOutput | null>(
      raw,
      (a): a is ((Partial<ClassOptionalOutput> | null) | null) => a,
      (a): a is ClassOptionalOutput | null => a,
      this.ctx_manager.get(),
    )
  }
  
  FnClassOptionalOutput2(
      input: string
  ): BamlStream<((Partial<ClassOptionalOutput2> | null) | null), ClassOptionalOutput2 | null> {
    const raw = this.runtime.streamFunction(
      "FnClassOptionalOutput2",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<((Partial<ClassOptionalOutput2> | null) | null), ClassOptionalOutput2 | null>(
      raw,
      (a): a is ((Partial<ClassOptionalOutput2> | null) | null) => a,
      (a): a is ClassOptionalOutput2 | null => a,
      this.ctx_manager.get(),
    )
  }
  
  FnEnumListOutput(
      input: string
  ): BamlStream<(EnumOutput | null)[], EnumOutput[]> {
    const raw = this.runtime.streamFunction(
      "FnEnumListOutput",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(EnumOutput | null)[], EnumOutput[]>(
      raw,
      (a): a is (EnumOutput | null)[] => a,
      (a): a is EnumOutput[] => a,
      this.ctx_manager.get(),
    )
  }
  
  FnEnumOutput(
      input: string
  ): BamlStream<(EnumOutput | null), EnumOutput> {
    const raw = this.runtime.streamFunction(
      "FnEnumOutput",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(EnumOutput | null), EnumOutput>(
      raw,
      (a): a is (EnumOutput | null) => a,
      (a): a is EnumOutput => a,
      this.ctx_manager.get(),
    )
  }
  
  FnNamedArgsSingleStringOptional(
      myString: string | null
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "FnNamedArgsSingleStringOptional",
      {
        "myString": myString,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  FnOutputBool(
      input: string
  ): BamlStream<(boolean | null), boolean> {
    const raw = this.runtime.streamFunction(
      "FnOutputBool",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(boolean | null), boolean>(
      raw,
      (a): a is (boolean | null) => a,
      (a): a is boolean => a,
      this.ctx_manager.get(),
    )
  }
  
  FnOutputClass(
      input: string
  ): BamlStream<(Partial<TestOutputClass> | null), TestOutputClass> {
    const raw = this.runtime.streamFunction(
      "FnOutputClass",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Partial<TestOutputClass> | null), TestOutputClass>(
      raw,
      (a): a is (Partial<TestOutputClass> | null) => a,
      (a): a is TestOutputClass => a,
      this.ctx_manager.get(),
    )
  }
  
  FnOutputClassList(
      input: string
  ): BamlStream<(Partial<TestOutputClass> | null)[], TestOutputClass[]> {
    const raw = this.runtime.streamFunction(
      "FnOutputClassList",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Partial<TestOutputClass> | null)[], TestOutputClass[]>(
      raw,
      (a): a is (Partial<TestOutputClass> | null)[] => a,
      (a): a is TestOutputClass[] => a,
      this.ctx_manager.get(),
    )
  }
  
  FnOutputClassWithEnum(
      input: string
  ): BamlStream<(Partial<TestClassWithEnum> | null), TestClassWithEnum> {
    const raw = this.runtime.streamFunction(
      "FnOutputClassWithEnum",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Partial<TestClassWithEnum> | null), TestClassWithEnum>(
      raw,
      (a): a is (Partial<TestClassWithEnum> | null) => a,
      (a): a is TestClassWithEnum => a,
      this.ctx_manager.get(),
    )
  }
  
  FnOutputNestedClass(
      input: string
  ): BamlStream<(Partial<TestOutputClassNested> | null), TestOutputClassNested> {
    const raw = this.runtime.streamFunction(
      "FnOutputNestedClass",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Partial<TestOutputClassNested> | null), TestOutputClassNested>(
      raw,
      (a): a is (Partial<TestOutputClassNested> | null) => a,
      (a): a is TestOutputClassNested => a,
      this.ctx_manager.get(),
    )
  }
  
  FnOutputStringList(
      input: string
  ): BamlStream<(string | null)[], string[]> {
    const raw = this.runtime.streamFunction(
      "FnOutputStringList",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null)[], string[]>(
      raw,
      (a): a is (string | null)[] => a,
      (a): a is string[] => a,
      this.ctx_manager.get(),
    )
  }
  
  FnTestAliasedEnumOutput(
      input: string
  ): BamlStream<(TestEnum | null), TestEnum> {
    const raw = this.runtime.streamFunction(
      "FnTestAliasedEnumOutput",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(TestEnum | null), TestEnum>(
      raw,
      (a): a is (TestEnum | null) => a,
      (a): a is TestEnum => a,
      this.ctx_manager.get(),
    )
  }
  
  FnTestClassAlias(
      input: string
  ): BamlStream<(Partial<TestClassAlias> | null), TestClassAlias> {
    const raw = this.runtime.streamFunction(
      "FnTestClassAlias",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Partial<TestClassAlias> | null), TestClassAlias>(
      raw,
      (a): a is (Partial<TestClassAlias> | null) => a,
      (a): a is TestClassAlias => a,
      this.ctx_manager.get(),
    )
  }
  
  FnTestNamedArgsSingleEnum(
      myArg: NamedArgsSingleEnum
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "FnTestNamedArgsSingleEnum",
      {
        "myArg": myArg,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  GetDataType(
      text: string
  ): BamlStream<(Partial<RaysData> | null), RaysData> {
    const raw = this.runtime.streamFunction(
      "GetDataType",
      {
        "text": text,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Partial<RaysData> | null), RaysData>(
      raw,
      (a): a is (Partial<RaysData> | null) => a,
      (a): a is RaysData => a,
      this.ctx_manager.get(),
    )
  }
  
  GetOrderInfo(
      email: Email
  ): BamlStream<(Partial<OrderInfo> | null), OrderInfo> {
    const raw = this.runtime.streamFunction(
      "GetOrderInfo",
      {
        "email": email,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Partial<OrderInfo> | null), OrderInfo>(
      raw,
      (a): a is (Partial<OrderInfo> | null) => a,
      (a): a is OrderInfo => a,
      this.ctx_manager.get(),
    )
  }
  
  GetQuery(
      query: string
  ): BamlStream<(Partial<SearchParams> | null), SearchParams> {
    const raw = this.runtime.streamFunction(
      "GetQuery",
      {
        "query": query,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Partial<SearchParams> | null), SearchParams>(
      raw,
      (a): a is (Partial<SearchParams> | null) => a,
      (a): a is SearchParams => a,
      this.ctx_manager.get(),
    )
  }
  
  OptionalTest_Function(
      input: string
  ): BamlStream<((Partial<OptionalTest_ReturnType> | null) | null)[], (OptionalTest_ReturnType | null)[]> {
    const raw = this.runtime.streamFunction(
      "OptionalTest_Function",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<((Partial<OptionalTest_ReturnType> | null) | null)[], (OptionalTest_ReturnType | null)[]>(
      raw,
      (a): a is ((Partial<OptionalTest_ReturnType> | null) | null)[] => a,
      (a): a is (OptionalTest_ReturnType | null)[] => a,
      this.ctx_manager.get(),
    )
  }
  
  PromptTestClaude(
      input: string
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "PromptTestClaude",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  PromptTestClaudeChat(
      input: string
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "PromptTestClaudeChat",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  PromptTestClaudeChatNoSystem(
      input: string
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "PromptTestClaudeChatNoSystem",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  PromptTestOpenAI(
      input: string
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "PromptTestOpenAI",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  PromptTestOpenAIChat(
      input: string
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "PromptTestOpenAIChat",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  PromptTestOpenAIChatNoSystem(
      input: string
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "PromptTestOpenAIChatNoSystem",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  TestFnNamedArgsSingleBool(
      myBool: boolean
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "TestFnNamedArgsSingleBool",
      {
        "myBool": myBool,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  TestFnNamedArgsSingleClass(
      myArg: NamedArgsSingleClass
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "TestFnNamedArgsSingleClass",
      {
        "myArg": myArg,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  TestFnNamedArgsSingleEnumList(
      myArg: NamedArgsSingleEnumList[]
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "TestFnNamedArgsSingleEnumList",
      {
        "myArg": myArg,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  TestFnNamedArgsSingleFloat(
      myFloat: number
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "TestFnNamedArgsSingleFloat",
      {
        "myFloat": myFloat,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  TestFnNamedArgsSingleInt(
      myInt: number
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "TestFnNamedArgsSingleInt",
      {
        "myInt": myInt,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  TestFnNamedArgsSingleString(
      myString: string
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "TestFnNamedArgsSingleString",
      {
        "myString": myString,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  TestFnNamedArgsSingleStringArray(
      myStringArray: string[]
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "TestFnNamedArgsSingleStringArray",
      {
        "myStringArray": myStringArray,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  TestFnNamedArgsSingleStringList(
      myArg: string[]
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "TestFnNamedArgsSingleStringList",
      {
        "myArg": myArg,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  TestImageInput(
      img: baml_ts.Image
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "TestImageInput",
      {
        "img": img,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  TestMulticlassNamedArgs(
      myArg: NamedArgsSingleClass,myArg2: NamedArgsSingleClass
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "TestMulticlassNamedArgs",
      {
        "myArg": myArg,
        "myArg2": myArg2,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  TestOllama(
      input: string
  ): BamlStream<(string | null), string> {
    const raw = this.runtime.streamFunction(
      "TestOllama",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(string | null), string>(
      raw,
      (a): a is (string | null) => a,
      (a): a is string => a,
      this.ctx_manager.get(),
    )
  }
  
  UnionTest_Function(
      input: string | boolean
  ): BamlStream<(Partial<UnionTest_ReturnType> | null), UnionTest_ReturnType> {
    const raw = this.runtime.streamFunction(
      "UnionTest_Function",
      {
        "input": input,
      },
      undefined,
      this.ctx_manager.get(),
    )
    return new BamlStream<(Partial<UnionTest_ReturnType> | null), UnionTest_ReturnType>(
      raw,
      (a): a is (Partial<UnionTest_ReturnType> | null) => a,
      (a): a is UnionTest_ReturnType => a,
      this.ctx_manager.get(),
    )
  }
  
}