// Structural validation of the hand-authored KDE KSyntaxHighlighting
// definition (syntaxes/baml.xml) with no XML dependencies: a small hand-rolled
// parser checks well-formedness (tag balance, attribute quoting, entity use),
// and the parsed tree is checked for referential integrity — every `context=`
// and `attribute=` must name a defined <context> / <itemData>, every <keyword>
// rule must name a defined, non-empty <list>.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const XML_PATH = join(import.meta.dirname, "..", "syntaxes", "baml.xml");
const source = readFileSync(XML_PATH, "utf8");

interface XmlElement {
  name: string;
  attributes: Record<string, string>;
  children: XmlElement[];
  text: string;
}

const TAG_NAME = /^[A-Za-z_][A-Za-z0-9_.:-]*/;
const ATTRIBUTE = /^\s+([A-Za-z_][A-Za-z0-9_.:-]*)\s*=\s*"([^"<]*)"/;

// Minimal non-validating XML parser: enough for a KSyntaxHighlighting file
// (elements, attributes, text, comments, prolog/DOCTYPE). Throws on unbalanced
// tags, unterminated constructs, and attributes that are not double-quoted.
function parseXml(xml: string): XmlElement {
  const root: XmlElement = { name: "#root", attributes: {}, children: [], text: "" };
  const stack: XmlElement[] = [root];
  let i = 0;

  while (i < xml.length) {
    const lt = xml.indexOf("<", i);

    if (lt === -1) {
      stack[stack.length - 1].text += xml.slice(i);
      break;
    }

    stack[stack.length - 1].text += xml.slice(i, lt);
    i = lt;

    if (xml.startsWith("<!--", i)) {
      const end = xml.indexOf("-->", i + 4);
      if (end === -1) throw new Error("unterminated comment");
      i = end + 3;
      continue;
    }

    if (xml.startsWith("<?", i)) {
      const end = xml.indexOf("?>", i + 2);
      if (end === -1) throw new Error("unterminated processing instruction");
      i = end + 2;
      continue;
    }

    if (xml.startsWith("<!", i)) {
      const end = xml.indexOf(">", i + 2);
      if (end === -1) throw new Error("unterminated declaration");
      i = end + 1;
      continue;
    }

    // Scan to the tag's closing `>`, skipping quoted attribute values.
    let j = i + 1;
    let inQuote = false;
    while (j < xml.length && (inQuote || xml[j] !== ">")) {
      if (xml[j] === '"') inQuote = !inQuote;
      j += 1;
    }
    if (j >= xml.length) throw new Error(`unterminated tag at offset ${i}`);
    if (inQuote) throw new Error(`unterminated attribute value at offset ${i}`);

    const raw = xml.slice(i + 1, j);
    i = j + 1;

    if (raw.startsWith("/")) {
      const name = raw.slice(1).trim();
      const open = stack.pop();
      if (!open || open.name === "#root") throw new Error(`stray closing tag </${name}>`);
      if (open.name !== name) throw new Error(`mismatched tags: <${open.name}> closed by </${name}>`);
      continue;
    }

    const selfClosing = raw.endsWith("/");
    const body = selfClosing ? raw.slice(0, -1) : raw;
    const nameMatch = TAG_NAME.exec(body);
    if (!nameMatch) throw new Error(`malformed tag <${raw}>`);

    const element: XmlElement = {
      name: nameMatch[0],
      attributes: {},
      children: [],
      text: "",
    };

    // Everything after the name must be well-formed, double-quoted attributes.
    let rest = body.slice(nameMatch[0].length);
    while (!/^\s*$/.test(rest)) {
      const attrMatch = ATTRIBUTE.exec(rest);
      if (!attrMatch) throw new Error(`malformed attributes in <${raw}>`);
      if (attrMatch[1] in element.attributes) {
        throw new Error(`duplicate attribute ${attrMatch[1]} in <${element.name}>`);
      }
      element.attributes[attrMatch[1]] = attrMatch[2];
      rest = rest.slice(attrMatch[0].length);
    }

    stack[stack.length - 1].children.push(element);
    if (!selfClosing) stack.push(element);
  }

  if (stack.length !== 1) {
    throw new Error(`unclosed tag <${stack[stack.length - 1].name}>`);
  }

  return root;
}

function walk(element: XmlElement, visit: (element: XmlElement) => void) {
  visit(element);
  for (const child of element.children) walk(child, visit);
}

function find(element: XmlElement, name: string): XmlElement[] {
  const found: XmlElement[] = [];
  walk(element, (el) => {
    if (el.name === name) found.push(el);
  });
  return found;
}

const root = parseXml(source);
const language = root.children[0];

// Context references may also be the special forms #stay / #pop chains
// (optionally with a #pop!Target switch) or cross-language ##Name includes.
function referencedContextName(reference: string): string | null {
  if (reference === "#stay" || reference.startsWith("##")) return null;
  const popChain = /^#pop(?:#pop)*(?:!(.+))?$/.exec(reference);
  if (popChain) return popChain[1] ?? null;
  return reference;
}

describe("BAML KDE syntax definition", () => {
  it("is well-formed: balanced tags, quoted attributes, valid entities", () => {
    // parseXml already threw during module init if the file were unbalanced;
    // re-run on the raw source so the failure surfaces in this test.
    expect(() => parseXml(source)).not.toThrow();

    // Every ampersand must begin a standard or numeric character reference.
    const badEntity = /&(?!amp;|lt;|gt;|quot;|apos;|#[0-9]+;|#x[0-9A-Fa-f]+;)/.exec(source);
    expect(badEntity).toBeNull();

    // Exactly one document element.
    expect(root.children).toHaveLength(1);
    expect(language.name).toBe("language");
  });

  it("declares the expected language header", () => {
    expect(language.attributes).toMatchObject({
      name: "BAML",
      section: "Sources",
      extensions: "*.baml",
      version: "1",
      kateversion: "5.62",
      author: "Boundary (contact@boundaryml.com)",
      license: "Apache-2.0",
    });
  });

  it("defines uniquely named contexts and itemDatas", () => {
    const contextNames = find(language, "context").map((el) => el.attributes.name);
    const itemDataNames = find(language, "itemData").map((el) => el.attributes.name);

    expect(contextNames.length).toBeGreaterThan(0);
    expect(itemDataNames.length).toBeGreaterThan(0);
    expect(new Set(contextNames).size).toBe(contextNames.length);
    expect(new Set(itemDataNames).size).toBe(itemDataNames.length);
  });

  it("resolves every context= and attribute= reference", () => {
    const contexts = new Set(find(language, "context").map((el) => el.attributes.name));
    const itemDatas = new Set(find(language, "itemData").map((el) => el.attributes.name));
    const contextKeys = ["context", "lineEndContext", "lineEmptyContext", "fallthroughContext"];
    const problems: string[] = [];

    for (const contextsBlock of find(language, "contexts")) {
      walk(contextsBlock, (el) => {
        if (el.name === "contexts") return;

        const attribute = el.attributes.attribute;
        if (attribute !== undefined && !itemDatas.has(attribute)) {
          problems.push(`<${el.name}> references undefined itemData "${attribute}"`);
        }

        for (const key of contextKeys) {
          const reference = el.attributes[key];
          if (reference === undefined) continue;
          const target = referencedContextName(reference);
          if (target !== null && !contexts.has(target)) {
            problems.push(`<${el.name}> ${key}="${reference}" references an undefined context`);
          }
        }

        if (el.name === "IncludeRules") {
          const target = referencedContextName(el.attributes.context ?? "");
          if (target !== null && !contexts.has(target)) {
            problems.push(`<IncludeRules context="${el.attributes.context}"> is undefined`);
          }
        }
      });
    }

    expect(problems).toEqual([]);
  });

  it("has non-empty keyword lists, all referenced by name", () => {
    const lists = new Map(
      find(language, "list").map((el) => [el.attributes.name, find(el, "item")]),
    );

    expect(lists.size).toBeGreaterThan(0);

    // Every list has at least one item with non-blank content.
    for (const [name, items] of lists) {
      expect(items.length, `list "${name}" is empty`).toBeGreaterThan(0);
      for (const item of items) {
        expect(item.text.trim(), `list "${name}" has a blank item`).not.toBe("");
      }
    }

    // Every <keyword> rule points at a defined list.
    const keywordRules = find(language, "keyword");
    expect(keywordRules.length).toBeGreaterThan(0);
    for (const rule of keywordRules) {
      expect(lists.has(rule.attributes.String), `keyword rule references undefined list "${rule.attributes.String}"`).toBe(true);
    }

    // The categories this grammar promises to cover.
    for (const expected of [
      "declarations",
      "controlflow",
      "wordoperators",
      "types",
      "constants",
      "builtins",
      "templatekeywords",
    ]) {
      expect(lists.has(expected), `expected list "${expected}"`).toBe(true);
    }
  });

  it("uses only valid KDE default styles", () => {
    const validStyles = new Set([
      "dsNormal", "dsKeyword", "dsFunction", "dsVariable", "dsControlFlow",
      "dsOperator", "dsBuiltIn", "dsExtension", "dsPreprocessor", "dsAttribute",
      "dsChar", "dsSpecialChar", "dsString", "dsVerbatimString", "dsSpecialString",
      "dsImport", "dsDataType", "dsDecVal", "dsBaseN", "dsFloat", "dsConstant",
      "dsComment", "dsCommentVar", "dsRegionMarker", "dsInformation", "dsWarning",
      "dsAlert", "dsOthers", "dsError", "dsAnnotation", "dsDocumentation",
    ]);

    for (const itemData of find(language, "itemData")) {
      expect(
        validStyles.has(itemData.attributes.defStyleNum),
        `itemData "${itemData.attributes.name}" has invalid defStyleNum "${itemData.attributes.defStyleNum}"`,
      ).toBe(true);
    }
  });

  it("marks contexts entered by dynamic rules as dynamic", () => {
    const contextsByName = new Map(
      find(language, "context").map((el) => [el.attributes.name, el]),
    );

    for (const contextsBlock of find(language, "contexts")) {
      walk(contextsBlock, (el) => {
        if (el.name === "context" || el.name === "contexts") return;
        // A rule whose String captures a group and whose target context uses
        // %N substitution must land in a dynamic="true" context.
        const target = referencedContextName(el.attributes.context ?? "");
        if (target === null) return;
        const targetContext = contextsByName.get(target);
        if (!targetContext) return;
        const usesSubstitution = find(targetContext, "RegExpr").some(
          (rule) => rule.attributes.dynamic === "true",
        );
        if (usesSubstitution && targetContext.attributes.dynamic !== "true") {
          throw new Error(`context "${target}" has dynamic rules but is not dynamic="true"`);
        }
      });
    }
  });
});
