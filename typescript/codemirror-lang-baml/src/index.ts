import { parser } from "./syntax.grammar"
import { LRLanguage, LanguageSupport, StreamLanguage, indentNodeProp, foldNodeProp, foldInside, delimitedIndent, syntaxHighlighting } from "@codemirror/language"
import { styleTags, tags as t, tagHighlighter } from "@lezer/highlight"
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
        //"EnumDecl": t.keyword,
        "EnumValueDecl/IdentifierDecl": t.literal,
        "EnumDecl": t.labelName,
        "ClassDecl": t.keyword,
        //"ClassDecl/IdentifierDecl": t.name,
        //"ClassField/IdentifierDecl": t.propertyName,
        //"SimpleTypeExpr/IdentifierDecl": t.name,
        //"PromptExpr": t.string,
        "FieldAttribute/IdentifierDecl": t.attributeName,
        "AttributeValue/...": t.attributeValue,
        //"FieldAttribute/LiteralDecl": t.string,
        //"EnumDecl/IdentifierDecl": t.name,
        //"EnumDecl": t.keyword,
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
  return new LanguageSupport(BAMLLanguage, [exampleCompletion, syntaxHighlighting(tagHighlighter(
    [
      {
        tag: t.attributeName,
        class: "text-green-500",
      },
      {
        tag: t.attributeValue,
        class: "text-green-800",
      },
      {
        tag: t.keyword,
        class: "text-green-500",
      },
      {
        tag: t.labelName,
        class: "text-red-500",
      }
    ]
  ))])
}