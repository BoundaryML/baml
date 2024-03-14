// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck


import { NamedArgsSingleClass, TestClassAlias, NamedArgsSingleEnumList, NamedArgsSingleEnum } from './types';
import { traceAsync, FireBamlEvent } from '@boundaryml/baml-core/ffi_layer';


type IFnTestClassAlias = (arg: string) => Promise<TestClassAlias>

type FnTestClassAliasImpls = 'v1';

interface FnTestClassAliasImpl {
    run: IFnTestClassAlias;
    name: FnTestClassAliasImpls;
}

interface FnTestClassAliasFunction {
  registerImpl: (name: FnTestClassAliasImpls, impl: FnTestClassAliasImpl) => void;
  getImpl: (name: FnTestClassAliasImpls) => FnTestClassAliasImpl;
}

function createFnTestClassAliasInstance(): IFnTestClassAlias & FnTestClassAliasFunction {

  const registry: Record<FnTestClassAliasImpls, FnTestClassAliasImpl> = {}

  const wrapper: FnTestClassAliasFunction = {
    getImpl: (name: FnTestClassAliasImpls) => {
      const impl = registry[name];
      if (!impl) {
        throw new Error(`No implementation for FnTestClassAlias with name ${name}`);
      }
      return impl;
    },
    registerImpl: (name: FnTestClassAliasImpls, cb: IFnTestClassAlias) => {
      if (registry[name]) {
        throw new Error(`Implementation for FnTestClassAlias with name ${name} already exists`);
      }
      registry[name] = {
        name,
        run: traceAsync(
          /* functionName */"FnTestClassAlias",
          /* returnType */ "TestClassAlias",
          /* paramters */ [
            [
              "arg",
              "string"
            ]
          ],
          /* arg_type */ 'positional',
          /* cb */ async (
          arg: string
        ) => {
          FireBamlEvent.variant(name);
          return await cb(arg);
        })
      };
    },
    validate: () => {
      const targets = ['v1'];
      const impls = Object.keys(registry);
      const missing = targets.filter(t => !impls.includes(t));
      if (missing.length > 0) {
        throw new Error(`Missing implementations for FnTestClassAlias: ${missing.join(', ')}`);
      }
    }
  };

  const impl = async (arg: string) => {
    return wrapper.getImpl('v1').run(params);
  };

  Object.assign(impl, wrapper);

  return impl as  IFnTestClassAlias & FnTestClassAliasFunction;
}

const FnTestClassAlias = createFnTestClassAliasInstance();

type ITestFnNamedArgsSingleBool = (args: {
  myBool: boolean
}) => Promise<string>

type TestFnNamedArgsSingleBoolImpls = 'v1';

interface TestFnNamedArgsSingleBoolImpl {
    run: ITestFnNamedArgsSingleBool;
    name: TestFnNamedArgsSingleBoolImpls;
}

interface TestFnNamedArgsSingleBoolFunction {
  registerImpl: (name: TestFnNamedArgsSingleBoolImpls, impl: TestFnNamedArgsSingleBoolImpl) => void;
  getImpl: (name: TestFnNamedArgsSingleBoolImpls) => TestFnNamedArgsSingleBoolImpl;
}

function createTestFnNamedArgsSingleBoolInstance(): ITestFnNamedArgsSingleBool & TestFnNamedArgsSingleBoolFunction {

  const registry: Record<TestFnNamedArgsSingleBoolImpls, TestFnNamedArgsSingleBoolImpl> = {}

  const wrapper: TestFnNamedArgsSingleBoolFunction = {
    getImpl: (name: TestFnNamedArgsSingleBoolImpls) => {
      const impl = registry[name];
      if (!impl) {
        throw new Error(`No implementation for TestFnNamedArgsSingleBool with name ${name}`);
      }
      return impl;
    },
    registerImpl: (name: TestFnNamedArgsSingleBoolImpls, cb: ITestFnNamedArgsSingleBool) => {
      if (registry[name]) {
        throw new Error(`Implementation for TestFnNamedArgsSingleBool with name ${name} already exists`);
      }
      registry[name] = {
        name,
        run: traceAsync(
          /* functionName */"TestFnNamedArgsSingleBool",
          /* returnType */ "string",
          /* paramters */ [
            [
              "myBool",
              "boolean"
            ]
          ],
          /* arg_type */ 'named',
          /* cb */ async (
          params: {
            myBool: boolean
          }
        ) => {
          FireBamlEvent.variant(name);
          return await cb(params);
        })
      };
    },
    validate: () => {
      const targets = ['v1'];
      const impls = Object.keys(registry);
      const missing = targets.filter(t => !impls.includes(t));
      if (missing.length > 0) {
        throw new Error(`Missing implementations for TestFnNamedArgsSingleBool: ${missing.join(', ')}`);
      }
    }
  };

  const impl = async (params : {
    myBool: boolean
  }) => {
    return wrapper.getImpl('v1').run(params);
  };

  Object.assign(impl, wrapper);

  return impl as  ITestFnNamedArgsSingleBool & TestFnNamedArgsSingleBoolFunction;
}

const TestFnNamedArgsSingleBool = createTestFnNamedArgsSingleBoolInstance();

type ITestFnNamedArgsSingleClass = (args: {
  myArg: NamedArgsSingleClass
}) => Promise<string>

type TestFnNamedArgsSingleClassImpls = 'v1';

interface TestFnNamedArgsSingleClassImpl {
    run: ITestFnNamedArgsSingleClass;
    name: TestFnNamedArgsSingleClassImpls;
}

interface TestFnNamedArgsSingleClassFunction {
  registerImpl: (name: TestFnNamedArgsSingleClassImpls, impl: TestFnNamedArgsSingleClassImpl) => void;
  getImpl: (name: TestFnNamedArgsSingleClassImpls) => TestFnNamedArgsSingleClassImpl;
}

function createTestFnNamedArgsSingleClassInstance(): ITestFnNamedArgsSingleClass & TestFnNamedArgsSingleClassFunction {

  const registry: Record<TestFnNamedArgsSingleClassImpls, TestFnNamedArgsSingleClassImpl> = {}

  const wrapper: TestFnNamedArgsSingleClassFunction = {
    getImpl: (name: TestFnNamedArgsSingleClassImpls) => {
      const impl = registry[name];
      if (!impl) {
        throw new Error(`No implementation for TestFnNamedArgsSingleClass with name ${name}`);
      }
      return impl;
    },
    registerImpl: (name: TestFnNamedArgsSingleClassImpls, cb: ITestFnNamedArgsSingleClass) => {
      if (registry[name]) {
        throw new Error(`Implementation for TestFnNamedArgsSingleClass with name ${name} already exists`);
      }
      registry[name] = {
        name,
        run: traceAsync(
          /* functionName */"TestFnNamedArgsSingleClass",
          /* returnType */ "string",
          /* paramters */ [
            [
              "myArg",
              "NamedArgsSingleClass"
            ]
          ],
          /* arg_type */ 'named',
          /* cb */ async (
          params: {
            myArg: NamedArgsSingleClass
          }
        ) => {
          FireBamlEvent.variant(name);
          return await cb(params);
        })
      };
    },
    validate: () => {
      const targets = ['v1'];
      const impls = Object.keys(registry);
      const missing = targets.filter(t => !impls.includes(t));
      if (missing.length > 0) {
        throw new Error(`Missing implementations for TestFnNamedArgsSingleClass: ${missing.join(', ')}`);
      }
    }
  };

  const impl = async (params : {
    myArg: NamedArgsSingleClass
  }) => {
    return wrapper.getImpl('v1').run(params);
  };

  Object.assign(impl, wrapper);

  return impl as  ITestFnNamedArgsSingleClass & TestFnNamedArgsSingleClassFunction;
}

const TestFnNamedArgsSingleClass = createTestFnNamedArgsSingleClassInstance();

type ITestFnNamedArgsSingleEnum = (args: {
  myArg: NamedArgsSingleEnum
}) => Promise<string>

type TestFnNamedArgsSingleEnumImpls = 'v1';

interface TestFnNamedArgsSingleEnumImpl {
    run: ITestFnNamedArgsSingleEnum;
    name: TestFnNamedArgsSingleEnumImpls;
}

interface TestFnNamedArgsSingleEnumFunction {
  registerImpl: (name: TestFnNamedArgsSingleEnumImpls, impl: TestFnNamedArgsSingleEnumImpl) => void;
  getImpl: (name: TestFnNamedArgsSingleEnumImpls) => TestFnNamedArgsSingleEnumImpl;
}

function createTestFnNamedArgsSingleEnumInstance(): ITestFnNamedArgsSingleEnum & TestFnNamedArgsSingleEnumFunction {

  const registry: Record<TestFnNamedArgsSingleEnumImpls, TestFnNamedArgsSingleEnumImpl> = {}

  const wrapper: TestFnNamedArgsSingleEnumFunction = {
    getImpl: (name: TestFnNamedArgsSingleEnumImpls) => {
      const impl = registry[name];
      if (!impl) {
        throw new Error(`No implementation for TestFnNamedArgsSingleEnum with name ${name}`);
      }
      return impl;
    },
    registerImpl: (name: TestFnNamedArgsSingleEnumImpls, cb: ITestFnNamedArgsSingleEnum) => {
      if (registry[name]) {
        throw new Error(`Implementation for TestFnNamedArgsSingleEnum with name ${name} already exists`);
      }
      registry[name] = {
        name,
        run: traceAsync(
          /* functionName */"TestFnNamedArgsSingleEnum",
          /* returnType */ "string",
          /* paramters */ [
            [
              "myArg",
              "NamedArgsSingleEnum"
            ]
          ],
          /* arg_type */ 'named',
          /* cb */ async (
          params: {
            myArg: NamedArgsSingleEnum
          }
        ) => {
          FireBamlEvent.variant(name);
          return await cb(params);
        })
      };
    },
    validate: () => {
      const targets = ['v1'];
      const impls = Object.keys(registry);
      const missing = targets.filter(t => !impls.includes(t));
      if (missing.length > 0) {
        throw new Error(`Missing implementations for TestFnNamedArgsSingleEnum: ${missing.join(', ')}`);
      }
    }
  };

  const impl = async (params : {
    myArg: NamedArgsSingleEnum
  }) => {
    return wrapper.getImpl('v1').run(params);
  };

  Object.assign(impl, wrapper);

  return impl as  ITestFnNamedArgsSingleEnum & TestFnNamedArgsSingleEnumFunction;
}

const TestFnNamedArgsSingleEnum = createTestFnNamedArgsSingleEnumInstance();

type ITestFnNamedArgsSingleEnumList = (args: {
  myArg: NamedArgsSingleEnumList[]
}) => Promise<string>

type TestFnNamedArgsSingleEnumListImpls = 'v1';

interface TestFnNamedArgsSingleEnumListImpl {
    run: ITestFnNamedArgsSingleEnumList;
    name: TestFnNamedArgsSingleEnumListImpls;
}

interface TestFnNamedArgsSingleEnumListFunction {
  registerImpl: (name: TestFnNamedArgsSingleEnumListImpls, impl: TestFnNamedArgsSingleEnumListImpl) => void;
  getImpl: (name: TestFnNamedArgsSingleEnumListImpls) => TestFnNamedArgsSingleEnumListImpl;
}

function createTestFnNamedArgsSingleEnumListInstance(): ITestFnNamedArgsSingleEnumList & TestFnNamedArgsSingleEnumListFunction {

  const registry: Record<TestFnNamedArgsSingleEnumListImpls, TestFnNamedArgsSingleEnumListImpl> = {}

  const wrapper: TestFnNamedArgsSingleEnumListFunction = {
    getImpl: (name: TestFnNamedArgsSingleEnumListImpls) => {
      const impl = registry[name];
      if (!impl) {
        throw new Error(`No implementation for TestFnNamedArgsSingleEnumList with name ${name}`);
      }
      return impl;
    },
    registerImpl: (name: TestFnNamedArgsSingleEnumListImpls, cb: ITestFnNamedArgsSingleEnumList) => {
      if (registry[name]) {
        throw new Error(`Implementation for TestFnNamedArgsSingleEnumList with name ${name} already exists`);
      }
      registry[name] = {
        name,
        run: traceAsync(
          /* functionName */"TestFnNamedArgsSingleEnumList",
          /* returnType */ "string",
          /* paramters */ [
            [
              "myArg",
              "NamedArgsSingleEnumList[]"
            ]
          ],
          /* arg_type */ 'named',
          /* cb */ async (
          params: {
            myArg: NamedArgsSingleEnumList[]
          }
        ) => {
          FireBamlEvent.variant(name);
          return await cb(params);
        })
      };
    },
    validate: () => {
      const targets = ['v1'];
      const impls = Object.keys(registry);
      const missing = targets.filter(t => !impls.includes(t));
      if (missing.length > 0) {
        throw new Error(`Missing implementations for TestFnNamedArgsSingleEnumList: ${missing.join(', ')}`);
      }
    }
  };

  const impl = async (params : {
    myArg: NamedArgsSingleEnumList[]
  }) => {
    return wrapper.getImpl('v1').run(params);
  };

  Object.assign(impl, wrapper);

  return impl as  ITestFnNamedArgsSingleEnumList & TestFnNamedArgsSingleEnumListFunction;
}

const TestFnNamedArgsSingleEnumList = createTestFnNamedArgsSingleEnumListInstance();

type ITestFnNamedArgsSingleStringList = (args: {
  myArg: string[]
}) => Promise<string>

type TestFnNamedArgsSingleStringListImpls = 'v1';

interface TestFnNamedArgsSingleStringListImpl {
    run: ITestFnNamedArgsSingleStringList;
    name: TestFnNamedArgsSingleStringListImpls;
}

interface TestFnNamedArgsSingleStringListFunction {
  registerImpl: (name: TestFnNamedArgsSingleStringListImpls, impl: TestFnNamedArgsSingleStringListImpl) => void;
  getImpl: (name: TestFnNamedArgsSingleStringListImpls) => TestFnNamedArgsSingleStringListImpl;
}

function createTestFnNamedArgsSingleStringListInstance(): ITestFnNamedArgsSingleStringList & TestFnNamedArgsSingleStringListFunction {

  const registry: Record<TestFnNamedArgsSingleStringListImpls, TestFnNamedArgsSingleStringListImpl> = {}

  const wrapper: TestFnNamedArgsSingleStringListFunction = {
    getImpl: (name: TestFnNamedArgsSingleStringListImpls) => {
      const impl = registry[name];
      if (!impl) {
        throw new Error(`No implementation for TestFnNamedArgsSingleStringList with name ${name}`);
      }
      return impl;
    },
    registerImpl: (name: TestFnNamedArgsSingleStringListImpls, cb: ITestFnNamedArgsSingleStringList) => {
      if (registry[name]) {
        throw new Error(`Implementation for TestFnNamedArgsSingleStringList with name ${name} already exists`);
      }
      registry[name] = {
        name,
        run: traceAsync(
          /* functionName */"TestFnNamedArgsSingleStringList",
          /* returnType */ "string",
          /* paramters */ [
            [
              "myArg",
              "string[]"
            ]
          ],
          /* arg_type */ 'named',
          /* cb */ async (
          params: {
            myArg: string[]
          }
        ) => {
          FireBamlEvent.variant(name);
          return await cb(params);
        })
      };
    },
    validate: () => {
      const targets = ['v1'];
      const impls = Object.keys(registry);
      const missing = targets.filter(t => !impls.includes(t));
      if (missing.length > 0) {
        throw new Error(`Missing implementations for TestFnNamedArgsSingleStringList: ${missing.join(', ')}`);
      }
    }
  };

  const impl = async (params : {
    myArg: string[]
  }) => {
    return wrapper.getImpl('v1').run(params);
  };

  Object.assign(impl, wrapper);

  return impl as  ITestFnNamedArgsSingleStringList & TestFnNamedArgsSingleStringListFunction;
}

const TestFnNamedArgsSingleStringList = createTestFnNamedArgsSingleStringListInstance();

type ITestFnNamedArgsSyntax = (args: {
  var: string, var_with_underscores: string
}) => Promise<string>

type TestFnNamedArgsSyntaxImpls = never;

interface TestFnNamedArgsSyntaxImpl {
    run: ITestFnNamedArgsSyntax;
    name: TestFnNamedArgsSyntaxImpls;
}

interface TestFnNamedArgsSyntaxFunction {
  registerImpl: (name: TestFnNamedArgsSyntaxImpls, impl: TestFnNamedArgsSyntaxImpl) => void;
  getImpl: (name: TestFnNamedArgsSyntaxImpls) => TestFnNamedArgsSyntaxImpl;
}

function createTestFnNamedArgsSyntaxInstance(): ITestFnNamedArgsSyntax & TestFnNamedArgsSyntaxFunction {

  const registry: Record<TestFnNamedArgsSyntaxImpls, TestFnNamedArgsSyntaxImpl> = {}

  const wrapper: TestFnNamedArgsSyntaxFunction = {
    getImpl: (name: TestFnNamedArgsSyntaxImpls) => {
      const impl = registry[name];
      if (!impl) {
        throw new Error(`No implementation for TestFnNamedArgsSyntax with name ${name}`);
      }
      return impl;
    },
    registerImpl: (name: TestFnNamedArgsSyntaxImpls, cb: ITestFnNamedArgsSyntax) => {
      if (registry[name]) {
        throw new Error(`Implementation for TestFnNamedArgsSyntax with name ${name} already exists`);
      }
      registry[name] = {
        name,
        run: traceAsync(
          /* functionName */"TestFnNamedArgsSyntax",
          /* returnType */ "string",
          /* paramters */ [
            [
              "var",
              "string"
            ],
            [
              "var_with_underscores",
              "string"
            ]
          ],
          /* arg_type */ 'named',
          /* cb */ async (
          params: {
            var: string, var_with_underscores: string
          }
        ) => {
          FireBamlEvent.variant(name);
          return await cb(params);
        })
      };
    },
    validate: () => {
    }
  };

  const impl = async (params : {
    var: string, var_with_underscores: string
  }) => {
    throw new Error('No implementation for TestFnNamedArgsSyntax');
  };

  Object.assign(impl, wrapper);

  return impl as  ITestFnNamedArgsSyntax & TestFnNamedArgsSyntaxFunction;
}

const TestFnNamedArgsSyntax = createTestFnNamedArgsSyntaxInstance();


export { FnTestClassAlias, IFnTestClassAlias, FnTestClassAliasFunction, TestFnNamedArgsSingleBool, ITestFnNamedArgsSingleBool, TestFnNamedArgsSingleBoolFunction, TestFnNamedArgsSingleClass, ITestFnNamedArgsSingleClass, TestFnNamedArgsSingleClassFunction, TestFnNamedArgsSingleEnum, ITestFnNamedArgsSingleEnum, TestFnNamedArgsSingleEnumFunction, TestFnNamedArgsSingleEnumList, ITestFnNamedArgsSingleEnumList, TestFnNamedArgsSingleEnumListFunction, TestFnNamedArgsSingleStringList, ITestFnNamedArgsSingleStringList, TestFnNamedArgsSingleStringListFunction, TestFnNamedArgsSyntax, ITestFnNamedArgsSyntax, TestFnNamedArgsSyntaxFunction }