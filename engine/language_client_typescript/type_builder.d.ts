import { ClassPropertyBuilder as _ClassPropertyBuilder, EnumValueBuilder, FieldType, TypeBuilder as _TypeBuilder } from './native';
type IsLiteral<T extends string> = string extends T ? false : true;
type NameOf<T extends string> = IsLiteral<T> extends true ? T : 'DynamicType';
type CheckNever<T, TypeName extends string, Value extends string> = [T] extends [never] ? `Error: Attempt to add value '${Value}' which is already a part of '${NameOf<TypeName>}'.` : T;
type ExcludeFrom<T, U> = T extends U ? never : T;
type RestrictNot<Name extends string, Value extends string, T extends string> = IsLiteral<T> extends true ? CheckNever<ExcludeFrom<Value, T>, Name, Value> : Value;
export declare class TypeBuilder {
    protected classes: Set<String>;
    protected enums: Set<String>;
    private tb;
    constructor(classes: Set<String>, enums: Set<String>);
    _tb(): _TypeBuilder;
    string(): FieldType;
    int(): FieldType;
    float(): FieldType;
    bool(): FieldType;
    list(type: FieldType): FieldType;
    classBuilder<Name extends string, Properties extends string>(name: Name, properties: Properties[]): ClassBuilder<Name, Properties>;
    enumBuilder<Name extends string, T extends string>(name: Name, values: T[]): EnumBuilder<Name, T>;
    addClass<Name extends string>(name: Name): ClassBuilder<Name>;
    addEnum<Name extends string>(name: Name): EnumBuilder<Name>;
}
export declare class ClassBuilder<ClassName extends string, Properties extends string = string> {
    private properties;
    private bldr;
    constructor(tb: _TypeBuilder, name: ClassName, properties?: Set<Properties | string>);
    type(): FieldType;
    listProperties(): Array<[string, ClassPropertyBuilder]>;
    addProperty<S extends string>(name: RestrictNot<ClassName, S, Properties>, type: FieldType): ClassPropertyBuilder;
    property(name: string): ClassPropertyBuilder;
}
declare class ClassPropertyBuilder {
    private bldr;
    constructor(bldr: _ClassPropertyBuilder);
    alias(alias: string | null): ClassPropertyBuilder;
    description(description: string | null): ClassPropertyBuilder;
}
export declare class EnumBuilder<EnumName extends string, T extends string = string> {
    private values;
    private bldr;
    constructor(tb: _TypeBuilder, name: EnumName, values?: Set<T | string>);
    type(): FieldType;
    value<S extends string>(name: S | T): EnumValueBuilder;
    listValues(): Array<[string, EnumValueBuilder]>;
    addValue<S extends string>(name: RestrictNot<EnumName, S, T>): EnumValueBuilder;
}
export {};
//# sourceMappingURL=type_builder.d.ts.map