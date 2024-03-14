// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck

const enum EnumInClass {
    ONE = "ONE",
    TWO = "TWO",
}

const enum EnumOutput {
    ONE = "ONE",
    TWO = "TWO",
    THREE = "THREE",
}

const enum NamedArgsSingleEnum {
    ONE = "ONE",
    TWO = "TWO",
}

const enum NamedArgsSingleEnumList {
    ONE = "ONE",
    TWO = "TWO",
}

const enum TestEnum {
    A = "A",
    B = "B",
    C = "C",
    D = "D",
    E = "E",
}

interface ModifiedOutput {
  reasoning: string;
  answer: string;
}

interface NamedArgsSingleClass {
  key: string;
  key_two: boolean;
  key_three: number;
}

interface TestClassAlias {
  key: string;
  key2: string;
  key3: string;
  key4: string;
  key5: string;
}

interface TestClassWithEnum {
  prop1: string;
  prop2: EnumInClass;
}

interface TestOutputClass {
  prop1: string;
  prop2: number;
}


export { EnumInClass, EnumOutput, NamedArgsSingleEnum, NamedArgsSingleEnumList, TestEnum, ModifiedOutput, NamedArgsSingleClass, TestClassAlias, TestClassWithEnum, TestOutputClass }