// Import necessary classes
import { Diagnostics } from '../diagnostics';
import { DictRawWrapper } from './dict_wrapper';
import { ListRawWrapper } from './list_wrapper';
import { RawNoneWrapper, RawBaseWrapper, RawStringWrapper } from './primitive_wrapper';
import { RawWrapper } from './raw_wrapper';
import XRegExp from 'xregexp';

export function fromValue(val: any, diagnostics: Diagnostics): RawWrapper {
    if (val === null) {
        return new RawNoneWrapper();
    }
    if (typeof val === 'boolean') {
        return new RawBaseWrapper(val);
    }
    if (typeof val === 'number') {
        if (Number.isInteger(val)) {
            return new RawBaseWrapper(val);
        } else {
            return new RawBaseWrapper(val);
        }
    }
    if (typeof val === 'string') {
        const strVal = val.trim();

        if (strVal.toLowerCase() === "true") {
            return new RawBaseWrapper(true);
        }
        if (strVal.toLowerCase() === "false") {
            return new RawBaseWrapper(false);
        }

        const isNumber = /^(\+|-)?\d+(\.\d+)?$/.test(strVal);
        if (isNumber) {
            if (strVal.includes(".")) {
                return new RawBaseWrapper(parseFloat(strVal));
            }
            return new RawBaseWrapper(parseInt(strVal));
        }

        let parsedList: any[] | undefined = undefined;
        let parsedObj: { [key: string]: any } | undefined = undefined;

        try {
            const parsed = JSON.parse(strVal);
            if (Array.isArray(parsed)) {
                parsedList = parsed;
            } else if (parsed !== null && typeof parsed === 'object') {
                parsedObj = parsed;
            }
        } catch (e) {
            try {
                const parsed = JSON.parse(makeStringRobustForJson(strVal));
                if (Array.isArray(parsed)) {
                    parsedList = parsed;
                } else if (parsed !== null && typeof parsed === 'object') {
                    parsedObj = parsed;
                }
            } catch (e) {
                // Do nothing
            }
        }

        if (parsedList !== undefined) {
            return new ListRawWrapper(parsedList.map(item => fromValue(item, diagnostics)));
        }

        if (parsedObj !== undefined) {
            const dict: Map<RawWrapper, RawWrapper> = new Map();
            for (const [key, value] of Object.entries(parsedObj)) {
                dict.set(fromValue(key, diagnostics), fromValue(value, diagnostics));
            }
            return new DictRawWrapper(dict);
        }

        // Further string parsing logic for as_inner, as_obj, as_list goes here...
        // Use regular expressions similar to Python's re.findall to extract relevant parts
        let asInner: RawWrapper | undefined = undefined;
        let asObj: RawWrapper | undefined = undefined;
        let asList: RawWrapper | undefined = undefined;

        // Regex to match json within triple backticks
        let startPos = strVal.indexOf('```json');
        if (startPos >= 0 && strVal.indexOf('```', startPos + 4) >= 0) {
            const jsonMatch = XRegExp.matchRecursive(strVal, '```json', '```');
            if (jsonMatch) {
                if (jsonMatch.length >= 1) {
                    asInner = fromValue(jsonMatch[0], diagnostics);
                    // TODO: Handle multiple matches
                }
            }
        } else {

        }

        // Regex to match object-like structures
        if (!(strVal.startsWith('{') && strVal.endsWith('}'))) {
            const objMatch = XRegExp.matchRecursive(strVal, '{', '}', 'g')
            if (objMatch.length > 1) {
                asList = new ListRawWrapper(objMatch.map(match => fromValue(`{${match}}`, diagnostics)));
            } else if (objMatch.length === 1) {
                asObj = fromValue(`{${objMatch[0]}}`, diagnostics);
            }
        }

        // Regex to match list-like structures
        if (asList === undefined) {
            if (!(strVal.startsWith('[') && strVal.endsWith(']'))) {
                const listMatch = XRegExp.matchRecursive(strVal, '\\[', '\\]', 'g')
                if (listMatch.length === 1) {
                    asList = fromValue(`[${listMatch[0]}]`, diagnostics);
                } else {
                    asList = new ListRawWrapper(listMatch.map(match => fromValue(match, diagnostics)));
                }
            }
        }

        return new RawStringWrapper(val, asObj, asList, asInner);
    }
    if (Array.isArray(val)) {
        return new ListRawWrapper(val.map(item => fromValue(item, diagnostics)));
    }
    if (typeof val === 'object' && val !== null) {
        const dict: Map<RawWrapper, RawWrapper> = new Map();
        for (const [key, value] of Object.entries(val)) {
            dict.set(fromValue(key, diagnostics), fromValue(value, diagnostics));
        }
        return new DictRawWrapper(dict);
    }

    diagnostics.pushUnknownError(`Unrecognized type: ${typeof val} in value ${val}`);
    diagnostics.toException();

    throw new Error("[unreachable] Unsupported type: " + typeof val);
}

function makeStringRobustForJson(s: string): string {
    let inString = false;
    let escapeCount = 0;
    let result: string[] = [];

    for (const char of s) {
        // Check for the quote character
        if (char === '"') {
            // If preceded by an odd number of backslashes, it's an escaped quote and doesn't toggle the string state
            if (escapeCount % 2 === 0) {
                inString = !inString;
                escapeCount = 0;  // Reset escape sequence counter after a non-escaped quote
            }
            // If it's an escaped quote, just reset the counter but don't add to it
        } else if (char === '\\') {
            // Increment escape sequence counter if we're in a string
            if (inString) {
                escapeCount += 1;
            }
        } else {
            // Any other character resets the escape sequence counter
            escapeCount = 0;
        }

        // When inside a string, escape the newline characters
        if (inString && char === '\n') {
            result.push('\\n');
        } else {
            result.push(char);
        }
    }

    return result.join('');
}
