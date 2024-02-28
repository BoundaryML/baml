"use strict";
/// Content once a function has been selected.
Object.defineProperty(exports, "__esModule", { value: true });
var hooks_1 = require("./hooks");
var react_1 = require("@vscode/webview-ui-toolkit/react");
var react_2 = require("react");
var Link_1 = require("./Link");
var clsx_1 = require("clsx");
var Whitespace = function (_a) {
    var char = _a.char;
    return (<span className="opacity-50 text-vscode-descriptionForeground">{char === 'space' ? <>&middot;</> : <>&rarr;</>}</span>);
};
var InvisibleUtf = function (_a) {
    var text = _a.text;
    return (<span className="text-xs text-red-500 opacity-75">
    {text
            .split('')
            .map(function (c) { return "U+".concat(c.charCodeAt(0).toString(16).padStart(4, '0')); })
            .join('')}
  </span>);
};
// Excludes 0x20 (space) and 0x09 (tab)
var VISIBLE_WHITESPACE = /\u0020\u0009/;
var INVISIBLE_CODES = /\u00a0\u00ad\u034f\u061c\u070f\u115f\u1160\u1680\u17b4\u17b5\u180e\u2000\u2001\u2002\u2003\u2004\u2005\u2006\u2007\u2008\u2009\u200a\u200b\u200c\u200d\u200e\u200f\u202f\u205f\u2060\u2061\u2062\u2063\u2064\u206a\u206b\u206c\u206d\u206e\u206f\u3000\u2800\u3164\ufeff\uffa0/;
var whitespaceRegexp = new RegExp("([".concat(VISIBLE_WHITESPACE, "]+|[").concat(INVISIBLE_CODES, "]+)"), 'g');
var CodeLine = function (_a) {
    var line = _a.line, number = _a.number, showWhitespace = _a.showWhitespace, wrapText = _a.wrapText;
    // Function to render whitespace characters and invisible UTF characters with special styling
    var renderLine = function (text) {
        // Function to replace whitespace characters with visible characters
        var replaceWhitespace = function (char, key) {
            if (char === ' ')
                return <Whitespace key={key} char="space"/>;
            if (char === '\t')
                return <Whitespace key={key} char="tab"/>;
            return char;
        };
        // Split the text into segments
        var segments = text.split(whitespaceRegexp);
        // Map segments to appropriate components or strings
        var formattedText = segments.map(function (segment, index) {
            if (showWhitespace && new RegExp("^[".concat(VISIBLE_WHITESPACE, "]+$")).test(segment)) {
                return segment.split('').map(function (char, charIndex) { return replaceWhitespace(char, index.toString() + charIndex); });
            }
            else if (new RegExp("^[".concat(INVISIBLE_CODES, "]+$")).test(segment)) {
                return <InvisibleUtf key={index} text={segment}/>;
            }
            else {
                return segment;
            }
        });
        return showWhitespace ? (<div className={(0, clsx_1.default)('flex text-xs', {
                'flex-wrap': wrapText,
            })}>
        {formattedText}
      </div>) : (<>{formattedText}</>);
    };
    return (<div className="table-row">
      <span className="table-cell pr-2 font-mono text-xs text-right text-gray-500 select-none">{number}</span>
      <span className={(0, clsx_1.default)('table-cell font-mono text-xs', {
            'whitespace-pre-wrap': wrapText,
        })}>
        {renderLine(line)}
      </span>
    </div>);
};
var Snippet = function (_a) {
    var text = _a.text;
    var _b = (0, react_2.useState)(true), showWhitespace = _b[0], setShowWhitespace = _b[1];
    var _c = (0, react_2.useState)(true), wrapText = _c[0], setWrapText = _c[1];
    var lines = text.split('\n');
    return (<div className="w-full p-1 overflow-hidden rounded-lg bg-vscode-input-background">
      <div className="flex flex-row justify-end gap-2 text-xs">
        <react_1.VSCodeCheckbox checked={wrapText} onChange={function (e) { return setWrapText(e.currentTarget.checked); }}>
          Wrap Text
        </react_1.VSCodeCheckbox>
        <react_1.VSCodeCheckbox checked={showWhitespace} onChange={function (e) { return setShowWhitespace(e.currentTarget.checked); }}>
          Whitespace
        </react_1.VSCodeCheckbox>
      </div>
      <pre className="w-full p-1 text-xs bg-vscode-input-background text-vscode-textPreformat-foreground">
        {lines.map(function (line, index) { return (<CodeLine key={index} line={line} number={index + 1} showWhitespace={showWhitespace} wrapText={wrapText}/>); })}
      </pre>
    </div>);
};
var ImplPanel = function (_a) {
    var impl = _a.impl;
    var func = (0, hooks_1.useImplCtx)(impl.name.value).func;
    var implPrompt = (0, react_2.useMemo)(function () {
        if (impl.has_v2) {
            return impl.prompt_v2.prompt;
        }
        else {
            var prompt_1 = impl.prompt;
            impl.input_replacers.forEach(function (_a) {
                var key = _a.key, value = _a.value;
                prompt_1 = prompt_1.replaceAll(key, "{".concat(value, "}"));
            });
            impl.output_replacers.forEach(function (_a) {
                var key = _a.key, value = _a.value;
                prompt_1 = prompt_1.replaceAll(key, value);
            });
            return prompt_1;
        }
    }, [impl]);
    if (!func)
        return null;
    return (<>
      <react_1.VSCodePanelTab key={"tab-".concat(impl.name.value)} id={"tab-".concat(func.name.value, "-").concat(impl.name.value)}>
        <div className="flex flex-row gap-1">
          <span>{impl.name.value}</span>
        </div>
      </react_1.VSCodePanelTab>
      <react_1.VSCodePanelView key={"view-".concat(impl.name.value)} id={"view-".concat(func.name.value, "-").concat(impl.name.value)}>
        <div className="flex flex-col w-full gap-2">
          <div className="flex flex-col gap-1">
            <div className="flex flex-row items-center justify-between">
              <span className="flex gap-1">
                <b>Prompt</b>
                <Link_1.default item={impl.name} display="Edit"/>
              </span>
              <div className="flex flex-row gap-1">
                {/* <span className="font-light">Client</span> */}
                <Link_1.default item={impl.client}/>
              </div>
            </div>
            {typeof implPrompt === 'string' ? <Snippet text={implPrompt}/> : <div className='flex flex-col gap-2'>
              {implPrompt.map(function (_a, index) {
                var role = _a.role, content = _a.content;
                return (<div className='flex flex-col'>
                  <div className='text-xs'><span className='text-muted-foreground'>Role:</span> <span className='font-bold'>{role}</span></div>
                  <Snippet key={index} text={content}/>
                </div>);
            })}
            </div>}
          </div>
        </div>
      </react_1.VSCodePanelView>
    </>);
};
exports.default = ImplPanel;
