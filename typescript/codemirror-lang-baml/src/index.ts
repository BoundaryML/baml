import { parser } from "./syntax.grammar"
import { vscodeDarkInit } from '@uiw/codemirror-theme-vscode'
import { LRLanguage, LanguageSupport, StreamLanguage, indentNodeProp, foldNodeProp, foldInside, delimitedIndent, syntaxHighlighting, continuedIndent, indentOnInput } from "@codemirror/language"
import { classHighlighter, styleTags, tags as t, tagHighlighter } from "@lezer/highlight"
import { closeBrackets, completeFromList, snippetCompletion } from "@codemirror/autocomplete";
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
        Decl: delimitedIndent({ closing: "}", align: true }),
        // not sure the except is doing anything.
        "PromptExpr": delimitedIndent({ closing: "#\"", align: true }),

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
        'FieldAttribute': t.attributeName,

        'FieldAttribute/@': t.attributeName,
        "FieldAttribute/IdentifierDecl": t.attributeName,
        'BlockAttribute/@@': t.attributeName,
        "BlockAttribute/IdentifierDecl": t.attributeName,

        "SimpleTypeExpr/IdentifierDecl": t.typeName,

        "variable": t.controlKeyword,
        "PromptExpr": t.string,
        'PromptExprNonJinja/...': t.string,
        "PromptExprNonJinja/PromptExprContents/...": t.operator,

        "TemplateStringDecl/IdentifierDecl": t.attributeName,
        "TemplateStringDecl/PromptExpr": t.string,

        "TupleValue/IdentifierDecl": t.operator,

        "TrailingComment": t.comment,
        "MultilineComment": t.comment,
      })
    ]
  }),
  languageData: {
    commentTokens: { line: "//" },
    closeBrackets: {
      brackets: ["(", "[", '"', "#\"", "{"],
      stringPrefixes: ["#\""],
      wordChars: ["#", "\""],
    },
    snippetCompletion: true,

  }
})


const exampleCompletion = BAMLLanguage.data.of({
  autocomplete: [
    snippetCompletion('@alias(#"${one}"#)', { label: '@alias' }),
    snippetCompletion('@description(#"${}"#)', { label: '@description' }),
    snippetCompletion("class ${ClassName} {\n  ${property string}\n}", { label: 'class' }),
    snippetCompletion('enum ${EnumName} {\n  ${ONE}\n  ${TWO}\n}', { label: 'enum' }),
    snippetCompletion('function MyFunction(${arg: string}) -> ${YourOutputType} {\n  ${client YourClient}\n  prompt #"\n    ${add some instructions}\n  "#\n}', { label: 'function' }),
    snippetCompletion('prompt #"\n  {{ _.chat("user") }}\n  INPUT:\n  ---\n  {{ your-variable }}\n  ---\n  Response:\n"#', { label: 'prompt #"' }),
    snippetCompletion('#"${mystring}"#', { label: '#"' }),
    snippetCompletion('client<llm> GPT4 {\n  provider baml-openai-chat\n  options {\n    model gpt4  \n}}', { label: 'client<llm> GPT4' }),
    snippetCompletion("template_string MyString(${arg1}: string) #\"\n  A jinja string\n\"#", { label: 'template_string' }),
  ],
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