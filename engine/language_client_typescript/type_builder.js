"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.EnumBuilder = exports.ClassBuilder = exports.TypeBuilder = void 0;
const native_1 = require("./native");
class TypeBuilder {
    tb;
    classes;
    enums;
    constructor({ classes, enums }) {
        this.classes = classes;
        this.enums = enums;
        this.tb = new native_1.TypeBuilder();
    }
    _tb() {
        return this.tb;
    }
    string() {
        return this.tb.string();
    }
    int() {
        return this.tb.int();
    }
    float() {
        return this.tb.float();
    }
    bool() {
        return this.tb.bool();
    }
    list(type) {
        return this.tb.list(type);
    }
    classBuilder(name, properties) {
        return new ClassBuilder(this.tb, name, new Set(properties));
    }
    enumBuilder(name, values) {
        return new EnumBuilder(this.tb, name, new Set(values));
    }
    addClass(name) {
        if (this.classes.has(name)) {
            throw new Error(`Class ${name} already exists`);
        }
        if (this.enums.has(name)) {
            throw new Error(`Enum ${name} already exists`);
        }
        this.classes.add(name);
        return new ClassBuilder(this.tb, name);
    }
    addEnum(name) {
        if (this.classes.has(name)) {
            throw new Error(`Class ${name} already exists`);
        }
        if (this.enums.has(name)) {
            throw new Error(`Enum ${name} already exists`);
        }
        this.enums.add(name);
        return new EnumBuilder(this.tb, name);
    }
}
exports.TypeBuilder = TypeBuilder;
class ClassBuilder {
    properties;
    bldr;
    constructor(tb, name, properties = new Set()) {
        this.properties = properties;
        this.bldr = tb.getClass(name);
    }
    type() {
        return this.bldr.field();
    }
    listProperties() {
        return Array.from(this.properties).map((name) => [name, new ClassPropertyBuilder(this.bldr.property(name))]);
    }
    addProperty(name, type) {
        if (this.properties.has(name)) {
            throw new Error(`Property ${name} already exists.`);
        }
        this.properties.add(name);
        return new ClassPropertyBuilder(this.bldr.property(name).setType(type));
    }
    property(name) {
        if (!this.properties.has(name)) {
            throw new Error(`Property ${name} not found.`);
        }
        return new ClassPropertyBuilder(this.bldr.property(name));
    }
}
exports.ClassBuilder = ClassBuilder;
class ClassPropertyBuilder {
    bldr;
    constructor(bldr) {
        this.bldr = bldr;
    }
    alias(alias) {
        this.bldr.alias(alias);
        return this;
    }
    description(description) {
        this.bldr.description(description);
        return this;
    }
}
class EnumBuilder {
    values;
    bldr;
    constructor(tb, name, values = new Set()) {
        this.values = values;
        this.bldr = tb.getEnum(name);
    }
    type() {
        return this.bldr.field();
    }
    value(name) {
        if (!this.values.has(name)) {
            throw new Error(`Value ${name} not found.`);
        }
        return this.bldr.value(name);
    }
    listValues() {
        return Array.from(this.values).map((name) => [name, this.bldr.value(name)]);
    }
    addValue(name) {
        if (this.values.has(name)) {
            throw new Error(`Value ${name} already exists.`);
        }
        this.values.add(name);
        return this.bldr.value(name);
    }
}
exports.EnumBuilder = EnumBuilder;
