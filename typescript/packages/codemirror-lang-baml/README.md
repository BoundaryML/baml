## test this in lezer-playground.vercel.app

Add this to the end of the grammar:
@detectDelim
@external propSource jsonHighlighting from "./highlight"

The highlighting stuff (udpate this with the new highlighter file)

```
import { styleTags, tags as t } from "@lezer/highlight";

export const jsonHighlighting = styleTags({
 // String: t.string,
	"ClassDecl": t.keyword,
	"ClassDecl/IdentifierDecl": t.name,
	"TestDecl/IdentifierDecl": t.name,
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

	//IdentifierDecl: t.variable,
  Number: t.number,
  "True False": t.bool,
  PropertyName: t.propertyName,
  Null: t.null,
  ",": t.separator,
  "[ ]": t.squareBracket,
  "{ }": t.brace,
});

// A very dim/dull syntax highlighting so you have something to look at, but also to trigger you to write your own ;)
// Also shows that you can use `export let extension = [...]`, to add extensions to the "demo text" editor.
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
const syntax_colors = syntaxHighlighting(
  HighlightStyle.define(
    [
			      { tag: t.brace, color: "#a8a8a8" },
			      { tag: t.annotation, color: "#a8a8a8" },

      { tag: t.name, color: "#a8a8a8" },
      { tag: t.propertyName, color: "#966a6a" },
      { tag: t.comment, color: "#4b4949" },
      { tag: t.atom, color: "#a25496" },

      { tag: t.literal, color: "#7b87b8" },
      { tag: t.unit, color: "#7b87b8" },
      { tag: t.null, color: "#7b87b8" },

      { tag: t.keyword, color: "#585858" },
      { tag: t.punctuation, color: "#585858" },
      { tag: t.derefOperator, color: "#585858" },
      { tag: t.special(t.brace), fontWeight: 700 },

      { tag: t.operator, color: "white" },
      { tag: t.self, color: "white" },
      { tag: t.function(t.punctuation), color: "white" },
      { tag: t.special(t.logicOperator), color: "white", fontWeight: "bold" },
      { tag: t.moduleKeyword, color: "white", fontWeight: "bold" },
      { tag: t.controlKeyword, color: "white", fontWeight: "bold" },
      { tag: t.controlOperator, color: "white", fontWeight: "bold" },
    ],
    { all: { color: "#585858" } }
  )
);

export let extensions = [syntax_colors];
```
