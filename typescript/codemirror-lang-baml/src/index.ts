import { parser } from "./syntax.grammar"
import { LRLanguage, LanguageSupport, indentNodeProp, foldNodeProp, foldInside, delimitedIndent } from "@codemirror/language"
import { styleTags, tags as t } from "@lezer/highlight"
import { completeFromList } from "@codemirror/autocomplete";

export const BAMLLanguage = LRLanguage.define({
  parser: parser.configure({
    props: [
      indentNodeProp.add({
        Application: delimitedIndent({ closing: ")", align: false })
      }),
      foldNodeProp.add({
        ClassDecl: foldInside
      }),
      styleTags({
        "ClassDecl": t.keyword,
        "ClassDecl/IdentifierDecl": t.name,
        "ClassField/IdentifierDecl": t.propertyName,
        "SimpleTypeExpr/IdentifierDecl": t.name,
        PromptExpr: t.string,
        "FieldAttribute/...": t.annotation,
        "FieldAttribute/LiteralDecl": t.string,
        "EnumDecl/IdentifierDecl": t.name,
        "EnumDecl": t.keyword,
        "BlockAttribute/...": t.annotation,
        "BlockAttribute/LiteralDecl": t.string,
        "EnumValueDecl/IdentifierDecl": t.propertyName,
        "MultilineComment": t.comment,
        "FunctionDecl": t.keyword,
        "IdentifierDecl": t.name,

        "LineComment": t.comment,
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
  return new LanguageSupport(BAMLLanguage, [exampleCompletion])
}