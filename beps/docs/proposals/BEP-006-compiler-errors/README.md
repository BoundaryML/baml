---
id: BEP-006
title: "compiler-errors"
shepherds: Greg Hale <greg@boundary.com>
status: Draft
created: 2025-12-11
---

# BEP-006: compiler-errors

## Summary

Uniform user-facing experience for compiler errors.

 - Enumerate all errors
 - Give them unique 4-digit codes
 - Specify how each is rendered as a diagnostic message
 - Specify how each is turned into guidance

## Motivation

We want to give users and coding agents a uniform experience when they encounter
compiler errors.

## Proposed Design

 - A top-level `compiler_error` module contains every error category.
   ```
   pub enum CompilerError<Ty> {
     ParseError(ParseError)
     TypeError(TypeError<Ty>)
   }
   ```
   (TypeError is parameterized because it's defined here, it needs to contain
   types, types are defined in another crate that depends on errors, and using
   types directly would lead to a circular crate dependency).
 - `compiler_error` has submodules errors pertinent to specific phases.
   For example type errors are defined in `compiler_error::type_error::TypeError`.
   ```
   pub enum TypeError<T> {
     TypeMismatch { expected: T, found: T, span: Span },
     UnknownType { name: String, span: Span }.
     ...
   }
   ```
 - `compiler_error::error_format` contains `error_report_and_code()`, which does
   a giant match over every error variant and specifies how that variant becomes
   an `ariadne` report builder and an error code. `render_error()` takes these
   two pieces of data and combines them into an `ariadne` Report.
   
This design is implemented and merged already, see
[PR 27651](https://github.com/BoundaryML/baml/pull/2751).

### All the errors

The following is a list of every possible error. It is derived from both
the new compiler (which follows the new scheme), and the legacy compiler
(which has a more exhaustive set of errors due to its longer time in
development).

Phase: compiler phase (Lexing, Parsing, NameResolution, Typing, Codegen)
Error: very brief description in the form of an Enum Variant

| Code   | Phase          | Error                          | Spans                                                      | Notes |
|--------|----------------|--------------------------------|------------------------------------------------------------|-------|
| E0001  | Typing         | TypeMismatch                   | (1) error location, (2) type constraint origin             | Expected X, found Y |
| E0002  | Typing         | UnknownType                    | type reference span                                        | Type name not found in scope |
| E0003  | Typing         | UnknownVariable                | variable reference span                                    | Variable name not found in scope |
| E0004  | Typing         | InvalidOperator                | operator span                                              | Binary or unary op invalid for types |
| E0005  | Typing         | ArgumentCountMismatch          | (1) call site, (2) function definition                     | Wrong number of args to function |
| E0006  | Typing         | NotCallable                    | call site span                                             | Calling a non-function type |
| E0007  | Typing         | NoSuchField                    | (1) field access span, (2) type definition                 | Field doesn't exist on type |
| E0008  | Typing         | NotIndexable                   | index access span                                          | Type doesn't support indexing |
| E0009  | Parsing        | UnexpectedEof                  | EOF location span                                          | Unexpected end of file |
| E0010  | Parsing        | UnexpectedToken                | token span                                                 | Expected X, found Y |
| E0011  | NameResolution | DuplicateName                  | (1) second definition, (2) first definition                | Same name defined twice |
| E0012  | Parsing        | LiteralParserError             | literal span                                               | Invalid literal value |
| E0013  | Validation     | ArgumentNotFound               | argument usage span                                        | Required argument missing |
| E0014  | Validation     | AttributeArgumentNotFound      | attribute span                                             | Attribute missing required arg |
| E0015  | Validation     | GeneratorArgumentNotFound      | generator block span                                       | Generator missing required arg |
| E0016  | Validation     | AttributeValidationError       | attribute span                                             | Attribute parsing failed |
| E0017  | Validation     | DuplicateAttribute             | attribute span                                             | Attribute defined multiple times |
| E0018  | Typing         | IncompatibleNativeType         | native type annotation span                                | Native type incompatible |
| E0019  | Typing         | InvalidNativeTypeArgument      | native type argument span                                  | Invalid arg for native type |
| E0020  | Typing         | InvalidNativeTypePrefix        | native type prefix span                                    | Wrong prefix for native type |
| E0021  | Validation     | NativeTypesNotSupported        | native type span                                           | Connector doesn't support native types |
| E0022  | Typing         | ReservedScalarType             | type name span                                             | Using reserved type name |
| E0023  | NameResolution | DuplicateEnumDatabaseName      | enum span                                                  | Duplicate DB name for enum |
| E0024  | NameResolution | DuplicateModelDatabaseName     | (1) new model span, (2) existing model span                | Duplicate DB name for model |
| E0025  | NameResolution | DuplicateViewDatabaseName      | (1) new view span, (2) existing view span                  | Duplicate DB name for view |
| E0026  | NameResolution | DuplicateTest                  | (1) test span, (2+) other test spans                       | Test name already defined |
| E0027  | NameResolution | DuplicateTopLevel              | (1) new definition, (2+) existing definitions              | Top-level name collision |
| E0028  | NameResolution | DuplicateConfigKey             | config key span                                            | Key already defined in config |
| E0029  | Validation     | DuplicateArgument              | argument span                                              | Argument specified twice |
| E0030  | Validation     | UnusedArgument                 | argument span                                              | No such argument exists |
| E0031  | Validation     | DuplicateDefaultArgument       | argument span                                              | Default arg already specified |
| E0032  | NameResolution | DuplicateFunction              | function span                                              | Function already defined |
| E0033  | Parsing        | InvalidFunctionSyntax          | function span                                              | Malformed function definition |
| E0034  | NameResolution | DuplicateEnumValue             | (1) new value span, (2) existing value span                | Enum value already defined |
| E0035  | NameResolution | DuplicateCompositeTypeField    | (1) new field span, (2) existing field span                | Field already on composite type |
| E0036  | NameResolution | DuplicateField                 | (1) new field span, (2) existing field span                | Field already on model/class |
| E0037  | Validation     | ScalarListFieldsNotSupported   | field span                                                 | Connector doesn't support scalar lists |
| E0038  | Validation     | ModelValidationError           | model span                                                 | Generic model validation failure |
| E0039  | Validation     | NameError                      | name span                                                  | Invalid identifier name |
| E0040  | Validation     | EnumValidationError            | enum span                                                  | Generic enum validation failure |
| E0041  | Validation     | CompositeTypeFieldValidation   | field span                                                 | Composite type field validation |
| E0042  | Validation     | FieldValidationError           | field span                                                 | Generic field validation failure |
| E0043  | Validation     | SourceValidationError          | datasource span                                            | Datasource validation failure |
| E0044  | Validation     | DynamicTypeNotAllowed          | @dynamic attribute span                                    | @dynamic not allowed in type_builder |
| E0045  | Validation     | ValidationError                | relevant span                                              | Generic validation error |
| E0046  | Parsing        | LegacyParserError              | token span                                                 | Legacy parser catch-all |
| E0047  | Typing         | OptionalArgumentCountMismatch  | native type span                                           | Wrong optional arg count |
| E0048  | Parsing        | ParserError                    | token span                                                 | Expected one of: X, Y, Z |
| E0049  | Typing         | FunctionalEvaluationError      | expression span                                            | Error evaluating expression |
| E0050  | NameResolution | NotFoundError                  | reference span                                             | Generic not-found with suggestions |
| E0051  | Typing         | TypeNotUsedInPrompt            | type reference span                                        | Type not in function output |
| E0052  | NameResolution | ClientNotFound                 | client reference span                                      | Client name not found |
| E0053  | NameResolution | TypeNotFound                   | type reference span                                        | Type name not found |
| E0054  | Validation     | AttributeNotKnown              | attribute span                                             | Unknown attribute name |
| E0055  | Validation     | PropertyNotKnown               | property span                                              | Unknown property in block |
| E0056  | Validation     | ArgumentNotKnown               | argument span                                              | Unknown argument name |
| E0057  | Typing         | ValueParserError               | value span                                                 | Expected type X, found Y |
| E0058  | Typing         | TypeMismatchLegacy             | value span                                                 | Legacy type mismatch error |
| E0059  | Validation     | MissingRequiredProperty        | block span                                                 | Required property missing |
| E0060  | Validation     | ConfigPropertyMissingValue     | property span                                              | Property needs a value |
| E0061  | Typing         | TypeNotAllowedAsMapKey         | map key span                                               | Invalid map key type |

## Appendix: Runtime Errors (VM)

These errors occur during execution, not compilation. They are out of scope for this BEP.

| Code   | Category       | Error                          | Notes |
|--------|----------------|--------------------------------|-------|
| R0001  | Runtime        | StackOverflow                  | Call stack exceeded |
| R0002  | Runtime        | AssertionError                 | User assertion failed |
| R0003  | Runtime        | NoSuchKeyInMap                 | Map key not found |
| R0004  | Runtime        | DivisionByZero                 | Division by zero |
| R0005  | Internal       | InvalidArgumentCount           | Wrong arg count (VM bug) |
| R0006  | Internal       | UnexpectedEmptyStack           | Stack underflow (VM bug) |
| R0007  | Internal       | NotEnoughItemsOnStack          | Stack depth error (VM bug) |
| R0008  | Internal       | InvalidObjectRef               | Bad object reference (VM bug) |
| R0009  | Internal       | TypeError                      | VM type error (VM bug) |
| R0010  | Internal       | CannotApplyBinOp               | Invalid binary op (VM bug) |
| R0011  | Internal       | CannotApplyCmpOp               | Invalid comparison (VM bug) |
| R0012  | Internal       | CannotApplyUnaryOp             | Invalid unary op (VM bug) |
| R0013  | Runtime        | ArrayIndexOutOfBounds          | Array index too large |
| R0014  | Runtime        | ArrayIndexIsNegative           | Negative array index |
| R0015  | Internal       | NegativeInstructionPtr         | Bad instruction ptr (VM bug) |
