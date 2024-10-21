import {
  ClassBuilder as _ClassBuilder,
  EnumBuilder as _EnumBuilder,
  ClassPropertyBuilder as _ClassPropertyBuilder,
  EnumValueBuilder,
  FieldType,
  TypeBuilder as _TypeBuilder,
} from './native'

type IsLiteral<T extends string> = string extends T ? false : true
type NameOf<T extends string> = IsLiteral<T> extends true ? T : 'DynamicType'
type CheckNever<T, TypeName extends string, Value extends string> = [T] extends [never]
  ? `Error: Attempt to add value '${Value}' which is already a part of '${NameOf<TypeName>}'.`
  : T
type ExcludeFrom<T, U> = T extends U ? never : T
type RestrictNot<Name extends string, Value extends string, T extends string> = IsLiteral<T> extends true
  ? CheckNever<ExcludeFrom<Value, T>, Name, Value>
  : Value

export class TypeBuilder {
  private tb: _TypeBuilder
  protected classes: Set<string>
  protected enums: Set<string>

  constructor({ classes, enums }: { classes: Set<string>; enums: Set<string> }) {
    this.classes = classes
    this.enums = enums
    this.tb = new _TypeBuilder()
  }

  _tb(): _TypeBuilder {
    return this.tb
  }

  null(): FieldType {
    return this.tb.null()
  }

  string(): FieldType {
    return this.tb.string()
  }

  literalString(value: string): FieldType {
    return this.tb.literalString(value)
  }

  literalInt(value: number): FieldType {
    return this.tb.literalInt(value)
  }

  literalBool(value: boolean): FieldType {
    return this.tb.literalBool(value)
  }

  int(): FieldType {
    return this.tb.int()
  }

  float(): FieldType {
    return this.tb.float()
  }

  bool(): FieldType {
    return this.tb.bool()
  }

  list(type: FieldType): FieldType {
    return this.tb.list(type)
  }

  map(keyType: FieldType, valueType: FieldType): FieldType {
    return this.tb.map(keyType, valueType)
  }

  union(types: FieldType[]): FieldType {
    return this.tb.union(types)
  }

  classBuilder<Name extends string, Properties extends string>(
    name: Name,
    properties: Properties[],
  ): ClassBuilder<Name, Properties> {
    return new ClassBuilder(this.tb, name, new Set(properties))
  }

  enumBuilder<Name extends string, T extends string>(name: Name, values: T[]): EnumBuilder<Name, T> {
    return new EnumBuilder(this.tb, name, new Set(values))
  }

  addClass<Name extends string>(name: Name): ClassBuilder<Name> {
    if (this.classes.has(name)) {
      throw new Error(`Class ${name} already exists`)
    }
    if (this.enums.has(name)) {
      throw new Error(`Enum ${name} already exists`)
    }
    this.classes.add(name)
    return new ClassBuilder(this.tb, name)
  }

  addEnum<Name extends string>(name: Name): EnumBuilder<Name> {
    if (this.classes.has(name)) {
      throw new Error(`Class ${name} already exists`)
    }
    if (this.enums.has(name)) {
      throw new Error(`Enum ${name} already exists`)
    }
    this.enums.add(name)
    return new EnumBuilder(this.tb, name)
  }
}

export class ClassBuilder<ClassName extends string, Properties extends string = string> {
  private bldr: _ClassBuilder

  constructor(
    tb: _TypeBuilder,
    name: ClassName,
    private properties: Set<Properties | string> = new Set(),
  ) {
    this.bldr = tb.getClass(name)
  }

  type(): FieldType {
    return this.bldr.field()
  }

  listProperties(): Array<[string, ClassPropertyBuilder]> {
    return Array.from(this.properties).map((name) => [name, new ClassPropertyBuilder(this.bldr.property(name))])
  }

  addProperty<S extends string>(name: RestrictNot<ClassName, S, Properties>, type: FieldType): ClassPropertyBuilder {
    if (this.properties.has(name)) {
      throw new Error(`Property ${name} already exists.`)
    }
    this.properties.add(name)
    return new ClassPropertyBuilder(this.bldr.property(name).setType(type))
  }

  property(name: string): ClassPropertyBuilder {
    if (!this.properties.has(name)) {
      throw new Error(`Property ${name} not found.`)
    }
    return new ClassPropertyBuilder(this.bldr.property(name))
  }
}

class ClassPropertyBuilder {
  private bldr: _ClassPropertyBuilder

  constructor(bldr: _ClassPropertyBuilder) {
    this.bldr = bldr
  }

  alias(alias: string | null): ClassPropertyBuilder {
    this.bldr.alias(alias)
    return this
  }

  description(description: string | null): ClassPropertyBuilder {
    this.bldr.description(description)
    return this
  }
}

export class EnumBuilder<EnumName extends string, T extends string = string> {
  private bldr: _EnumBuilder

  constructor(
    tb: _TypeBuilder,
    name: EnumName,
    private values: Set<T | string> = new Set(),
  ) {
    this.bldr = tb.getEnum(name)
  }

  type(): FieldType {
    return this.bldr.field()
  }

  value<S extends string>(name: S | T): EnumValueBuilder {
    if (!this.values.has(name)) {
      throw new Error(`Value ${name} not found.`)
    }
    return this.bldr.value(name)
  }

  listValues(): Array<[string, EnumValueBuilder]> {
    return Array.from(this.values).map((name) => [name, this.bldr.value(name)])
  }

  addValue<S extends string>(name: RestrictNot<EnumName, S, T>): EnumValueBuilder {
    if (this.values.has(name)) {
      throw new Error(`Value ${name} already exists.`)
    }
    this.values.add(name)
    return this.bldr.value(name)
  }
}
