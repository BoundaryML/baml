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
import { FieldType } from '@boundaryml/baml/native'
import { TypeBuilder as _TypeBuilder, EnumBuilder, ClassBuilder } from '@boundaryml/baml/type_builder'

export default class TypeBuilder {
    private tb: _TypeBuilder;
    
    DynInputOutput: ClassBuilder<'DynInputOutput', "testKey">;
    
    DynamicClassOne: ClassBuilder<'DynamicClassOne'>;
    
    DynamicClassTwo: ClassBuilder<'DynamicClassTwo', "hi" | "some_class" | "status">;
    
    DynamicOutput: ClassBuilder<'DynamicOutput'>;
    
    Person: ClassBuilder<'Person', "name" | "hair_color">;
    
    SomeClassNestedDynamic: ClassBuilder<'SomeClassNestedDynamic', "hi">;
    
    
    Color: EnumBuilder<'Color', "RED" | "BLUE" | "GREEN" | "YELLOW" | "BLACK" | "WHITE">;
    
    DynEnumOne: EnumBuilder<'DynEnumOne'>;
    
    DynEnumTwo: EnumBuilder<'DynEnumTwo'>;
    
    Hobby: EnumBuilder<'Hobby', "SPORTS" | "MUSIC" | "READING">;
    

    constructor() {
        this.tb = new _TypeBuilder({
          classes: new Set([
            "Actor","ActorSubject","Blah","ClassOptionalOutput","ClassOptionalOutput2","ClassWithImage","DynInputOutput","DynamicClassOne","DynamicClassTwo","DynamicOutput","Education","Email","Event","FakeImage","InnerClass","InnerClass2","NamedArgsSingleClass","OptionalTest_Prop1","OptionalTest_ReturnType","OrderInfo","Party","Person","RaysData","ReceiptInfo","ReceiptItem","Resume","SearchParams","SomeClassNestedDynamic","TestClassAlias","TestClassNested","TestClassWithEnum","TestOutputClass","UnionTest_ReturnType","WithReasoning",
          ]),
          enums: new Set([
            "Category","Category2","Category3","Color","DataType","DynEnumOne","DynEnumTwo","EnumInClass","EnumOutput","Gender","Hobby","NamedArgsSingleEnum","NamedArgsSingleEnumList","OptionalTest_CategoryType","OrderStatus","PartyOfficial","Region","Tag","TestEnum",
          ])
        });
        
        this.DynInputOutput = this.tb.classBuilder("DynInputOutput", [
          "testKey",
        ]);
        
        this.DynamicClassOne = this.tb.classBuilder("DynamicClassOne", [
          
        ]);
        
        this.DynamicClassTwo = this.tb.classBuilder("DynamicClassTwo", [
          "hi","some_class","status",
        ]);
        
        this.DynamicOutput = this.tb.classBuilder("DynamicOutput", [
          
        ]);
        
        this.Person = this.tb.classBuilder("Person", [
          "name","hair_color",
        ]);
        
        this.SomeClassNestedDynamic = this.tb.classBuilder("SomeClassNestedDynamic", [
          "hi",
        ]);
        
        
        this.Color = this.tb.enumBuilder("Color", [
          "RED","BLUE","GREEN","YELLOW","BLACK","WHITE",
        ]);
        
        this.DynEnumOne = this.tb.enumBuilder("DynEnumOne", [
          
        ]);
        
        this.DynEnumTwo = this.tb.enumBuilder("DynEnumTwo", [
          
        ]);
        
        this.Hobby = this.tb.enumBuilder("Hobby", [
          "SPORTS","MUSIC","READING",
        ]);
        
    }

    __tb() {
      return this.tb._tb();
    }
    
    string(): FieldType {
        return this.tb.string()
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

    addClass<Name extends string>(name: Name): ClassBuilder<Name> {
        return this.tb.addClass(name);
    }

    addEnum<Name extends string>(name: Name): EnumBuilder<Name> {
        return this.tb.addEnum(name);
    }
}