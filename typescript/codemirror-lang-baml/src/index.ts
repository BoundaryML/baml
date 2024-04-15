import { parser } from "./syntax.grammar"
import { LRLanguage, LanguageSupport, StreamLanguage, indentNodeProp, foldNodeProp, foldInside, delimitedIndent, syntaxHighlighting } from "@codemirror/language"
import { classHighlighter, styleTags, tags as t, tagHighlighter } from "@lezer/highlight"
import { completeFromList } from "@codemirror/autocomplete";
import { jinja2 } from "@codemirror/legacy-modes/mode/jinja2";
import { parseMixed } from "@lezer/common";

export const BAMLLanguage = LRLanguage.define({
  parser: parser.configure({
    wrap: parseMixed(node => {
      if (node.name !== "PromptExprContents") {
        return null
      }

      return {
        parser: StreamLanguage.define(jinja2).parser,
      }

    }),
    props: [
      indentNodeProp.add({
        Application: delimitedIndent({ closing: ")", align: false })
      }),
      foldNodeProp.add({
        ClassDecl: foldInside
      }),
      styleTags({
        "EnumDecl": t.keyword,
        "EnumDecl/IdentifierDecl": t.typeName,

        "ClassDecl": t.keyword,
        "ClassDecl/IdentifierDecl": t.typeName,

        "ClientDecl": t.keyword,
        "ClientDecl/IdentifierDecl": t.typeName,

        "FunctionDecl": t.keyword,
        "FunctionDecl/IdentifierDecl": t.typeName,

        "ClassField/IdentifierDecl": t.propertyName,
        "NumericLiteral": t.number,
        "QuotedString": t.string,
        "UnquotedString": t.string,
        "AttributeValue/UnquotedAttributeValue": t.string,
        
        "FieldAttribute/IdentifierDecl": t.operator,
        "BlockAttribute/IdentifierDecl": t.operator,

        "SimpleTypeExpr/IdentifierDecl": t.typeName,

        //"ClassDecl/IdentifierDecl": t.name,
        //"ClassField/IdentifierDecl": t.propertyName,
        //"SimpleTypeExpr/IdentifierDecl": t.name,
        //"PromptExpr": t.string,
        //"FieldAttribute/LiteralDecl": t.string,
        //"EnumDecl/IdentifierDecl": t.name,
        //"BlockAttribute/...": t.annotation,
        //"BlockAttribute/LiteralDecl": t.string,
        //"EnumValueDecl/IdentifierDecl": t.propertyName,
        //"MultilineComment": t.comment,
        //"FunctionDecl": t.keyword,
        //"IdentifierDecl": t.name,
        //"ClientDecl/...": t.keyword,
        //"ClientDecl": t.keyword,
        //"LineComment": t.comment,
        //"AttributeValue/UnquotedAttributeValue": t.literal,
        //"BlockAttribute/..": t.keyword,
        //"QuotedString": t.literal,
        //"UnquotedString": t.literal,
        //"NumericLiteral": t.literal,
        //"LiteralDecl": t.literal,
        //"EnumValueDecl/IdentifierDecl": t.literal,
        //"EnumDecl": t.labelName,
        //"ClassDecl": t.keyword,
        //"FieldAttribute/IdentifierDecl": t.attributeName,
        //"AttributeValue/...": t.attributeValue,
        "TrailingComment": t.comment,
        "MultilineComment": t.comment,
      })
    ]
  }),
  languageData: {
    commentTokens: { line: "//" }
  }
})


const exampleCompletion = BAMLLanguage.data.of({
  autocomplete: completeFromList([
    { label: "class", type: "keyword" },
  ])
})

export function BAML() {
  return new LanguageSupport(BAMLLanguage, [exampleCompletion, syntaxHighlighting(classHighlighter)])
}