// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */

const enum DataType {
    Resume = "Resume",
    Event = "Event",
}

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

const enum OptionalTest_CategoryType {
    Aleph = "Aleph",
    Beta = "Beta",
    Gamma = "Gamma",
}

const enum OverrideEnum {
    ONE = "ONE",
    TWO = "TWO",
}

const enum Tag {
    Security = "Security",
    AI = "AI",
    Blockchain = "Blockchain",
}

const enum TestEnum {
    A = "A",
    B = "B",
    C = "C",
    D = "D",
    E = "E",
    F = "F",
    G = "G",
}

interface Blah {
  prop4: string | null;
}

interface ClassOptionalFields {
  prop1: string | null;
  prop2: string | null;
}

interface ClassOptionalOutput {
  prop1: string;
  prop2: string;
}

interface ClassOptionalOutput2 {
  prop1: string | null;
  prop2: string | null;
  prop3: Blah | null;
}

interface DynamicPropsClass {
  prop1: string;
  prop2: string;
  prop3: number;
}

interface Event {
  title: string;
  date: string;
  location: string;
  description: string;
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

interface OptionalClass {
  prop1: string;
  prop2: string;
}

interface OptionalTest_Prop1 {
  omega_a: string;
  omega_b: number;
}

interface OptionalTest_ReturnType {
  omega_1: OptionalTest_Prop1 | null;
  omega_2: string | null;
  omega_3: OptionalTest_CategoryType | null[];
}

interface OverrideClass {
  prop1: string;
  prop2: string;
}

interface RaysData {
  dataType: DataType;
  value: Resume | Event;
}

interface Resume {
  name: string;
  email: string;
  phone: string;
  experience: string[];
  education: string[];
  skills: string[];
}

interface SearchParams {
  dateRange: number | null;
  location: string[];
  jobTitle: WithReasoning | null;
  company: WithReasoning | null;
  description: WithReasoning[];
  tags: Tag | string[];
}

interface SomeClass2 {
  prop1: string;
  prop2: string;
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

interface UnionTest_ReturnType {
  prop1: string | boolean;
  prop2: number | boolean[];
  prop3: number[] | boolean[];
}

interface WithReasoning {
  value: string;
  reasoning: string;
}


export { DataType, EnumInClass, EnumOutput, NamedArgsSingleEnum, NamedArgsSingleEnumList, OptionalTest_CategoryType, OverrideEnum, Tag, TestEnum, Blah, ClassOptionalFields, ClassOptionalOutput, ClassOptionalOutput2, DynamicPropsClass, Event, ModifiedOutput, NamedArgsSingleClass, OptionalClass, OptionalTest_Prop1, OptionalTest_ReturnType, OverrideClass, RaysData, Resume, SearchParams, SomeClass2, TestClassAlias, TestClassWithEnum, TestOutputClass, UnionTest_ReturnType, WithReasoning }

