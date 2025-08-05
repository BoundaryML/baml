import { ClassBuilder as _ClassBuilder, EnumBuilder as _EnumBuilder, ClassPropertyBuilder as _ClassPropertyBuilder, EnumValueBuilder, FieldType, TypeBuilder as _TypeBuilder, BamlRuntime } from './native';
type IsLiteral<T extends string> = string extends T ? false : true;
type NameOf<T extends string> = IsLiteral<T> extends true ? T : 'DynamicType';
type CheckNever<T, TypeName extends string, Value extends string> = [T] extends [never] ? `Error: Attempt to add value '${Value}' which is already a part of '${NameOf<TypeName>}'.` : T;
type ExcludeFrom<T, U> = T extends U ? never : T;
type RestrictNot<Name extends string, Value extends string, T extends string> = IsLiteral<T> extends true ? CheckNever<ExcludeFrom<Value, T>, Name, Value> : Value;
export declare class TypeBuilder {
    private tb;
    protected classes: Set<string>;
    protected enums: Set<string>;
    protected runtime: BamlRuntime;
    constructor({ classes, enums, runtime }: {
        classes: Set<string>;
        enums: Set<string>;
        runtime: BamlRuntime;
    });
    reset(): void;
    _tb(): _TypeBuilder;
    null(): FieldType;
    string(): FieldType;
    literalString(value: string): FieldType;
    literalInt(value: number): FieldType;
    literalBool(value: boolean): FieldType;
    int(): FieldType;
    float(): FieldType;
    bool(): FieldType;
    list(type: FieldType): FieldType;
    map(keyType: FieldType, valueType: FieldType): FieldType;
    union(types: FieldType[]): FieldType;
    classViewer<Name extends string, Properties extends string>(name: Name, properties: Properties[]): ClassViewer<Name, Properties>;
    classBuilder<Name extends string, Properties extends string>(name: Name, properties: Properties[]): ClassBuilder<Name, Properties>;
    enumViewer<Name extends string, Values extends string>(name: Name, values: Values[]): EnumViewer<Name, Values>;
    enumBuilder<Name extends string, Values extends string>(name: Name, values: Values[]): EnumBuilder<Name, Values>;
    addClass<Name extends string>(name: Name): ClassBuilder<Name>;
    addEnum<Name extends string>(name: Name): EnumBuilder<Name>;
    addBaml(baml: string): void;
}
export declare class ClassAst<ClassName extends string, Properties extends string = string> {
    protected properties: Set<Properties | string>;
    protected bldr: _ClassBuilder;
    constructor(tb: _TypeBuilder, name: ClassName, properties?: Set<Properties | string>);
    listProperties(): Record<string, FieldType | null>;
    type(): FieldType;
}
export declare class ClassViewer<ClassName extends string, Properties extends string = string> extends ClassAst<ClassName, Properties> {
    constructor(tb: _TypeBuilder, name: ClassName, properties?: Set<Properties | string>);
    listProperties(): Array<[string, ClassPropertyViewer]>;
    property(name: string): ClassPropertyViewer;
}
export declare class ClassBuilder<ClassName extends string, Properties extends string = string> extends ClassAst<ClassName, Properties> {
    constructor(tb: _TypeBuilder, name: ClassName, properties?: Set<Properties | string>);
    addProperty<S extends string>(name: RestrictNot<ClassName, S, Properties>, type: FieldType): ClassPropertyBuilder;
    listProperties(): Array<[string, ClassPropertyBuilder]>;
    removeProperty(name: string): void;
    reset(): void;
    property(name: string): ClassPropertyBuilder;
}
declare class ClassPropertyViewer {
    constructor();
}
declare class ClassPropertyBuilder {
    private bldr;
    constructor(bldr: _ClassPropertyBuilder);
    getType(): FieldType;
    setType(type: FieldType): ClassPropertyBuilder;
    alias(alias: string | null): ClassPropertyBuilder;
    description(description: string | null): ClassPropertyBuilder;
}
export declare class EnumAst<EnumName extends string, Values extends string = string> {
    protected values: Set<Values | string>;
    protected bldr: _EnumBuilder;
    constructor(tb: _TypeBuilder, name: EnumName, values?: Set<Values | string>);
    type(): FieldType;
}
export declare class EnumViewer<EnumName extends string, T extends string = string> extends EnumAst<EnumName, T> {
    constructor(tb: _TypeBuilder, name: EnumName, values?: Set<T | string>);
    listValues(): Array<[string, EnumValueViewer]>;
    value(name: string): EnumValueViewer;
}
export declare class EnumValueViewer {
    constructor();
}
export declare class EnumBuilder<EnumName extends string, T extends string = string> extends EnumAst<EnumName, T> {
    constructor(tb: _TypeBuilder, name: EnumName, values?: Set<T | string>);
    addValue<S extends string>(name: RestrictNot<EnumName, S, T>): EnumValueBuilder;
    listValues(): Array<[string, EnumValueBuilder]>;
    value(name: string): EnumValueBuilder;
}
export {};
//# sourceMappingURL=type_builder.d.ts.map