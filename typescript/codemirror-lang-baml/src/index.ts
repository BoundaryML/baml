import { parser } from "./syntax.grammar"
import { vscodeDarkInit } from '@uiw/codemirror-theme-vscode'
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
        'Decl/*/"{" Decl/*/"}"': t.brace,
        'ValueExpr/"{" ValueExpr/"}"': t.brace,

        "EnumDecl": t.keyword,
        "EnumDecl/IdentifierDecl": t.typeName,
        "EnumDecl/EnumValueDecl/IdentifierDecl": t.propertyName,

        "ClassDecl": t.keyword,
        "ClassDecl/IdentifierDecl": t.typeName,

        "ClientDecl": t.keyword,
        "ClientDecl/IdentifierDecl": t.typeName,

        "FunctionDecl": t.keyword,
        "FunctionDecl/IdentifierDecl": t.function(t.variableName),
        "FunctionArg/IdentifierDecl": t.variableName,

        "ClassField/IdentifierDecl": t.propertyName,
        "NumericLiteral": t.number,
        "QuotedString": t.string,
        "UnquotedString": t.string,
        "AttributeValue/UnquotedAttributeValue": t.string,
        
        'FieldAttribute/@': t.attributeName,
        "FieldAttribute/IdentifierDecl": t.attributeName,
        'BlockAttribute/@@': t.attributeName,
        "BlockAttribute/IdentifierDecl": t.attributeName,

        "SimpleTypeExpr/IdentifierDecl": t.typeName,
        
        "variable": t.controlKeyword,
        
        "TupleValue/IdentifierDecl": t.operator,

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

export const theme = vscodeDarkInit({
  styles: [
    {
      tag: [t.variableName],
      color: '#dcdcaa',
    },
    {
      tag: [t.brace],
      color: '#569cd6',
    },
    {
      tag: [t.variableName, t.propertyName],
      color: '#d4d4d4',
    },
    {
      tag: [t.attributeName],
      color: '#c586c0',
    },
  ]
});

export function BAML() {
  return new LanguageSupport(BAMLLanguage, [exampleCompletion, syntaxHighlighting(classHighlighter)])
}