// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck


import { NamedArgsSingleEnum, TestClassAlias, NamedArgsSingleClass, NamedArgsSingleEnumList } from './types';


// Function to check if a value is a member of the NamedArgsSingleEnum enum
function isNamedArgsSingleEnum(value: any): value is NamedArgsSingleEnum {
  return Object.values(NamedArgsSingleEnum).includes(value);
}

// Function to check if a value is a member of the NamedArgsSingleEnumList enum
function isNamedArgsSingleEnumList(value: any): value is NamedArgsSingleEnumList {
  return Object.values(NamedArgsSingleEnumList).includes(value);
}

// Function to validate if an object is a NamedArgsSingleClass object
function isNamedArgsSingleClass(obj: any): obj is NamedArgsSingleClass {
  return (
    obj &&
    typeof obj === "object"
    && ("key" in obj && (typeof obj.key === 'string'))
    && ("key_two" in obj && (typeof obj.key_two === 'boolean'))
    && ("key_three" in obj && (typeof obj.key_three === 'number'))
  );
}


class InternalNamedArgsSingleClass implements NamedArgsSingleClass {
  private constructor(private data: {
    key: string,
    key_two: boolean,
    key_three: number,
  }, private raw: NamedArgsSingleClass) {}

  static from(data: NamedArgsSingleClass): InternalNamedArgsSingleClass {
    return new InternalNamedArgsSingleClass({
      key: data.key,
      key_two: data.key_two,
      key_three: data.key_three,
    }, data);
  }

  get key(): string {
    return this.data.key;
  }
  get key_two(): boolean {
    return this.data.key_two;
  }
  get key_three(): number {
    return this.data.key_three;
  }


  toJSON(): string {
    return JSON.stringify(this.raw, null, 2);
  }
}

// Function to validate if an object is a TestClassAlias object
function isTestClassAlias(obj: any): obj is TestClassAlias {
  return (
    obj &&
    typeof obj === "object"
    && ("key" in obj && (typeof obj.key === 'string'))
    && ("key2" in obj && (typeof obj.key2 === 'string'))
    && ("key3" in obj && (typeof obj.key3 === 'string'))
    && ("key4" in obj && (typeof obj.key4 === 'string'))
  );
}


class InternalTestClassAlias implements TestClassAlias {
  private constructor(private data: {
    key: string,
    key2: string,
    key3: string,
    key4: string,
  }, private raw: TestClassAlias) {}

  static from(data: TestClassAlias): InternalTestClassAlias {
    return new InternalTestClassAlias({
      key: data.key,
      key2: data.key2,
      key3: data.key3,
      key4: data.key4,
    }, data);
  }

  get key(): string {
    return this.data.key;
  }
  get key2(): string {
    return this.data.key2;
  }
  get key3(): string {
    return this.data.key3;
  }
  get key4(): string {
    return this.data.key4;
  }


  toJSON(): string {
    return JSON.stringify(this.raw, null, 2);
  }
}


export { InternalNamedArgsSingleClass, InternalTestClassAlias }