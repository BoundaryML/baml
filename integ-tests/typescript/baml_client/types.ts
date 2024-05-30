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
import { Image } from "@boundaryml/baml"
export enum Category {
  Refund = "Refund",
  CancelOrder = "CancelOrder",
  TechnicalSupport = "TechnicalSupport",
  AccountIssue = "AccountIssue",
  Question = "Question",
}

export enum Category2 {
  Refund = "Refund",
  CancelOrder = "CancelOrder",
  TechnicalSupport = "TechnicalSupport",
  AccountIssue = "AccountIssue",
  Question = "Question",
}

export enum Category3 {
  Refund = "Refund",
  CancelOrder = "CancelOrder",
  TechnicalSupport = "TechnicalSupport",
  AccountIssue = "AccountIssue",
  Question = "Question",
}

export enum DataType {
  Resume = "Resume",
  Event = "Event",
}

export enum EnumInClass {
  ONE = "ONE",
  TWO = "TWO",
}

export enum EnumOutput {
  ONE = "ONE",
  TWO = "TWO",
  THREE = "THREE",
}

export enum Hobby {
  SPORTS = "SPORTS",
  MUSIC = "MUSIC",
  READING = "READING",
}

export enum NamedArgsSingleEnum {
  ONE = "ONE",
  TWO = "TWO",
}

export enum NamedArgsSingleEnumList {
  ONE = "ONE",
  TWO = "TWO",
}

export enum OptionalTest_CategoryType {
  Aleph = "Aleph",
  Beta = "Beta",
  Gamma = "Gamma",
}

export enum OrderStatus {
  ORDERED = "ORDERED",
  SHIPPED = "SHIPPED",
  DELIVERED = "DELIVERED",
  CANCELLED = "CANCELLED",
}

export enum Tag {
  Security = "Security",
  AI = "AI",
  Blockchain = "Blockchain",
}

export enum TestEnum {
  A = "A",
  B = "B",
  C = "C",
  D = "D",
  E = "E",
  F = "F",
  G = "G",
}

export interface Blah {
  prop4?: string | null
}

export interface ClassOptionalOutput {
  prop1: string
  prop2: string
}

export interface ClassOptionalOutput2 {
  prop1?: string | null
  prop2?: string | null
  prop3?: Blah | null
}

export interface ClassWithImage {
  myImage: Image
  param2: string
  fake_image: FakeImage
}

export interface Education {
  institution: string
  location: string
  degree: string
  major: string[]
  graduation_date?: string | null
}

export interface Email {
  subject: string
  body: string
  from_address: string
}

export interface Event {
  title: string
  date: string
  location: string
  description: string
}

export interface FakeImage {
  url: string
}

export interface NamedArgsSingleClass {
  key: string
  key_two: boolean
  key_three: number
}

export interface OptionalTest_Prop1 {
  omega_a: string
  omega_b: number
}

export interface OptionalTest_ReturnType {
  omega_1?: OptionalTest_Prop1 | null
  omega_2?: string | null
  omega_3: (OptionalTest_CategoryType | null)[]
}

export interface OrderInfo {
  order_status: OrderStatus
  tracking_number?: string | null
  estimated_arrival_date?: string | null
}

export interface Person {
  name?: string | null
  hair_color?: string | null
}

export interface RaysData {
  dataType: DataType
  value: Resume | Event
}

export interface Resume {
  name: string
  email: string
  phone: string
  experience: Education[]
  education: string[]
  skills: string[]
}

export interface SearchParams {
  dateRange?: number | null
  location: string[]
  jobTitle?: WithReasoning | null
  company?: WithReasoning | null
  description: WithReasoning[]
  tags: (Tag | string)[]
}

export interface TestClassAlias {
  key: string
  key2: string
  key3: string
  key4: string
  key5: string
}

export interface TestClassWithEnum {
  prop1: string
  prop2: EnumInClass
}

export interface TestOutputClass {
  prop1: string
  prop2: number
}

export interface TestOutputClassNested {
  prop1: string
  prop2: number
  prop3: TestOutputClass
}

export interface UnionTest_ReturnType {
  prop1: string | boolean
  prop2: (number | boolean)[]
  prop3: number[] | boolean[]
}

export interface WithReasoning {
  value: string
  reasoning: string
}
