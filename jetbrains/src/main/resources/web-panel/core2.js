import { t as toHtml } from './index.js';

let ShikiError$2 = class ShikiError extends Error {
  constructor(message) {
    super(message);
    this.name = "ShikiError";
  }
};

let ShikiError$1 = class ShikiError extends Error {
  constructor(message) {
    super(message);
    this.name = "ShikiError";
  }
};

function getHeapMax() {
  return 2147483648;
}
function _emscripten_get_now() {
  return typeof performance !== "undefined" ? performance.now() : Date.now();
}
const alignUp = (x, multiple) => x + (multiple - x % multiple) % multiple;
async function main(init) {
  let wasmMemory;
  let buffer;
  const binding = {};
  function updateGlobalBufferAndViews(buf) {
    buffer = buf;
    binding.HEAPU8 = new Uint8Array(buf);
    binding.HEAPU32 = new Uint32Array(buf);
  }
  function _emscripten_memcpy_big(dest, src, num) {
    binding.HEAPU8.copyWithin(dest, src, src + num);
  }
  function emscripten_realloc_buffer(size) {
    try {
      wasmMemory.grow(size - buffer.byteLength + 65535 >>> 16);
      updateGlobalBufferAndViews(wasmMemory.buffer);
      return 1;
    } catch {
    }
  }
  function _emscripten_resize_heap(requestedSize) {
    const oldSize = binding.HEAPU8.length;
    requestedSize = requestedSize >>> 0;
    const maxHeapSize = getHeapMax();
    if (requestedSize > maxHeapSize)
      return false;
    for (let cutDown = 1; cutDown <= 4; cutDown *= 2) {
      let overGrownHeapSize = oldSize * (1 + 0.2 / cutDown);
      overGrownHeapSize = Math.min(overGrownHeapSize, requestedSize + 100663296);
      const newSize = Math.min(maxHeapSize, alignUp(Math.max(requestedSize, overGrownHeapSize), 65536));
      const replacement = emscripten_realloc_buffer(newSize);
      if (replacement)
        return true;
    }
    return false;
  }
  const UTF8Decoder = typeof TextDecoder != "undefined" ? new TextDecoder("utf8") : undefined;
  function UTF8ArrayToString(heapOrArray, idx, maxBytesToRead = 1024) {
    const endIdx = idx + maxBytesToRead;
    let endPtr = idx;
    while (heapOrArray[endPtr] && !(endPtr >= endIdx))
      ++endPtr;
    if (endPtr - idx > 16 && heapOrArray.buffer && UTF8Decoder) {
      return UTF8Decoder.decode(heapOrArray.subarray(idx, endPtr));
    }
    let str = "";
    while (idx < endPtr) {
      let u0 = heapOrArray[idx++];
      if (!(u0 & 128)) {
        str += String.fromCharCode(u0);
        continue;
      }
      const u1 = heapOrArray[idx++] & 63;
      if ((u0 & 224) === 192) {
        str += String.fromCharCode((u0 & 31) << 6 | u1);
        continue;
      }
      const u2 = heapOrArray[idx++] & 63;
      if ((u0 & 240) === 224) {
        u0 = (u0 & 15) << 12 | u1 << 6 | u2;
      } else {
        u0 = (u0 & 7) << 18 | u1 << 12 | u2 << 6 | heapOrArray[idx++] & 63;
      }
      if (u0 < 65536) {
        str += String.fromCharCode(u0);
      } else {
        const ch = u0 - 65536;
        str += String.fromCharCode(55296 | ch >> 10, 56320 | ch & 1023);
      }
    }
    return str;
  }
  function UTF8ToString(ptr, maxBytesToRead) {
    return ptr ? UTF8ArrayToString(binding.HEAPU8, ptr, maxBytesToRead) : "";
  }
  const asmLibraryArg = {
    emscripten_get_now: _emscripten_get_now,
    emscripten_memcpy_big: _emscripten_memcpy_big,
    emscripten_resize_heap: _emscripten_resize_heap,
    fd_write: () => 0
  };
  async function createWasm() {
    const info = {
      env: asmLibraryArg,
      wasi_snapshot_preview1: asmLibraryArg
    };
    const exports = await init(info);
    wasmMemory = exports.memory;
    updateGlobalBufferAndViews(wasmMemory.buffer);
    Object.assign(binding, exports);
    binding.UTF8ToString = UTF8ToString;
  }
  await createWasm();
  return binding;
}

var __defProp = Object.defineProperty;
var __defNormalProp = (obj, key, value) => key in obj ? __defProp(obj, key, { enumerable: true, configurable: true, writable: true, value }) : obj[key] = value;
var __publicField = (obj, key, value) => {
  __defNormalProp(obj, typeof key !== "symbol" ? key + "" : key, value);
  return value;
};
let onigBinding = null;
function throwLastOnigError(onigBinding2) {
  throw new ShikiError$1(onigBinding2.UTF8ToString(onigBinding2.getLastOnigError()));
}
class UtfString {
  constructor(str) {
    __publicField(this, "utf16Length");
    __publicField(this, "utf8Length");
    __publicField(this, "utf16Value");
    __publicField(this, "utf8Value");
    __publicField(this, "utf16OffsetToUtf8");
    __publicField(this, "utf8OffsetToUtf16");
    const utf16Length = str.length;
    const utf8Length = UtfString._utf8ByteLength(str);
    const computeIndicesMapping = utf8Length !== utf16Length;
    const utf16OffsetToUtf8 = computeIndicesMapping ? new Uint32Array(utf16Length + 1) : null;
    if (computeIndicesMapping)
      utf16OffsetToUtf8[utf16Length] = utf8Length;
    const utf8OffsetToUtf16 = computeIndicesMapping ? new Uint32Array(utf8Length + 1) : null;
    if (computeIndicesMapping)
      utf8OffsetToUtf16[utf8Length] = utf16Length;
    const utf8Value = new Uint8Array(utf8Length);
    let i8 = 0;
    for (let i16 = 0; i16 < utf16Length; i16++) {
      const charCode = str.charCodeAt(i16);
      let codePoint = charCode;
      let wasSurrogatePair = false;
      if (charCode >= 55296 && charCode <= 56319) {
        if (i16 + 1 < utf16Length) {
          const nextCharCode = str.charCodeAt(i16 + 1);
          if (nextCharCode >= 56320 && nextCharCode <= 57343) {
            codePoint = (charCode - 55296 << 10) + 65536 | nextCharCode - 56320;
            wasSurrogatePair = true;
          }
        }
      }
      if (computeIndicesMapping) {
        utf16OffsetToUtf8[i16] = i8;
        if (wasSurrogatePair)
          utf16OffsetToUtf8[i16 + 1] = i8;
        if (codePoint <= 127) {
          utf8OffsetToUtf16[i8 + 0] = i16;
        } else if (codePoint <= 2047) {
          utf8OffsetToUtf16[i8 + 0] = i16;
          utf8OffsetToUtf16[i8 + 1] = i16;
        } else if (codePoint <= 65535) {
          utf8OffsetToUtf16[i8 + 0] = i16;
          utf8OffsetToUtf16[i8 + 1] = i16;
          utf8OffsetToUtf16[i8 + 2] = i16;
        } else {
          utf8OffsetToUtf16[i8 + 0] = i16;
          utf8OffsetToUtf16[i8 + 1] = i16;
          utf8OffsetToUtf16[i8 + 2] = i16;
          utf8OffsetToUtf16[i8 + 3] = i16;
        }
      }
      if (codePoint <= 127) {
        utf8Value[i8++] = codePoint;
      } else if (codePoint <= 2047) {
        utf8Value[i8++] = 192 | (codePoint & 1984) >>> 6;
        utf8Value[i8++] = 128 | (codePoint & 63) >>> 0;
      } else if (codePoint <= 65535) {
        utf8Value[i8++] = 224 | (codePoint & 61440) >>> 12;
        utf8Value[i8++] = 128 | (codePoint & 4032) >>> 6;
        utf8Value[i8++] = 128 | (codePoint & 63) >>> 0;
      } else {
        utf8Value[i8++] = 240 | (codePoint & 1835008) >>> 18;
        utf8Value[i8++] = 128 | (codePoint & 258048) >>> 12;
        utf8Value[i8++] = 128 | (codePoint & 4032) >>> 6;
        utf8Value[i8++] = 128 | (codePoint & 63) >>> 0;
      }
      if (wasSurrogatePair)
        i16++;
    }
    this.utf16Length = utf16Length;
    this.utf8Length = utf8Length;
    this.utf16Value = str;
    this.utf8Value = utf8Value;
    this.utf16OffsetToUtf8 = utf16OffsetToUtf8;
    this.utf8OffsetToUtf16 = utf8OffsetToUtf16;
  }
  static _utf8ByteLength(str) {
    let result = 0;
    for (let i = 0, len = str.length; i < len; i++) {
      const charCode = str.charCodeAt(i);
      let codepoint = charCode;
      let wasSurrogatePair = false;
      if (charCode >= 55296 && charCode <= 56319) {
        if (i + 1 < len) {
          const nextCharCode = str.charCodeAt(i + 1);
          if (nextCharCode >= 56320 && nextCharCode <= 57343) {
            codepoint = (charCode - 55296 << 10) + 65536 | nextCharCode - 56320;
            wasSurrogatePair = true;
          }
        }
      }
      if (codepoint <= 127)
        result += 1;
      else if (codepoint <= 2047)
        result += 2;
      else if (codepoint <= 65535)
        result += 3;
      else
        result += 4;
      if (wasSurrogatePair)
        i++;
    }
    return result;
  }
  createString(onigBinding2) {
    const result = onigBinding2.omalloc(this.utf8Length);
    onigBinding2.HEAPU8.set(this.utf8Value, result);
    return result;
  }
}
const _OnigString = class {
  constructor(str) {
    __publicField(this, "id", ++_OnigString.LAST_ID);
    __publicField(this, "_onigBinding");
    __publicField(this, "content");
    __publicField(this, "utf16Length");
    __publicField(this, "utf8Length");
    __publicField(this, "utf16OffsetToUtf8");
    __publicField(this, "utf8OffsetToUtf16");
    __publicField(this, "ptr");
    if (!onigBinding)
      throw new ShikiError$1("Must invoke loadWasm first.");
    this._onigBinding = onigBinding;
    this.content = str;
    const utfString = new UtfString(str);
    this.utf16Length = utfString.utf16Length;
    this.utf8Length = utfString.utf8Length;
    this.utf16OffsetToUtf8 = utfString.utf16OffsetToUtf8;
    this.utf8OffsetToUtf16 = utfString.utf8OffsetToUtf16;
    if (this.utf8Length < 1e4 && !_OnigString._sharedPtrInUse) {
      if (!_OnigString._sharedPtr)
        _OnigString._sharedPtr = onigBinding.omalloc(1e4);
      _OnigString._sharedPtrInUse = true;
      onigBinding.HEAPU8.set(utfString.utf8Value, _OnigString._sharedPtr);
      this.ptr = _OnigString._sharedPtr;
    } else {
      this.ptr = utfString.createString(onigBinding);
    }
  }
  convertUtf8OffsetToUtf16(utf8Offset) {
    if (this.utf8OffsetToUtf16) {
      if (utf8Offset < 0)
        return 0;
      if (utf8Offset > this.utf8Length)
        return this.utf16Length;
      return this.utf8OffsetToUtf16[utf8Offset];
    }
    return utf8Offset;
  }
  convertUtf16OffsetToUtf8(utf16Offset) {
    if (this.utf16OffsetToUtf8) {
      if (utf16Offset < 0)
        return 0;
      if (utf16Offset > this.utf16Length)
        return this.utf8Length;
      return this.utf16OffsetToUtf8[utf16Offset];
    }
    return utf16Offset;
  }
  dispose() {
    if (this.ptr === _OnigString._sharedPtr)
      _OnigString._sharedPtrInUse = false;
    else
      this._onigBinding.ofree(this.ptr);
  }
};
let OnigString = _OnigString;
__publicField(OnigString, "LAST_ID", 0);
__publicField(OnigString, "_sharedPtr", 0);
// a pointer to a string of 10000 bytes
__publicField(OnigString, "_sharedPtrInUse", false);
class OnigScanner {
  constructor(patterns) {
    __publicField(this, "_onigBinding");
    __publicField(this, "_ptr");
    if (!onigBinding)
      throw new ShikiError$1("Must invoke loadWasm first.");
    const strPtrsArr = [];
    const strLenArr = [];
    for (let i = 0, len = patterns.length; i < len; i++) {
      const utfString = new UtfString(patterns[i]);
      strPtrsArr[i] = utfString.createString(onigBinding);
      strLenArr[i] = utfString.utf8Length;
    }
    const strPtrsPtr = onigBinding.omalloc(4 * patterns.length);
    onigBinding.HEAPU32.set(strPtrsArr, strPtrsPtr / 4);
    const strLenPtr = onigBinding.omalloc(4 * patterns.length);
    onigBinding.HEAPU32.set(strLenArr, strLenPtr / 4);
    const scannerPtr = onigBinding.createOnigScanner(strPtrsPtr, strLenPtr, patterns.length);
    for (let i = 0, len = patterns.length; i < len; i++)
      onigBinding.ofree(strPtrsArr[i]);
    onigBinding.ofree(strLenPtr);
    onigBinding.ofree(strPtrsPtr);
    if (scannerPtr === 0)
      throwLastOnigError(onigBinding);
    this._onigBinding = onigBinding;
    this._ptr = scannerPtr;
  }
  dispose() {
    this._onigBinding.freeOnigScanner(this._ptr);
  }
  findNextMatchSync(string, startPosition, arg) {
    let options = 0 /* None */;
    if (typeof arg === "number") {
      options = arg;
    }
    if (typeof string === "string") {
      string = new OnigString(string);
      const result = this._findNextMatchSync(string, startPosition, false, options);
      string.dispose();
      return result;
    }
    return this._findNextMatchSync(string, startPosition, false, options);
  }
  _findNextMatchSync(string, startPosition, debugCall, options) {
    const onigBinding2 = this._onigBinding;
    const resultPtr = onigBinding2.findNextOnigScannerMatch(this._ptr, string.id, string.ptr, string.utf8Length, string.convertUtf16OffsetToUtf8(startPosition), options);
    if (resultPtr === 0) {
      return null;
    }
    const HEAPU32 = onigBinding2.HEAPU32;
    let offset = resultPtr / 4;
    const index = HEAPU32[offset++];
    const count = HEAPU32[offset++];
    const captureIndices = [];
    for (let i = 0; i < count; i++) {
      const beg = string.convertUtf8OffsetToUtf16(HEAPU32[offset++]);
      const end = string.convertUtf8OffsetToUtf16(HEAPU32[offset++]);
      captureIndices[i] = {
        start: beg,
        end,
        length: end - beg
      };
    }
    return {
      index,
      captureIndices
    };
  }
}
function isInstantiatorOptionsObject(dataOrOptions) {
  return typeof dataOrOptions.instantiator === "function";
}
function isInstantiatorModule(dataOrOptions) {
  return typeof dataOrOptions.default === "function";
}
function isDataOptionsObject(dataOrOptions) {
  return typeof dataOrOptions.data !== "undefined";
}
function isResponse(dataOrOptions) {
  return typeof Response !== "undefined" && dataOrOptions instanceof Response;
}
function isArrayBuffer(data) {
  return typeof ArrayBuffer !== "undefined" && (data instanceof ArrayBuffer || ArrayBuffer.isView(data)) || typeof Buffer !== "undefined" && Buffer.isBuffer?.(data) || typeof SharedArrayBuffer !== "undefined" && data instanceof SharedArrayBuffer || typeof Uint32Array !== "undefined" && data instanceof Uint32Array;
}
let initPromise;
function loadWasm(options) {
  if (initPromise)
    return initPromise;
  async function _load() {
    onigBinding = await main(async (info) => {
      let instance = options;
      instance = await instance;
      if (typeof instance === "function")
        instance = await instance(info);
      if (typeof instance === "function")
        instance = await instance(info);
      if (isInstantiatorOptionsObject(instance)) {
        instance = await instance.instantiator(info);
      } else if (isInstantiatorModule(instance)) {
        instance = await instance.default(info);
      } else {
        if (isDataOptionsObject(instance))
          instance = instance.data;
        if (isResponse(instance)) {
          if (typeof WebAssembly.instantiateStreaming === "function")
            instance = await _makeResponseStreamingLoader(instance)(info);
          else
            instance = await _makeResponseNonStreamingLoader(instance)(info);
        } else if (isArrayBuffer(instance)) {
          instance = await _makeArrayBufferLoader(instance)(info);
        } else if (instance instanceof WebAssembly.Module) {
          instance = await _makeArrayBufferLoader(instance)(info);
        } else if ("default" in instance && instance.default instanceof WebAssembly.Module) {
          instance = await _makeArrayBufferLoader(instance.default)(info);
        }
      }
      if ("instance" in instance)
        instance = instance.instance;
      if ("exports" in instance)
        instance = instance.exports;
      return instance;
    });
  }
  initPromise = _load();
  return initPromise;
}
function _makeArrayBufferLoader(data) {
  return (importObject) => WebAssembly.instantiate(data, importObject);
}
function _makeResponseStreamingLoader(data) {
  return (importObject) => WebAssembly.instantiateStreaming(data, importObject);
}
function _makeResponseNonStreamingLoader(data) {
  return async (importObject) => {
    const arrayBuffer = await data.arrayBuffer();
    return WebAssembly.instantiate(arrayBuffer, importObject);
  };
}

let _defaultWasmLoader;
function getDefaultWasmLoader() {
  return _defaultWasmLoader;
}
async function createOnigurumaEngine(options) {
  if (options)
    await loadWasm(options);
  return {
    createScanner(patterns) {
      return new OnigScanner(patterns.map((p) => typeof p === "string" ? p : p.source));
    },
    createString(s) {
      return new OnigString(s);
    }
  };
}

function clone(something) {
  return doClone(something);
}
function doClone(something) {
  if (Array.isArray(something)) {
    return cloneArray(something);
  }
  if (something instanceof RegExp) {
    return something;
  }
  if (typeof something === "object") {
    return cloneObj(something);
  }
  return something;
}
function cloneArray(arr) {
  let r = [];
  for (let i = 0, len = arr.length; i < len; i++) {
    r[i] = doClone(arr[i]);
  }
  return r;
}
function cloneObj(obj) {
  let r = {};
  for (let key in obj) {
    r[key] = doClone(obj[key]);
  }
  return r;
}
function mergeObjects(target, ...sources) {
  sources.forEach((source) => {
    for (let key in source) {
      target[key] = source[key];
    }
  });
  return target;
}
function basename(path) {
  const idx = ~path.lastIndexOf("/") || ~path.lastIndexOf("\\");
  if (idx === 0) {
    return path;
  } else if (~idx === path.length - 1) {
    return basename(path.substring(0, path.length - 1));
  } else {
    return path.substr(~idx + 1);
  }
}
var CAPTURING_REGEX_SOURCE = /\$(\d+)|\${(\d+):\/(downcase|upcase)}/g;
var RegexSource = class {
  static hasCaptures(regexSource) {
    if (regexSource === null) {
      return false;
    }
    CAPTURING_REGEX_SOURCE.lastIndex = 0;
    return CAPTURING_REGEX_SOURCE.test(regexSource);
  }
  static replaceCaptures(regexSource, captureSource, captureIndices) {
    return regexSource.replace(CAPTURING_REGEX_SOURCE, (match, index, commandIndex, command) => {
      let capture = captureIndices[parseInt(index || commandIndex, 10)];
      if (capture) {
        let result = captureSource.substring(capture.start, capture.end);
        while (result[0] === ".") {
          result = result.substring(1);
        }
        switch (command) {
          case "downcase":
            return result.toLowerCase();
          case "upcase":
            return result.toUpperCase();
          default:
            return result;
        }
      } else {
        return match;
      }
    });
  }
};
function strcmp(a, b) {
  if (a < b) {
    return -1;
  }
  if (a > b) {
    return 1;
  }
  return 0;
}
function strArrCmp(a, b) {
  if (a === null && b === null) {
    return 0;
  }
  if (!a) {
    return -1;
  }
  if (!b) {
    return 1;
  }
  let len1 = a.length;
  let len2 = b.length;
  if (len1 === len2) {
    for (let i = 0; i < len1; i++) {
      let res = strcmp(a[i], b[i]);
      if (res !== 0) {
        return res;
      }
    }
    return 0;
  }
  return len1 - len2;
}
function isValidHexColor(hex) {
  if (/^#[0-9a-f]{6}$/i.test(hex)) {
    return true;
  }
  if (/^#[0-9a-f]{8}$/i.test(hex)) {
    return true;
  }
  if (/^#[0-9a-f]{3}$/i.test(hex)) {
    return true;
  }
  if (/^#[0-9a-f]{4}$/i.test(hex)) {
    return true;
  }
  return false;
}
function escapeRegExpCharacters(value) {
  return value.replace(/[\-\\\{\}\*\+\?\|\^\$\.\,\[\]\(\)\#\s]/g, "\\$&");
}
var CachedFn = class {
  constructor(fn) {
    this.fn = fn;
  }
  cache = /* @__PURE__ */ new Map();
  get(key) {
    if (this.cache.has(key)) {
      return this.cache.get(key);
    }
    const value = this.fn(key);
    this.cache.set(key, value);
    return value;
  }
};
var Theme = class {
  constructor(_colorMap, _defaults, _root) {
    this._colorMap = _colorMap;
    this._defaults = _defaults;
    this._root = _root;
  }
  static createFromRawTheme(source, colorMap) {
    return this.createFromParsedTheme(parseTheme(source), colorMap);
  }
  static createFromParsedTheme(source, colorMap) {
    return resolveParsedThemeRules(source, colorMap);
  }
  _cachedMatchRoot = new CachedFn(
    (scopeName) => this._root.match(scopeName)
  );
  getColorMap() {
    return this._colorMap.getColorMap();
  }
  getDefaults() {
    return this._defaults;
  }
  match(scopePath) {
    if (scopePath === null) {
      return this._defaults;
    }
    const scopeName = scopePath.scopeName;
    const matchingTrieElements = this._cachedMatchRoot.get(scopeName);
    const effectiveRule = matchingTrieElements.find(
      (v) => _scopePathMatchesParentScopes(scopePath.parent, v.parentScopes)
    );
    if (!effectiveRule) {
      return null;
    }
    return new StyleAttributes(
      effectiveRule.fontStyle,
      effectiveRule.foreground,
      effectiveRule.background
    );
  }
};
var ScopeStack = class _ScopeStack {
  constructor(parent, scopeName) {
    this.parent = parent;
    this.scopeName = scopeName;
  }
  static push(path, scopeNames) {
    for (const name of scopeNames) {
      path = new _ScopeStack(path, name);
    }
    return path;
  }
  static from(...segments) {
    let result = null;
    for (let i = 0; i < segments.length; i++) {
      result = new _ScopeStack(result, segments[i]);
    }
    return result;
  }
  push(scopeName) {
    return new _ScopeStack(this, scopeName);
  }
  getSegments() {
    let item = this;
    const result = [];
    while (item) {
      result.push(item.scopeName);
      item = item.parent;
    }
    result.reverse();
    return result;
  }
  toString() {
    return this.getSegments().join(" ");
  }
  extends(other) {
    if (this === other) {
      return true;
    }
    if (this.parent === null) {
      return false;
    }
    return this.parent.extends(other);
  }
  getExtensionIfDefined(base) {
    const result = [];
    let item = this;
    while (item && item !== base) {
      result.push(item.scopeName);
      item = item.parent;
    }
    return item === base ? result.reverse() : undefined;
  }
};
function _scopePathMatchesParentScopes(scopePath, parentScopes) {
  if (parentScopes.length === 0) {
    return true;
  }
  for (let index = 0; index < parentScopes.length; index++) {
    let scopePattern = parentScopes[index];
    let scopeMustMatch = false;
    if (scopePattern === ">") {
      if (index === parentScopes.length - 1) {
        return false;
      }
      scopePattern = parentScopes[++index];
      scopeMustMatch = true;
    }
    while (scopePath) {
      if (_matchesScope(scopePath.scopeName, scopePattern)) {
        break;
      }
      if (scopeMustMatch) {
        return false;
      }
      scopePath = scopePath.parent;
    }
    if (!scopePath) {
      return false;
    }
    scopePath = scopePath.parent;
  }
  return true;
}
function _matchesScope(scopeName, scopePattern) {
  return scopePattern === scopeName || scopeName.startsWith(scopePattern) && scopeName[scopePattern.length] === ".";
}
var StyleAttributes = class {
  constructor(fontStyle, foregroundId, backgroundId) {
    this.fontStyle = fontStyle;
    this.foregroundId = foregroundId;
    this.backgroundId = backgroundId;
  }
};
function parseTheme(source) {
  if (!source) {
    return [];
  }
  if (!source.settings || !Array.isArray(source.settings)) {
    return [];
  }
  let settings = source.settings;
  let result = [], resultLen = 0;
  for (let i = 0, len = settings.length; i < len; i++) {
    let entry = settings[i];
    if (!entry.settings) {
      continue;
    }
    let scopes;
    if (typeof entry.scope === "string") {
      let _scope = entry.scope;
      _scope = _scope.replace(/^[,]+/, "");
      _scope = _scope.replace(/[,]+$/, "");
      scopes = _scope.split(",");
    } else if (Array.isArray(entry.scope)) {
      scopes = entry.scope;
    } else {
      scopes = [""];
    }
    let fontStyle = -1;
    if (typeof entry.settings.fontStyle === "string") {
      fontStyle = 0;
      let segments = entry.settings.fontStyle.split(" ");
      for (let j = 0, lenJ = segments.length; j < lenJ; j++) {
        let segment = segments[j];
        switch (segment) {
          case "italic":
            fontStyle = fontStyle | 1;
            break;
          case "bold":
            fontStyle = fontStyle | 2;
            break;
          case "underline":
            fontStyle = fontStyle | 4;
            break;
          case "strikethrough":
            fontStyle = fontStyle | 8;
            break;
        }
      }
    }
    let foreground = null;
    if (typeof entry.settings.foreground === "string" && isValidHexColor(entry.settings.foreground)) {
      foreground = entry.settings.foreground;
    }
    let background = null;
    if (typeof entry.settings.background === "string" && isValidHexColor(entry.settings.background)) {
      background = entry.settings.background;
    }
    for (let j = 0, lenJ = scopes.length; j < lenJ; j++) {
      let _scope = scopes[j].trim();
      let segments = _scope.split(" ");
      let scope = segments[segments.length - 1];
      let parentScopes = null;
      if (segments.length > 1) {
        parentScopes = segments.slice(0, segments.length - 1);
        parentScopes.reverse();
      }
      result[resultLen++] = new ParsedThemeRule(
        scope,
        parentScopes,
        i,
        fontStyle,
        foreground,
        background
      );
    }
  }
  return result;
}
var ParsedThemeRule = class {
  constructor(scope, parentScopes, index, fontStyle, foreground, background) {
    this.scope = scope;
    this.parentScopes = parentScopes;
    this.index = index;
    this.fontStyle = fontStyle;
    this.foreground = foreground;
    this.background = background;
  }
};
var FontStyle = /* @__PURE__ */ ((FontStyle2) => {
  FontStyle2[FontStyle2["NotSet"] = -1] = "NotSet";
  FontStyle2[FontStyle2["None"] = 0] = "None";
  FontStyle2[FontStyle2["Italic"] = 1] = "Italic";
  FontStyle2[FontStyle2["Bold"] = 2] = "Bold";
  FontStyle2[FontStyle2["Underline"] = 4] = "Underline";
  FontStyle2[FontStyle2["Strikethrough"] = 8] = "Strikethrough";
  return FontStyle2;
})(FontStyle || {});
function resolveParsedThemeRules(parsedThemeRules, _colorMap) {
  parsedThemeRules.sort((a, b) => {
    let r = strcmp(a.scope, b.scope);
    if (r !== 0) {
      return r;
    }
    r = strArrCmp(a.parentScopes, b.parentScopes);
    if (r !== 0) {
      return r;
    }
    return a.index - b.index;
  });
  let defaultFontStyle = 0;
  let defaultForeground = "#000000";
  let defaultBackground = "#ffffff";
  while (parsedThemeRules.length >= 1 && parsedThemeRules[0].scope === "") {
    let incomingDefaults = parsedThemeRules.shift();
    if (incomingDefaults.fontStyle !== -1) {
      defaultFontStyle = incomingDefaults.fontStyle;
    }
    if (incomingDefaults.foreground !== null) {
      defaultForeground = incomingDefaults.foreground;
    }
    if (incomingDefaults.background !== null) {
      defaultBackground = incomingDefaults.background;
    }
  }
  let colorMap = new ColorMap(_colorMap);
  let defaults = new StyleAttributes(defaultFontStyle, colorMap.getId(defaultForeground), colorMap.getId(defaultBackground));
  let root = new ThemeTrieElement(new ThemeTrieElementRule(0, null, -1, 0, 0), []);
  for (let i = 0, len = parsedThemeRules.length; i < len; i++) {
    let rule = parsedThemeRules[i];
    root.insert(0, rule.scope, rule.parentScopes, rule.fontStyle, colorMap.getId(rule.foreground), colorMap.getId(rule.background));
  }
  return new Theme(colorMap, defaults, root);
}
var ColorMap = class {
  _isFrozen;
  _lastColorId;
  _id2color;
  _color2id;
  constructor(_colorMap) {
    this._lastColorId = 0;
    this._id2color = [];
    this._color2id = /* @__PURE__ */ Object.create(null);
    if (Array.isArray(_colorMap)) {
      this._isFrozen = true;
      for (let i = 0, len = _colorMap.length; i < len; i++) {
        this._color2id[_colorMap[i]] = i;
        this._id2color[i] = _colorMap[i];
      }
    } else {
      this._isFrozen = false;
    }
  }
  getId(color) {
    if (color === null) {
      return 0;
    }
    color = color.toUpperCase();
    let value = this._color2id[color];
    if (value) {
      return value;
    }
    if (this._isFrozen) {
      throw new Error(`Missing color in color map - ${color}`);
    }
    value = ++this._lastColorId;
    this._color2id[color] = value;
    this._id2color[value] = color;
    return value;
  }
  getColorMap() {
    return this._id2color.slice(0);
  }
};
var emptyParentScopes = Object.freeze([]);
var ThemeTrieElementRule = class _ThemeTrieElementRule {
  scopeDepth;
  parentScopes;
  fontStyle;
  foreground;
  background;
  constructor(scopeDepth, parentScopes, fontStyle, foreground, background) {
    this.scopeDepth = scopeDepth;
    this.parentScopes = parentScopes || emptyParentScopes;
    this.fontStyle = fontStyle;
    this.foreground = foreground;
    this.background = background;
  }
  clone() {
    return new _ThemeTrieElementRule(this.scopeDepth, this.parentScopes, this.fontStyle, this.foreground, this.background);
  }
  static cloneArr(arr) {
    let r = [];
    for (let i = 0, len = arr.length; i < len; i++) {
      r[i] = arr[i].clone();
    }
    return r;
  }
  acceptOverwrite(scopeDepth, fontStyle, foreground, background) {
    if (this.scopeDepth > scopeDepth) {
      console.log("how did this happen?");
    } else {
      this.scopeDepth = scopeDepth;
    }
    if (fontStyle !== -1) {
      this.fontStyle = fontStyle;
    }
    if (foreground !== 0) {
      this.foreground = foreground;
    }
    if (background !== 0) {
      this.background = background;
    }
  }
};
var ThemeTrieElement = class _ThemeTrieElement {
  constructor(_mainRule, rulesWithParentScopes = [], _children = {}) {
    this._mainRule = _mainRule;
    this._children = _children;
    this._rulesWithParentScopes = rulesWithParentScopes;
  }
  _rulesWithParentScopes;
  static _cmpBySpecificity(a, b) {
    if (a.scopeDepth !== b.scopeDepth) {
      return b.scopeDepth - a.scopeDepth;
    }
    let aParentIndex = 0;
    let bParentIndex = 0;
    while (true) {
      if (a.parentScopes[aParentIndex] === ">") {
        aParentIndex++;
      }
      if (b.parentScopes[bParentIndex] === ">") {
        bParentIndex++;
      }
      if (aParentIndex >= a.parentScopes.length || bParentIndex >= b.parentScopes.length) {
        break;
      }
      const parentScopeLengthDiff = b.parentScopes[bParentIndex].length - a.parentScopes[aParentIndex].length;
      if (parentScopeLengthDiff !== 0) {
        return parentScopeLengthDiff;
      }
      aParentIndex++;
      bParentIndex++;
    }
    return b.parentScopes.length - a.parentScopes.length;
  }
  match(scope) {
    if (scope !== "") {
      let dotIndex = scope.indexOf(".");
      let head;
      let tail;
      if (dotIndex === -1) {
        head = scope;
        tail = "";
      } else {
        head = scope.substring(0, dotIndex);
        tail = scope.substring(dotIndex + 1);
      }
      if (this._children.hasOwnProperty(head)) {
        return this._children[head].match(tail);
      }
    }
    const rules = this._rulesWithParentScopes.concat(this._mainRule);
    rules.sort(_ThemeTrieElement._cmpBySpecificity);
    return rules;
  }
  insert(scopeDepth, scope, parentScopes, fontStyle, foreground, background) {
    if (scope === "") {
      this._doInsertHere(scopeDepth, parentScopes, fontStyle, foreground, background);
      return;
    }
    let dotIndex = scope.indexOf(".");
    let head;
    let tail;
    if (dotIndex === -1) {
      head = scope;
      tail = "";
    } else {
      head = scope.substring(0, dotIndex);
      tail = scope.substring(dotIndex + 1);
    }
    let child;
    if (this._children.hasOwnProperty(head)) {
      child = this._children[head];
    } else {
      child = new _ThemeTrieElement(this._mainRule.clone(), ThemeTrieElementRule.cloneArr(this._rulesWithParentScopes));
      this._children[head] = child;
    }
    child.insert(scopeDepth + 1, tail, parentScopes, fontStyle, foreground, background);
  }
  _doInsertHere(scopeDepth, parentScopes, fontStyle, foreground, background) {
    if (parentScopes === null) {
      this._mainRule.acceptOverwrite(scopeDepth, fontStyle, foreground, background);
      return;
    }
    for (let i = 0, len = this._rulesWithParentScopes.length; i < len; i++) {
      let rule = this._rulesWithParentScopes[i];
      if (strArrCmp(rule.parentScopes, parentScopes) === 0) {
        rule.acceptOverwrite(scopeDepth, fontStyle, foreground, background);
        return;
      }
    }
    if (fontStyle === -1) {
      fontStyle = this._mainRule.fontStyle;
    }
    if (foreground === 0) {
      foreground = this._mainRule.foreground;
    }
    if (background === 0) {
      background = this._mainRule.background;
    }
    this._rulesWithParentScopes.push(new ThemeTrieElementRule(scopeDepth, parentScopes, fontStyle, foreground, background));
  }
};
var EncodedTokenMetadata = class _EncodedTokenMetadata {
  static toBinaryStr(encodedTokenAttributes) {
    return encodedTokenAttributes.toString(2).padStart(32, "0");
  }
  static print(encodedTokenAttributes) {
    const languageId = _EncodedTokenMetadata.getLanguageId(encodedTokenAttributes);
    const tokenType = _EncodedTokenMetadata.getTokenType(encodedTokenAttributes);
    const fontStyle = _EncodedTokenMetadata.getFontStyle(encodedTokenAttributes);
    const foreground = _EncodedTokenMetadata.getForeground(encodedTokenAttributes);
    const background = _EncodedTokenMetadata.getBackground(encodedTokenAttributes);
    console.log({
      languageId,
      tokenType,
      fontStyle,
      foreground,
      background
    });
  }
  static getLanguageId(encodedTokenAttributes) {
    return (encodedTokenAttributes & 255) >>> 0;
  }
  static getTokenType(encodedTokenAttributes) {
    return (encodedTokenAttributes & 768) >>> 8;
  }
  static containsBalancedBrackets(encodedTokenAttributes) {
    return (encodedTokenAttributes & 1024) !== 0;
  }
  static getFontStyle(encodedTokenAttributes) {
    return (encodedTokenAttributes & 30720) >>> 11;
  }
  static getForeground(encodedTokenAttributes) {
    return (encodedTokenAttributes & 16744448) >>> 15;
  }
  static getBackground(encodedTokenAttributes) {
    return (encodedTokenAttributes & 4278190080) >>> 24;
  }
  /**
   * Updates the fields in `metadata`.
   * A value of `0`, `NotSet` or `null` indicates that the corresponding field should be left as is.
   */
  static set(encodedTokenAttributes, languageId, tokenType, containsBalancedBrackets, fontStyle, foreground, background) {
    let _languageId = _EncodedTokenMetadata.getLanguageId(encodedTokenAttributes);
    let _tokenType = _EncodedTokenMetadata.getTokenType(encodedTokenAttributes);
    let _containsBalancedBracketsBit = _EncodedTokenMetadata.containsBalancedBrackets(encodedTokenAttributes) ? 1 : 0;
    let _fontStyle = _EncodedTokenMetadata.getFontStyle(encodedTokenAttributes);
    let _foreground = _EncodedTokenMetadata.getForeground(encodedTokenAttributes);
    let _background = _EncodedTokenMetadata.getBackground(encodedTokenAttributes);
    if (languageId !== 0) {
      _languageId = languageId;
    }
    if (tokenType !== 8) {
      _tokenType = fromOptionalTokenType(tokenType);
    }
    if (containsBalancedBrackets !== null) {
      _containsBalancedBracketsBit = containsBalancedBrackets ? 1 : 0;
    }
    if (fontStyle !== -1) {
      _fontStyle = fontStyle;
    }
    if (foreground !== 0) {
      _foreground = foreground;
    }
    if (background !== 0) {
      _background = background;
    }
    return (_languageId << 0 | _tokenType << 8 | _containsBalancedBracketsBit << 10 | _fontStyle << 11 | _foreground << 15 | _background << 24) >>> 0;
  }
};
function toOptionalTokenType(standardType) {
  return standardType;
}
function fromOptionalTokenType(standardType) {
  return standardType;
}
function createMatchers(selector, matchesName) {
  const results = [];
  const tokenizer = newTokenizer(selector);
  let token = tokenizer.next();
  while (token !== null) {
    let priority = 0;
    if (token.length === 2 && token.charAt(1) === ":") {
      switch (token.charAt(0)) {
        case "R":
          priority = 1;
          break;
        case "L":
          priority = -1;
          break;
        default:
          console.log(`Unknown priority ${token} in scope selector`);
      }
      token = tokenizer.next();
    }
    let matcher = parseConjunction();
    results.push({ matcher, priority });
    if (token !== ",") {
      break;
    }
    token = tokenizer.next();
  }
  return results;
  function parseOperand() {
    if (token === "-") {
      token = tokenizer.next();
      const expressionToNegate = parseOperand();
      return (matcherInput) => !!expressionToNegate && !expressionToNegate(matcherInput);
    }
    if (token === "(") {
      token = tokenizer.next();
      const expressionInParents = parseInnerExpression();
      if (token === ")") {
        token = tokenizer.next();
      }
      return expressionInParents;
    }
    if (isIdentifier(token)) {
      const identifiers = [];
      do {
        identifiers.push(token);
        token = tokenizer.next();
      } while (isIdentifier(token));
      return (matcherInput) => matchesName(identifiers, matcherInput);
    }
    return null;
  }
  function parseConjunction() {
    const matchers = [];
    let matcher = parseOperand();
    while (matcher) {
      matchers.push(matcher);
      matcher = parseOperand();
    }
    return (matcherInput) => matchers.every((matcher2) => matcher2(matcherInput));
  }
  function parseInnerExpression() {
    const matchers = [];
    let matcher = parseConjunction();
    while (matcher) {
      matchers.push(matcher);
      if (token === "|" || token === ",") {
        do {
          token = tokenizer.next();
        } while (token === "|" || token === ",");
      } else {
        break;
      }
      matcher = parseConjunction();
    }
    return (matcherInput) => matchers.some((matcher2) => matcher2(matcherInput));
  }
}
function isIdentifier(token) {
  return !!token && !!token.match(/[\w\.:]+/);
}
function newTokenizer(input) {
  let regex = /([LR]:|[\w\.:][\w\.:\-]*|[\,\|\-\(\)])/g;
  let match = regex.exec(input);
  return {
    next: () => {
      if (!match) {
        return null;
      }
      const res = match[0];
      match = regex.exec(input);
      return res;
    }
  };
}
function disposeOnigString(str) {
  if (typeof str.dispose === "function") {
    str.dispose();
  }
}
var TopLevelRuleReference = class {
  constructor(scopeName) {
    this.scopeName = scopeName;
  }
  toKey() {
    return this.scopeName;
  }
};
var TopLevelRepositoryRuleReference = class {
  constructor(scopeName, ruleName) {
    this.scopeName = scopeName;
    this.ruleName = ruleName;
  }
  toKey() {
    return `${this.scopeName}#${this.ruleName}`;
  }
};
var ExternalReferenceCollector = class {
  _references = [];
  _seenReferenceKeys = /* @__PURE__ */ new Set();
  get references() {
    return this._references;
  }
  visitedRule = /* @__PURE__ */ new Set();
  add(reference) {
    const key = reference.toKey();
    if (this._seenReferenceKeys.has(key)) {
      return;
    }
    this._seenReferenceKeys.add(key);
    this._references.push(reference);
  }
};
var ScopeDependencyProcessor = class {
  constructor(repo, initialScopeName) {
    this.repo = repo;
    this.initialScopeName = initialScopeName;
    this.seenFullScopeRequests.add(this.initialScopeName);
    this.Q = [new TopLevelRuleReference(this.initialScopeName)];
  }
  seenFullScopeRequests = /* @__PURE__ */ new Set();
  seenPartialScopeRequests = /* @__PURE__ */ new Set();
  Q;
  processQueue() {
    const q = this.Q;
    this.Q = [];
    const deps = new ExternalReferenceCollector();
    for (const dep of q) {
      collectReferencesOfReference(dep, this.initialScopeName, this.repo, deps);
    }
    for (const dep of deps.references) {
      if (dep instanceof TopLevelRuleReference) {
        if (this.seenFullScopeRequests.has(dep.scopeName)) {
          continue;
        }
        this.seenFullScopeRequests.add(dep.scopeName);
        this.Q.push(dep);
      } else {
        if (this.seenFullScopeRequests.has(dep.scopeName)) {
          continue;
        }
        if (this.seenPartialScopeRequests.has(dep.toKey())) {
          continue;
        }
        this.seenPartialScopeRequests.add(dep.toKey());
        this.Q.push(dep);
      }
    }
  }
};
function collectReferencesOfReference(reference, baseGrammarScopeName, repo, result) {
  const selfGrammar = repo.lookup(reference.scopeName);
  if (!selfGrammar) {
    if (reference.scopeName === baseGrammarScopeName) {
      throw new Error(`No grammar provided for <${baseGrammarScopeName}>`);
    }
    return;
  }
  const baseGrammar = repo.lookup(baseGrammarScopeName);
  if (reference instanceof TopLevelRuleReference) {
    collectExternalReferencesInTopLevelRule({ baseGrammar, selfGrammar }, result);
  } else {
    collectExternalReferencesInTopLevelRepositoryRule(
      reference.ruleName,
      { baseGrammar, selfGrammar, repository: selfGrammar.repository },
      result
    );
  }
  const injections = repo.injections(reference.scopeName);
  if (injections) {
    for (const injection of injections) {
      result.add(new TopLevelRuleReference(injection));
    }
  }
}
function collectExternalReferencesInTopLevelRepositoryRule(ruleName, context, result) {
  if (context.repository && context.repository[ruleName]) {
    const rule = context.repository[ruleName];
    collectExternalReferencesInRules([rule], context, result);
  }
}
function collectExternalReferencesInTopLevelRule(context, result) {
  if (context.selfGrammar.patterns && Array.isArray(context.selfGrammar.patterns)) {
    collectExternalReferencesInRules(
      context.selfGrammar.patterns,
      { ...context, repository: context.selfGrammar.repository },
      result
    );
  }
  if (context.selfGrammar.injections) {
    collectExternalReferencesInRules(
      Object.values(context.selfGrammar.injections),
      { ...context, repository: context.selfGrammar.repository },
      result
    );
  }
}
function collectExternalReferencesInRules(rules, context, result) {
  for (const rule of rules) {
    if (result.visitedRule.has(rule)) {
      continue;
    }
    result.visitedRule.add(rule);
    const patternRepository = rule.repository ? mergeObjects({}, context.repository, rule.repository) : context.repository;
    if (Array.isArray(rule.patterns)) {
      collectExternalReferencesInRules(rule.patterns, { ...context, repository: patternRepository }, result);
    }
    const include = rule.include;
    if (!include) {
      continue;
    }
    const reference = parseInclude(include);
    switch (reference.kind) {
      case 0:
        collectExternalReferencesInTopLevelRule({ ...context, selfGrammar: context.baseGrammar }, result);
        break;
      case 1:
        collectExternalReferencesInTopLevelRule(context, result);
        break;
      case 2:
        collectExternalReferencesInTopLevelRepositoryRule(reference.ruleName, { ...context, repository: patternRepository }, result);
        break;
      case 3:
      case 4:
        const selfGrammar = reference.scopeName === context.selfGrammar.scopeName ? context.selfGrammar : reference.scopeName === context.baseGrammar.scopeName ? context.baseGrammar : undefined;
        if (selfGrammar) {
          const newContext = { baseGrammar: context.baseGrammar, selfGrammar, repository: patternRepository };
          if (reference.kind === 4) {
            collectExternalReferencesInTopLevelRepositoryRule(reference.ruleName, newContext, result);
          } else {
            collectExternalReferencesInTopLevelRule(newContext, result);
          }
        } else {
          if (reference.kind === 4) {
            result.add(new TopLevelRepositoryRuleReference(reference.scopeName, reference.ruleName));
          } else {
            result.add(new TopLevelRuleReference(reference.scopeName));
          }
        }
        break;
    }
  }
}
var BaseReference = class {
  kind = 0;
};
var SelfReference = class {
  kind = 1;
};
var RelativeReference = class {
  constructor(ruleName) {
    this.ruleName = ruleName;
  }
  kind = 2;
};
var TopLevelReference = class {
  constructor(scopeName) {
    this.scopeName = scopeName;
  }
  kind = 3;
};
var TopLevelRepositoryReference = class {
  constructor(scopeName, ruleName) {
    this.scopeName = scopeName;
    this.ruleName = ruleName;
  }
  kind = 4;
};
function parseInclude(include) {
  if (include === "$base") {
    return new BaseReference();
  } else if (include === "$self") {
    return new SelfReference();
  }
  const indexOfSharp = include.indexOf("#");
  if (indexOfSharp === -1) {
    return new TopLevelReference(include);
  } else if (indexOfSharp === 0) {
    return new RelativeReference(include.substring(1));
  } else {
    const scopeName = include.substring(0, indexOfSharp);
    const ruleName = include.substring(indexOfSharp + 1);
    return new TopLevelRepositoryReference(scopeName, ruleName);
  }
}
var HAS_BACK_REFERENCES = /\\(\d+)/;
var BACK_REFERENCING_END = /\\(\d+)/g;
var endRuleId = -1;
var whileRuleId = -2;
function ruleIdFromNumber(id) {
  return id;
}
function ruleIdToNumber(id) {
  return id;
}
var Rule = class {
  $location;
  id;
  _nameIsCapturing;
  _name;
  _contentNameIsCapturing;
  _contentName;
  constructor($location, id, name, contentName) {
    this.$location = $location;
    this.id = id;
    this._name = name || null;
    this._nameIsCapturing = RegexSource.hasCaptures(this._name);
    this._contentName = contentName || null;
    this._contentNameIsCapturing = RegexSource.hasCaptures(this._contentName);
  }
  get debugName() {
    const location = this.$location ? `${basename(this.$location.filename)}:${this.$location.line}` : "unknown";
    return `${this.constructor.name}#${this.id} @ ${location}`;
  }
  getName(lineText, captureIndices) {
    if (!this._nameIsCapturing || this._name === null || lineText === null || captureIndices === null) {
      return this._name;
    }
    return RegexSource.replaceCaptures(this._name, lineText, captureIndices);
  }
  getContentName(lineText, captureIndices) {
    if (!this._contentNameIsCapturing || this._contentName === null) {
      return this._contentName;
    }
    return RegexSource.replaceCaptures(this._contentName, lineText, captureIndices);
  }
};
var CaptureRule = class extends Rule {
  retokenizeCapturedWithRuleId;
  constructor($location, id, name, contentName, retokenizeCapturedWithRuleId) {
    super($location, id, name, contentName);
    this.retokenizeCapturedWithRuleId = retokenizeCapturedWithRuleId;
  }
  dispose() {
  }
  collectPatterns(grammar, out) {
    throw new Error("Not supported!");
  }
  compile(grammar, endRegexSource) {
    throw new Error("Not supported!");
  }
  compileAG(grammar, endRegexSource, allowA, allowG) {
    throw new Error("Not supported!");
  }
};
var MatchRule = class extends Rule {
  _match;
  captures;
  _cachedCompiledPatterns;
  constructor($location, id, name, match, captures) {
    super($location, id, name, null);
    this._match = new RegExpSource(match, this.id);
    this.captures = captures;
    this._cachedCompiledPatterns = null;
  }
  dispose() {
    if (this._cachedCompiledPatterns) {
      this._cachedCompiledPatterns.dispose();
      this._cachedCompiledPatterns = null;
    }
  }
  get debugMatchRegExp() {
    return `${this._match.source}`;
  }
  collectPatterns(grammar, out) {
    out.push(this._match);
  }
  compile(grammar, endRegexSource) {
    return this._getCachedCompiledPatterns(grammar).compile(grammar);
  }
  compileAG(grammar, endRegexSource, allowA, allowG) {
    return this._getCachedCompiledPatterns(grammar).compileAG(grammar, allowA, allowG);
  }
  _getCachedCompiledPatterns(grammar) {
    if (!this._cachedCompiledPatterns) {
      this._cachedCompiledPatterns = new RegExpSourceList();
      this.collectPatterns(grammar, this._cachedCompiledPatterns);
    }
    return this._cachedCompiledPatterns;
  }
};
var IncludeOnlyRule = class extends Rule {
  hasMissingPatterns;
  patterns;
  _cachedCompiledPatterns;
  constructor($location, id, name, contentName, patterns) {
    super($location, id, name, contentName);
    this.patterns = patterns.patterns;
    this.hasMissingPatterns = patterns.hasMissingPatterns;
    this._cachedCompiledPatterns = null;
  }
  dispose() {
    if (this._cachedCompiledPatterns) {
      this._cachedCompiledPatterns.dispose();
      this._cachedCompiledPatterns = null;
    }
  }
  collectPatterns(grammar, out) {
    for (const pattern of this.patterns) {
      const rule = grammar.getRule(pattern);
      rule.collectPatterns(grammar, out);
    }
  }
  compile(grammar, endRegexSource) {
    return this._getCachedCompiledPatterns(grammar).compile(grammar);
  }
  compileAG(grammar, endRegexSource, allowA, allowG) {
    return this._getCachedCompiledPatterns(grammar).compileAG(grammar, allowA, allowG);
  }
  _getCachedCompiledPatterns(grammar) {
    if (!this._cachedCompiledPatterns) {
      this._cachedCompiledPatterns = new RegExpSourceList();
      this.collectPatterns(grammar, this._cachedCompiledPatterns);
    }
    return this._cachedCompiledPatterns;
  }
};
var BeginEndRule = class extends Rule {
  _begin;
  beginCaptures;
  _end;
  endHasBackReferences;
  endCaptures;
  applyEndPatternLast;
  hasMissingPatterns;
  patterns;
  _cachedCompiledPatterns;
  constructor($location, id, name, contentName, begin, beginCaptures, end, endCaptures, applyEndPatternLast, patterns) {
    super($location, id, name, contentName);
    this._begin = new RegExpSource(begin, this.id);
    this.beginCaptures = beginCaptures;
    this._end = new RegExpSource(end ? end : "￿", -1);
    this.endHasBackReferences = this._end.hasBackReferences;
    this.endCaptures = endCaptures;
    this.applyEndPatternLast = applyEndPatternLast || false;
    this.patterns = patterns.patterns;
    this.hasMissingPatterns = patterns.hasMissingPatterns;
    this._cachedCompiledPatterns = null;
  }
  dispose() {
    if (this._cachedCompiledPatterns) {
      this._cachedCompiledPatterns.dispose();
      this._cachedCompiledPatterns = null;
    }
  }
  get debugBeginRegExp() {
    return `${this._begin.source}`;
  }
  get debugEndRegExp() {
    return `${this._end.source}`;
  }
  getEndWithResolvedBackReferences(lineText, captureIndices) {
    return this._end.resolveBackReferences(lineText, captureIndices);
  }
  collectPatterns(grammar, out) {
    out.push(this._begin);
  }
  compile(grammar, endRegexSource) {
    return this._getCachedCompiledPatterns(grammar, endRegexSource).compile(grammar);
  }
  compileAG(grammar, endRegexSource, allowA, allowG) {
    return this._getCachedCompiledPatterns(grammar, endRegexSource).compileAG(grammar, allowA, allowG);
  }
  _getCachedCompiledPatterns(grammar, endRegexSource) {
    if (!this._cachedCompiledPatterns) {
      this._cachedCompiledPatterns = new RegExpSourceList();
      for (const pattern of this.patterns) {
        const rule = grammar.getRule(pattern);
        rule.collectPatterns(grammar, this._cachedCompiledPatterns);
      }
      if (this.applyEndPatternLast) {
        this._cachedCompiledPatterns.push(this._end.hasBackReferences ? this._end.clone() : this._end);
      } else {
        this._cachedCompiledPatterns.unshift(this._end.hasBackReferences ? this._end.clone() : this._end);
      }
    }
    if (this._end.hasBackReferences) {
      if (this.applyEndPatternLast) {
        this._cachedCompiledPatterns.setSource(this._cachedCompiledPatterns.length() - 1, endRegexSource);
      } else {
        this._cachedCompiledPatterns.setSource(0, endRegexSource);
      }
    }
    return this._cachedCompiledPatterns;
  }
};
var BeginWhileRule = class extends Rule {
  _begin;
  beginCaptures;
  whileCaptures;
  _while;
  whileHasBackReferences;
  hasMissingPatterns;
  patterns;
  _cachedCompiledPatterns;
  _cachedCompiledWhilePatterns;
  constructor($location, id, name, contentName, begin, beginCaptures, _while, whileCaptures, patterns) {
    super($location, id, name, contentName);
    this._begin = new RegExpSource(begin, this.id);
    this.beginCaptures = beginCaptures;
    this.whileCaptures = whileCaptures;
    this._while = new RegExpSource(_while, whileRuleId);
    this.whileHasBackReferences = this._while.hasBackReferences;
    this.patterns = patterns.patterns;
    this.hasMissingPatterns = patterns.hasMissingPatterns;
    this._cachedCompiledPatterns = null;
    this._cachedCompiledWhilePatterns = null;
  }
  dispose() {
    if (this._cachedCompiledPatterns) {
      this._cachedCompiledPatterns.dispose();
      this._cachedCompiledPatterns = null;
    }
    if (this._cachedCompiledWhilePatterns) {
      this._cachedCompiledWhilePatterns.dispose();
      this._cachedCompiledWhilePatterns = null;
    }
  }
  get debugBeginRegExp() {
    return `${this._begin.source}`;
  }
  get debugWhileRegExp() {
    return `${this._while.source}`;
  }
  getWhileWithResolvedBackReferences(lineText, captureIndices) {
    return this._while.resolveBackReferences(lineText, captureIndices);
  }
  collectPatterns(grammar, out) {
    out.push(this._begin);
  }
  compile(grammar, endRegexSource) {
    return this._getCachedCompiledPatterns(grammar).compile(grammar);
  }
  compileAG(grammar, endRegexSource, allowA, allowG) {
    return this._getCachedCompiledPatterns(grammar).compileAG(grammar, allowA, allowG);
  }
  _getCachedCompiledPatterns(grammar) {
    if (!this._cachedCompiledPatterns) {
      this._cachedCompiledPatterns = new RegExpSourceList();
      for (const pattern of this.patterns) {
        const rule = grammar.getRule(pattern);
        rule.collectPatterns(grammar, this._cachedCompiledPatterns);
      }
    }
    return this._cachedCompiledPatterns;
  }
  compileWhile(grammar, endRegexSource) {
    return this._getCachedCompiledWhilePatterns(grammar, endRegexSource).compile(grammar);
  }
  compileWhileAG(grammar, endRegexSource, allowA, allowG) {
    return this._getCachedCompiledWhilePatterns(grammar, endRegexSource).compileAG(grammar, allowA, allowG);
  }
  _getCachedCompiledWhilePatterns(grammar, endRegexSource) {
    if (!this._cachedCompiledWhilePatterns) {
      this._cachedCompiledWhilePatterns = new RegExpSourceList();
      this._cachedCompiledWhilePatterns.push(this._while.hasBackReferences ? this._while.clone() : this._while);
    }
    if (this._while.hasBackReferences) {
      this._cachedCompiledWhilePatterns.setSource(0, endRegexSource ? endRegexSource : "￿");
    }
    return this._cachedCompiledWhilePatterns;
  }
};
var RuleFactory = class _RuleFactory {
  static createCaptureRule(helper, $location, name, contentName, retokenizeCapturedWithRuleId) {
    return helper.registerRule((id) => {
      return new CaptureRule($location, id, name, contentName, retokenizeCapturedWithRuleId);
    });
  }
  static getCompiledRuleId(desc, helper, repository) {
    if (!desc.id) {
      helper.registerRule((id) => {
        desc.id = id;
        if (desc.match) {
          return new MatchRule(
            desc.$vscodeTextmateLocation,
            desc.id,
            desc.name,
            desc.match,
            _RuleFactory._compileCaptures(desc.captures, helper, repository)
          );
        }
        if (typeof desc.begin === "undefined") {
          if (desc.repository) {
            repository = mergeObjects({}, repository, desc.repository);
          }
          let patterns = desc.patterns;
          if (typeof patterns === "undefined" && desc.include) {
            patterns = [{ include: desc.include }];
          }
          return new IncludeOnlyRule(
            desc.$vscodeTextmateLocation,
            desc.id,
            desc.name,
            desc.contentName,
            _RuleFactory._compilePatterns(patterns, helper, repository)
          );
        }
        if (desc.while) {
          return new BeginWhileRule(
            desc.$vscodeTextmateLocation,
            desc.id,
            desc.name,
            desc.contentName,
            desc.begin,
            _RuleFactory._compileCaptures(desc.beginCaptures || desc.captures, helper, repository),
            desc.while,
            _RuleFactory._compileCaptures(desc.whileCaptures || desc.captures, helper, repository),
            _RuleFactory._compilePatterns(desc.patterns, helper, repository)
          );
        }
        return new BeginEndRule(
          desc.$vscodeTextmateLocation,
          desc.id,
          desc.name,
          desc.contentName,
          desc.begin,
          _RuleFactory._compileCaptures(desc.beginCaptures || desc.captures, helper, repository),
          desc.end,
          _RuleFactory._compileCaptures(desc.endCaptures || desc.captures, helper, repository),
          desc.applyEndPatternLast,
          _RuleFactory._compilePatterns(desc.patterns, helper, repository)
        );
      });
    }
    return desc.id;
  }
  static _compileCaptures(captures, helper, repository) {
    let r = [];
    if (captures) {
      let maximumCaptureId = 0;
      for (const captureId in captures) {
        if (captureId === "$vscodeTextmateLocation") {
          continue;
        }
        const numericCaptureId = parseInt(captureId, 10);
        if (numericCaptureId > maximumCaptureId) {
          maximumCaptureId = numericCaptureId;
        }
      }
      for (let i = 0; i <= maximumCaptureId; i++) {
        r[i] = null;
      }
      for (const captureId in captures) {
        if (captureId === "$vscodeTextmateLocation") {
          continue;
        }
        const numericCaptureId = parseInt(captureId, 10);
        let retokenizeCapturedWithRuleId = 0;
        if (captures[captureId].patterns) {
          retokenizeCapturedWithRuleId = _RuleFactory.getCompiledRuleId(captures[captureId], helper, repository);
        }
        r[numericCaptureId] = _RuleFactory.createCaptureRule(helper, captures[captureId].$vscodeTextmateLocation, captures[captureId].name, captures[captureId].contentName, retokenizeCapturedWithRuleId);
      }
    }
    return r;
  }
  static _compilePatterns(patterns, helper, repository) {
    let r = [];
    if (patterns) {
      for (let i = 0, len = patterns.length; i < len; i++) {
        const pattern = patterns[i];
        let ruleId = -1;
        if (pattern.include) {
          const reference = parseInclude(pattern.include);
          switch (reference.kind) {
            case 0:
            case 1:
              ruleId = _RuleFactory.getCompiledRuleId(repository[pattern.include], helper, repository);
              break;
            case 2:
              let localIncludedRule = repository[reference.ruleName];
              if (localIncludedRule) {
                ruleId = _RuleFactory.getCompiledRuleId(localIncludedRule, helper, repository);
              }
              break;
            case 3:
            case 4:
              const externalGrammarName = reference.scopeName;
              const externalGrammarInclude = reference.kind === 4 ? reference.ruleName : null;
              const externalGrammar = helper.getExternalGrammar(externalGrammarName, repository);
              if (externalGrammar) {
                if (externalGrammarInclude) {
                  let externalIncludedRule = externalGrammar.repository[externalGrammarInclude];
                  if (externalIncludedRule) {
                    ruleId = _RuleFactory.getCompiledRuleId(externalIncludedRule, helper, externalGrammar.repository);
                  }
                } else {
                  ruleId = _RuleFactory.getCompiledRuleId(externalGrammar.repository.$self, helper, externalGrammar.repository);
                }
              }
              break;
          }
        } else {
          ruleId = _RuleFactory.getCompiledRuleId(pattern, helper, repository);
        }
        if (ruleId !== -1) {
          const rule = helper.getRule(ruleId);
          let skipRule = false;
          if (rule instanceof IncludeOnlyRule || rule instanceof BeginEndRule || rule instanceof BeginWhileRule) {
            if (rule.hasMissingPatterns && rule.patterns.length === 0) {
              skipRule = true;
            }
          }
          if (skipRule) {
            continue;
          }
          r.push(ruleId);
        }
      }
    }
    return {
      patterns: r,
      hasMissingPatterns: (patterns ? patterns.length : 0) !== r.length
    };
  }
};
var RegExpSource = class _RegExpSource {
  source;
  ruleId;
  hasAnchor;
  hasBackReferences;
  _anchorCache;
  constructor(regExpSource, ruleId) {
    if (regExpSource && typeof regExpSource === "string") {
      const len = regExpSource.length;
      let lastPushedPos = 0;
      let output = [];
      let hasAnchor = false;
      for (let pos = 0; pos < len; pos++) {
        const ch = regExpSource.charAt(pos);
        if (ch === "\\") {
          if (pos + 1 < len) {
            const nextCh = regExpSource.charAt(pos + 1);
            if (nextCh === "z") {
              output.push(regExpSource.substring(lastPushedPos, pos));
              output.push("$(?!\\n)(?<!\\n)");
              lastPushedPos = pos + 2;
            } else if (nextCh === "A" || nextCh === "G") {
              hasAnchor = true;
            }
            pos++;
          }
        }
      }
      this.hasAnchor = hasAnchor;
      if (lastPushedPos === 0) {
        this.source = regExpSource;
      } else {
        output.push(regExpSource.substring(lastPushedPos, len));
        this.source = output.join("");
      }
    } else {
      this.hasAnchor = false;
      this.source = regExpSource;
    }
    if (this.hasAnchor) {
      this._anchorCache = this._buildAnchorCache();
    } else {
      this._anchorCache = null;
    }
    this.ruleId = ruleId;
    if (typeof this.source === "string") {
      this.hasBackReferences = HAS_BACK_REFERENCES.test(this.source);
    } else {
      this.hasBackReferences = false;
    }
  }
  clone() {
    return new _RegExpSource(this.source, this.ruleId);
  }
  setSource(newSource) {
    if (this.source === newSource) {
      return;
    }
    this.source = newSource;
    if (this.hasAnchor) {
      this._anchorCache = this._buildAnchorCache();
    }
  }
  resolveBackReferences(lineText, captureIndices) {
    if (typeof this.source !== "string") {
      throw new Error("This method should only be called if the source is a string");
    }
    let capturedValues = captureIndices.map((capture) => {
      return lineText.substring(capture.start, capture.end);
    });
    BACK_REFERENCING_END.lastIndex = 0;
    return this.source.replace(BACK_REFERENCING_END, (match, g1) => {
      return escapeRegExpCharacters(capturedValues[parseInt(g1, 10)] || "");
    });
  }
  _buildAnchorCache() {
    if (typeof this.source !== "string") {
      throw new Error("This method should only be called if the source is a string");
    }
    let A0_G0_result = [];
    let A0_G1_result = [];
    let A1_G0_result = [];
    let A1_G1_result = [];
    let pos, len, ch, nextCh;
    for (pos = 0, len = this.source.length; pos < len; pos++) {
      ch = this.source.charAt(pos);
      A0_G0_result[pos] = ch;
      A0_G1_result[pos] = ch;
      A1_G0_result[pos] = ch;
      A1_G1_result[pos] = ch;
      if (ch === "\\") {
        if (pos + 1 < len) {
          nextCh = this.source.charAt(pos + 1);
          if (nextCh === "A") {
            A0_G0_result[pos + 1] = "￿";
            A0_G1_result[pos + 1] = "￿";
            A1_G0_result[pos + 1] = "A";
            A1_G1_result[pos + 1] = "A";
          } else if (nextCh === "G") {
            A0_G0_result[pos + 1] = "￿";
            A0_G1_result[pos + 1] = "G";
            A1_G0_result[pos + 1] = "￿";
            A1_G1_result[pos + 1] = "G";
          } else {
            A0_G0_result[pos + 1] = nextCh;
            A0_G1_result[pos + 1] = nextCh;
            A1_G0_result[pos + 1] = nextCh;
            A1_G1_result[pos + 1] = nextCh;
          }
          pos++;
        }
      }
    }
    return {
      A0_G0: A0_G0_result.join(""),
      A0_G1: A0_G1_result.join(""),
      A1_G0: A1_G0_result.join(""),
      A1_G1: A1_G1_result.join("")
    };
  }
  resolveAnchors(allowA, allowG) {
    if (!this.hasAnchor || !this._anchorCache || typeof this.source !== "string") {
      return this.source;
    }
    if (allowA) {
      if (allowG) {
        return this._anchorCache.A1_G1;
      } else {
        return this._anchorCache.A1_G0;
      }
    } else {
      if (allowG) {
        return this._anchorCache.A0_G1;
      } else {
        return this._anchorCache.A0_G0;
      }
    }
  }
};
var RegExpSourceList = class {
  _items;
  _hasAnchors;
  _cached;
  _anchorCache;
  constructor() {
    this._items = [];
    this._hasAnchors = false;
    this._cached = null;
    this._anchorCache = {
      A0_G0: null,
      A0_G1: null,
      A1_G0: null,
      A1_G1: null
    };
  }
  dispose() {
    this._disposeCaches();
  }
  _disposeCaches() {
    if (this._cached) {
      this._cached.dispose();
      this._cached = null;
    }
    if (this._anchorCache.A0_G0) {
      this._anchorCache.A0_G0.dispose();
      this._anchorCache.A0_G0 = null;
    }
    if (this._anchorCache.A0_G1) {
      this._anchorCache.A0_G1.dispose();
      this._anchorCache.A0_G1 = null;
    }
    if (this._anchorCache.A1_G0) {
      this._anchorCache.A1_G0.dispose();
      this._anchorCache.A1_G0 = null;
    }
    if (this._anchorCache.A1_G1) {
      this._anchorCache.A1_G1.dispose();
      this._anchorCache.A1_G1 = null;
    }
  }
  push(item) {
    this._items.push(item);
    this._hasAnchors = this._hasAnchors || item.hasAnchor;
  }
  unshift(item) {
    this._items.unshift(item);
    this._hasAnchors = this._hasAnchors || item.hasAnchor;
  }
  length() {
    return this._items.length;
  }
  setSource(index, newSource) {
    if (this._items[index].source !== newSource) {
      this._disposeCaches();
      this._items[index].setSource(newSource);
    }
  }
  compile(onigLib) {
    if (!this._cached) {
      let regExps = this._items.map((e) => e.source);
      this._cached = new CompiledRule(onigLib, regExps, this._items.map((e) => e.ruleId));
    }
    return this._cached;
  }
  compileAG(onigLib, allowA, allowG) {
    if (!this._hasAnchors) {
      return this.compile(onigLib);
    } else {
      if (allowA) {
        if (allowG) {
          if (!this._anchorCache.A1_G1) {
            this._anchorCache.A1_G1 = this._resolveAnchors(onigLib, allowA, allowG);
          }
          return this._anchorCache.A1_G1;
        } else {
          if (!this._anchorCache.A1_G0) {
            this._anchorCache.A1_G0 = this._resolveAnchors(onigLib, allowA, allowG);
          }
          return this._anchorCache.A1_G0;
        }
      } else {
        if (allowG) {
          if (!this._anchorCache.A0_G1) {
            this._anchorCache.A0_G1 = this._resolveAnchors(onigLib, allowA, allowG);
          }
          return this._anchorCache.A0_G1;
        } else {
          if (!this._anchorCache.A0_G0) {
            this._anchorCache.A0_G0 = this._resolveAnchors(onigLib, allowA, allowG);
          }
          return this._anchorCache.A0_G0;
        }
      }
    }
  }
  _resolveAnchors(onigLib, allowA, allowG) {
    let regExps = this._items.map((e) => e.resolveAnchors(allowA, allowG));
    return new CompiledRule(onigLib, regExps, this._items.map((e) => e.ruleId));
  }
};
var CompiledRule = class {
  constructor(onigLib, regExps, rules) {
    this.regExps = regExps;
    this.rules = rules;
    this.scanner = onigLib.createOnigScanner(regExps);
  }
  scanner;
  dispose() {
    if (typeof this.scanner.dispose === "function") {
      this.scanner.dispose();
    }
  }
  toString() {
    const r = [];
    for (let i = 0, len = this.rules.length; i < len; i++) {
      r.push("   - " + this.rules[i] + ": " + this.regExps[i]);
    }
    return r.join("\n");
  }
  findNextMatchSync(string, startPosition, options) {
    const result = this.scanner.findNextMatchSync(string, startPosition, options);
    if (!result) {
      return null;
    }
    return {
      ruleId: this.rules[result.index],
      captureIndices: result.captureIndices
    };
  }
};
var BasicScopeAttributes = class {
  constructor(languageId, tokenType) {
    this.languageId = languageId;
    this.tokenType = tokenType;
  }
};
var BasicScopeAttributesProvider = class _BasicScopeAttributesProvider {
  _defaultAttributes;
  _embeddedLanguagesMatcher;
  constructor(initialLanguageId, embeddedLanguages) {
    this._defaultAttributes = new BasicScopeAttributes(
      initialLanguageId,
      8
      /* NotSet */
    );
    this._embeddedLanguagesMatcher = new ScopeMatcher(Object.entries(embeddedLanguages || {}));
  }
  getDefaultAttributes() {
    return this._defaultAttributes;
  }
  getBasicScopeAttributes(scopeName) {
    if (scopeName === null) {
      return _BasicScopeAttributesProvider._NULL_SCOPE_METADATA;
    }
    return this._getBasicScopeAttributes.get(scopeName);
  }
  static _NULL_SCOPE_METADATA = new BasicScopeAttributes(0, 0);
  _getBasicScopeAttributes = new CachedFn((scopeName) => {
    const languageId = this._scopeToLanguage(scopeName);
    const standardTokenType = this._toStandardTokenType(scopeName);
    return new BasicScopeAttributes(languageId, standardTokenType);
  });
  /**
   * Given a produced TM scope, return the language that token describes or null if unknown.
   * e.g. source.html => html, source.css.embedded.html => css, punctuation.definition.tag.html => null
   */
  _scopeToLanguage(scope) {
    return this._embeddedLanguagesMatcher.match(scope) || 0;
  }
  _toStandardTokenType(scopeName) {
    const m = scopeName.match(_BasicScopeAttributesProvider.STANDARD_TOKEN_TYPE_REGEXP);
    if (!m) {
      return 8;
    }
    switch (m[1]) {
      case "comment":
        return 1;
      case "string":
        return 2;
      case "regex":
        return 3;
      case "meta.embedded":
        return 0;
    }
    throw new Error("Unexpected match for standard token type!");
  }
  static STANDARD_TOKEN_TYPE_REGEXP = /\b(comment|string|regex|meta\.embedded)\b/;
};
var ScopeMatcher = class {
  values;
  scopesRegExp;
  constructor(values) {
    if (values.length === 0) {
      this.values = null;
      this.scopesRegExp = null;
    } else {
      this.values = new Map(values);
      const escapedScopes = values.map(
        ([scopeName, value]) => escapeRegExpCharacters(scopeName)
      );
      escapedScopes.sort();
      escapedScopes.reverse();
      this.scopesRegExp = new RegExp(
        `^((${escapedScopes.join(")|(")}))($|\\.)`,
        ""
      );
    }
  }
  match(scope) {
    if (!this.scopesRegExp) {
      return undefined;
    }
    const m = scope.match(this.scopesRegExp);
    if (!m) {
      return undefined;
    }
    return this.values.get(m[1]);
  }
};
var TokenizeStringResult = class {
  constructor(stack, stoppedEarly) {
    this.stack = stack;
    this.stoppedEarly = stoppedEarly;
  }
};
function _tokenizeString(grammar, lineText, isFirstLine, linePos, stack, lineTokens, checkWhileConditions, timeLimit) {
  const lineLength = lineText.content.length;
  let STOP = false;
  let anchorPosition = -1;
  if (checkWhileConditions) {
    const whileCheckResult = _checkWhileConditions(
      grammar,
      lineText,
      isFirstLine,
      linePos,
      stack,
      lineTokens
    );
    stack = whileCheckResult.stack;
    linePos = whileCheckResult.linePos;
    isFirstLine = whileCheckResult.isFirstLine;
    anchorPosition = whileCheckResult.anchorPosition;
  }
  const startTime = Date.now();
  while (!STOP) {
    if (timeLimit !== 0) {
      const elapsedTime = Date.now() - startTime;
      if (elapsedTime > timeLimit) {
        return new TokenizeStringResult(stack, true);
      }
    }
    scanNext();
  }
  return new TokenizeStringResult(stack, false);
  function scanNext() {
    const r = matchRuleOrInjections(
      grammar,
      lineText,
      isFirstLine,
      linePos,
      stack,
      anchorPosition
    );
    if (!r) {
      lineTokens.produce(stack, lineLength);
      STOP = true;
      return;
    }
    const captureIndices = r.captureIndices;
    const matchedRuleId = r.matchedRuleId;
    const hasAdvanced = captureIndices && captureIndices.length > 0 ? captureIndices[0].end > linePos : false;
    if (matchedRuleId === endRuleId) {
      const poppedRule = stack.getRule(grammar);
      lineTokens.produce(stack, captureIndices[0].start);
      stack = stack.withContentNameScopesList(stack.nameScopesList);
      handleCaptures(
        grammar,
        lineText,
        isFirstLine,
        stack,
        lineTokens,
        poppedRule.endCaptures,
        captureIndices
      );
      lineTokens.produce(stack, captureIndices[0].end);
      const popped = stack;
      stack = stack.parent;
      anchorPosition = popped.getAnchorPos();
      if (!hasAdvanced && popped.getEnterPos() === linePos) {
        stack = popped;
        lineTokens.produce(stack, lineLength);
        STOP = true;
        return;
      }
    } else {
      const _rule = grammar.getRule(matchedRuleId);
      lineTokens.produce(stack, captureIndices[0].start);
      const beforePush = stack;
      const scopeName = _rule.getName(lineText.content, captureIndices);
      const nameScopesList = stack.contentNameScopesList.pushAttributed(
        scopeName,
        grammar
      );
      stack = stack.push(
        matchedRuleId,
        linePos,
        anchorPosition,
        captureIndices[0].end === lineLength,
        null,
        nameScopesList,
        nameScopesList
      );
      if (_rule instanceof BeginEndRule) {
        const pushedRule = _rule;
        handleCaptures(
          grammar,
          lineText,
          isFirstLine,
          stack,
          lineTokens,
          pushedRule.beginCaptures,
          captureIndices
        );
        lineTokens.produce(stack, captureIndices[0].end);
        anchorPosition = captureIndices[0].end;
        const contentName = pushedRule.getContentName(
          lineText.content,
          captureIndices
        );
        const contentNameScopesList = nameScopesList.pushAttributed(
          contentName,
          grammar
        );
        stack = stack.withContentNameScopesList(contentNameScopesList);
        if (pushedRule.endHasBackReferences) {
          stack = stack.withEndRule(
            pushedRule.getEndWithResolvedBackReferences(
              lineText.content,
              captureIndices
            )
          );
        }
        if (!hasAdvanced && beforePush.hasSameRuleAs(stack)) {
          stack = stack.pop();
          lineTokens.produce(stack, lineLength);
          STOP = true;
          return;
        }
      } else if (_rule instanceof BeginWhileRule) {
        const pushedRule = _rule;
        handleCaptures(
          grammar,
          lineText,
          isFirstLine,
          stack,
          lineTokens,
          pushedRule.beginCaptures,
          captureIndices
        );
        lineTokens.produce(stack, captureIndices[0].end);
        anchorPosition = captureIndices[0].end;
        const contentName = pushedRule.getContentName(
          lineText.content,
          captureIndices
        );
        const contentNameScopesList = nameScopesList.pushAttributed(
          contentName,
          grammar
        );
        stack = stack.withContentNameScopesList(contentNameScopesList);
        if (pushedRule.whileHasBackReferences) {
          stack = stack.withEndRule(
            pushedRule.getWhileWithResolvedBackReferences(
              lineText.content,
              captureIndices
            )
          );
        }
        if (!hasAdvanced && beforePush.hasSameRuleAs(stack)) {
          stack = stack.pop();
          lineTokens.produce(stack, lineLength);
          STOP = true;
          return;
        }
      } else {
        const matchingRule = _rule;
        handleCaptures(
          grammar,
          lineText,
          isFirstLine,
          stack,
          lineTokens,
          matchingRule.captures,
          captureIndices
        );
        lineTokens.produce(stack, captureIndices[0].end);
        stack = stack.pop();
        if (!hasAdvanced) {
          stack = stack.safePop();
          lineTokens.produce(stack, lineLength);
          STOP = true;
          return;
        }
      }
    }
    if (captureIndices[0].end > linePos) {
      linePos = captureIndices[0].end;
      isFirstLine = false;
    }
  }
}
function _checkWhileConditions(grammar, lineText, isFirstLine, linePos, stack, lineTokens) {
  let anchorPosition = stack.beginRuleCapturedEOL ? 0 : -1;
  const whileRules = [];
  for (let node = stack; node; node = node.pop()) {
    const nodeRule = node.getRule(grammar);
    if (nodeRule instanceof BeginWhileRule) {
      whileRules.push({
        rule: nodeRule,
        stack: node
      });
    }
  }
  for (let whileRule = whileRules.pop(); whileRule; whileRule = whileRules.pop()) {
    const { ruleScanner, findOptions } = prepareRuleWhileSearch(whileRule.rule, grammar, whileRule.stack.endRule, isFirstLine, linePos === anchorPosition);
    const r = ruleScanner.findNextMatchSync(lineText, linePos, findOptions);
    if (r) {
      const matchedRuleId = r.ruleId;
      if (matchedRuleId !== whileRuleId) {
        stack = whileRule.stack.pop();
        break;
      }
      if (r.captureIndices && r.captureIndices.length) {
        lineTokens.produce(whileRule.stack, r.captureIndices[0].start);
        handleCaptures(grammar, lineText, isFirstLine, whileRule.stack, lineTokens, whileRule.rule.whileCaptures, r.captureIndices);
        lineTokens.produce(whileRule.stack, r.captureIndices[0].end);
        anchorPosition = r.captureIndices[0].end;
        if (r.captureIndices[0].end > linePos) {
          linePos = r.captureIndices[0].end;
          isFirstLine = false;
        }
      }
    } else {
      stack = whileRule.stack.pop();
      break;
    }
  }
  return { stack, linePos, anchorPosition, isFirstLine };
}
function matchRuleOrInjections(grammar, lineText, isFirstLine, linePos, stack, anchorPosition) {
  const matchResult = matchRule(grammar, lineText, isFirstLine, linePos, stack, anchorPosition);
  const injections = grammar.getInjections();
  if (injections.length === 0) {
    return matchResult;
  }
  const injectionResult = matchInjections(injections, grammar, lineText, isFirstLine, linePos, stack, anchorPosition);
  if (!injectionResult) {
    return matchResult;
  }
  if (!matchResult) {
    return injectionResult;
  }
  const matchResultScore = matchResult.captureIndices[0].start;
  const injectionResultScore = injectionResult.captureIndices[0].start;
  if (injectionResultScore < matchResultScore || injectionResult.priorityMatch && injectionResultScore === matchResultScore) {
    return injectionResult;
  }
  return matchResult;
}
function matchRule(grammar, lineText, isFirstLine, linePos, stack, anchorPosition) {
  const rule = stack.getRule(grammar);
  const { ruleScanner, findOptions } = prepareRuleSearch(rule, grammar, stack.endRule, isFirstLine, linePos === anchorPosition);
  const r = ruleScanner.findNextMatchSync(lineText, linePos, findOptions);
  if (r) {
    return {
      captureIndices: r.captureIndices,
      matchedRuleId: r.ruleId
    };
  }
  return null;
}
function matchInjections(injections, grammar, lineText, isFirstLine, linePos, stack, anchorPosition) {
  let bestMatchRating = Number.MAX_VALUE;
  let bestMatchCaptureIndices = null;
  let bestMatchRuleId;
  let bestMatchResultPriority = 0;
  const scopes = stack.contentNameScopesList.getScopeNames();
  for (let i = 0, len = injections.length; i < len; i++) {
    const injection = injections[i];
    if (!injection.matcher(scopes)) {
      continue;
    }
    const rule = grammar.getRule(injection.ruleId);
    const { ruleScanner, findOptions } = prepareRuleSearch(rule, grammar, null, isFirstLine, linePos === anchorPosition);
    const matchResult = ruleScanner.findNextMatchSync(lineText, linePos, findOptions);
    if (!matchResult) {
      continue;
    }
    const matchRating = matchResult.captureIndices[0].start;
    if (matchRating >= bestMatchRating) {
      continue;
    }
    bestMatchRating = matchRating;
    bestMatchCaptureIndices = matchResult.captureIndices;
    bestMatchRuleId = matchResult.ruleId;
    bestMatchResultPriority = injection.priority;
    if (bestMatchRating === linePos) {
      break;
    }
  }
  if (bestMatchCaptureIndices) {
    return {
      priorityMatch: bestMatchResultPriority === -1,
      captureIndices: bestMatchCaptureIndices,
      matchedRuleId: bestMatchRuleId
    };
  }
  return null;
}
function prepareRuleSearch(rule, grammar, endRegexSource, allowA, allowG) {
  const ruleScanner = rule.compileAG(grammar, endRegexSource, allowA, allowG);
  return {
    ruleScanner,
    findOptions: 0
    /* None */
  };
}
function prepareRuleWhileSearch(rule, grammar, endRegexSource, allowA, allowG) {
  const ruleScanner = rule.compileWhileAG(grammar, endRegexSource, allowA, allowG);
  return {
    ruleScanner,
    findOptions: 0
    /* None */
  };
}
function handleCaptures(grammar, lineText, isFirstLine, stack, lineTokens, captures, captureIndices) {
  if (captures.length === 0) {
    return;
  }
  const lineTextContent = lineText.content;
  const len = Math.min(captures.length, captureIndices.length);
  const localStack = [];
  const maxEnd = captureIndices[0].end;
  for (let i = 0; i < len; i++) {
    const captureRule = captures[i];
    if (captureRule === null) {
      continue;
    }
    const captureIndex = captureIndices[i];
    if (captureIndex.length === 0) {
      continue;
    }
    if (captureIndex.start > maxEnd) {
      break;
    }
    while (localStack.length > 0 && localStack[localStack.length - 1].endPos <= captureIndex.start) {
      lineTokens.produceFromScopes(localStack[localStack.length - 1].scopes, localStack[localStack.length - 1].endPos);
      localStack.pop();
    }
    if (localStack.length > 0) {
      lineTokens.produceFromScopes(localStack[localStack.length - 1].scopes, captureIndex.start);
    } else {
      lineTokens.produce(stack, captureIndex.start);
    }
    if (captureRule.retokenizeCapturedWithRuleId) {
      const scopeName = captureRule.getName(lineTextContent, captureIndices);
      const nameScopesList = stack.contentNameScopesList.pushAttributed(scopeName, grammar);
      const contentName = captureRule.getContentName(lineTextContent, captureIndices);
      const contentNameScopesList = nameScopesList.pushAttributed(contentName, grammar);
      const stackClone = stack.push(captureRule.retokenizeCapturedWithRuleId, captureIndex.start, -1, false, null, nameScopesList, contentNameScopesList);
      const onigSubStr = grammar.createOnigString(lineTextContent.substring(0, captureIndex.end));
      _tokenizeString(
        grammar,
        onigSubStr,
        isFirstLine && captureIndex.start === 0,
        captureIndex.start,
        stackClone,
        lineTokens,
        false,
        /* no time limit */
        0
      );
      disposeOnigString(onigSubStr);
      continue;
    }
    const captureRuleScopeName = captureRule.getName(lineTextContent, captureIndices);
    if (captureRuleScopeName !== null) {
      const base = localStack.length > 0 ? localStack[localStack.length - 1].scopes : stack.contentNameScopesList;
      const captureRuleScopesList = base.pushAttributed(captureRuleScopeName, grammar);
      localStack.push(new LocalStackElement(captureRuleScopesList, captureIndex.end));
    }
  }
  while (localStack.length > 0) {
    lineTokens.produceFromScopes(localStack[localStack.length - 1].scopes, localStack[localStack.length - 1].endPos);
    localStack.pop();
  }
}
var LocalStackElement = class {
  scopes;
  endPos;
  constructor(scopes, endPos) {
    this.scopes = scopes;
    this.endPos = endPos;
  }
};
function createGrammar(scopeName, grammar, initialLanguage, embeddedLanguages, tokenTypes, balancedBracketSelectors, grammarRepository, onigLib) {
  return new Grammar(
    scopeName,
    grammar,
    initialLanguage,
    embeddedLanguages,
    tokenTypes,
    balancedBracketSelectors,
    grammarRepository,
    onigLib
  );
}
function collectInjections(result, selector, rule, ruleFactoryHelper, grammar) {
  const matchers = createMatchers(selector, nameMatcher);
  const ruleId = RuleFactory.getCompiledRuleId(rule, ruleFactoryHelper, grammar.repository);
  for (const matcher of matchers) {
    result.push({
      debugSelector: selector,
      matcher: matcher.matcher,
      ruleId,
      grammar,
      priority: matcher.priority
    });
  }
}
function nameMatcher(identifers, scopes) {
  if (scopes.length < identifers.length) {
    return false;
  }
  let lastIndex = 0;
  return identifers.every((identifier) => {
    for (let i = lastIndex; i < scopes.length; i++) {
      if (scopesAreMatching(scopes[i], identifier)) {
        lastIndex = i + 1;
        return true;
      }
    }
    return false;
  });
}
function scopesAreMatching(thisScopeName, scopeName) {
  if (!thisScopeName) {
    return false;
  }
  if (thisScopeName === scopeName) {
    return true;
  }
  const len = scopeName.length;
  return thisScopeName.length > len && thisScopeName.substr(0, len) === scopeName && thisScopeName[len] === ".";
}
var Grammar = class {
  constructor(_rootScopeName, grammar, initialLanguage, embeddedLanguages, tokenTypes, balancedBracketSelectors, grammarRepository, _onigLib) {
    this._rootScopeName = _rootScopeName;
    this.balancedBracketSelectors = balancedBracketSelectors;
    this._onigLib = _onigLib;
    this._basicScopeAttributesProvider = new BasicScopeAttributesProvider(
      initialLanguage,
      embeddedLanguages
    );
    this._rootId = -1;
    this._lastRuleId = 0;
    this._ruleId2desc = [null];
    this._includedGrammars = {};
    this._grammarRepository = grammarRepository;
    this._grammar = initGrammar(grammar, null);
    this._injections = null;
    this._tokenTypeMatchers = [];
    if (tokenTypes) {
      for (const selector of Object.keys(tokenTypes)) {
        const matchers = createMatchers(selector, nameMatcher);
        for (const matcher of matchers) {
          this._tokenTypeMatchers.push({
            matcher: matcher.matcher,
            type: tokenTypes[selector]
          });
        }
      }
    }
  }
  _rootId;
  _lastRuleId;
  _ruleId2desc;
  _includedGrammars;
  _grammarRepository;
  _grammar;
  _injections;
  _basicScopeAttributesProvider;
  _tokenTypeMatchers;
  get themeProvider() {
    return this._grammarRepository;
  }
  dispose() {
    for (const rule of this._ruleId2desc) {
      if (rule) {
        rule.dispose();
      }
    }
  }
  createOnigScanner(sources) {
    return this._onigLib.createOnigScanner(sources);
  }
  createOnigString(sources) {
    return this._onigLib.createOnigString(sources);
  }
  getMetadataForScope(scope) {
    return this._basicScopeAttributesProvider.getBasicScopeAttributes(scope);
  }
  _collectInjections() {
    const grammarRepository = {
      lookup: (scopeName2) => {
        if (scopeName2 === this._rootScopeName) {
          return this._grammar;
        }
        return this.getExternalGrammar(scopeName2);
      },
      injections: (scopeName2) => {
        return this._grammarRepository.injections(scopeName2);
      }
    };
    const result = [];
    const scopeName = this._rootScopeName;
    const grammar = grammarRepository.lookup(scopeName);
    if (grammar) {
      const rawInjections = grammar.injections;
      if (rawInjections) {
        for (let expression in rawInjections) {
          collectInjections(
            result,
            expression,
            rawInjections[expression],
            this,
            grammar
          );
        }
      }
      const injectionScopeNames = this._grammarRepository.injections(scopeName);
      if (injectionScopeNames) {
        injectionScopeNames.forEach((injectionScopeName) => {
          const injectionGrammar = this.getExternalGrammar(injectionScopeName);
          if (injectionGrammar) {
            const selector = injectionGrammar.injectionSelector;
            if (selector) {
              collectInjections(
                result,
                selector,
                injectionGrammar,
                this,
                injectionGrammar
              );
            }
          }
        });
      }
    }
    result.sort((i1, i2) => i1.priority - i2.priority);
    return result;
  }
  getInjections() {
    if (this._injections === null) {
      this._injections = this._collectInjections();
    }
    return this._injections;
  }
  registerRule(factory) {
    const id = ++this._lastRuleId;
    const result = factory(ruleIdFromNumber(id));
    this._ruleId2desc[id] = result;
    return result;
  }
  getRule(ruleId) {
    return this._ruleId2desc[ruleIdToNumber(ruleId)];
  }
  getExternalGrammar(scopeName, repository) {
    if (this._includedGrammars[scopeName]) {
      return this._includedGrammars[scopeName];
    } else if (this._grammarRepository) {
      const rawIncludedGrammar = this._grammarRepository.lookup(scopeName);
      if (rawIncludedGrammar) {
        this._includedGrammars[scopeName] = initGrammar(
          rawIncludedGrammar,
          repository && repository.$base
        );
        return this._includedGrammars[scopeName];
      }
    }
    return undefined;
  }
  tokenizeLine(lineText, prevState, timeLimit = 0) {
    const r = this._tokenize(lineText, prevState, false, timeLimit);
    return {
      tokens: r.lineTokens.getResult(r.ruleStack, r.lineLength),
      ruleStack: r.ruleStack,
      stoppedEarly: r.stoppedEarly
    };
  }
  tokenizeLine2(lineText, prevState, timeLimit = 0) {
    const r = this._tokenize(lineText, prevState, true, timeLimit);
    return {
      tokens: r.lineTokens.getBinaryResult(r.ruleStack, r.lineLength),
      ruleStack: r.ruleStack,
      stoppedEarly: r.stoppedEarly
    };
  }
  _tokenize(lineText, prevState, emitBinaryTokens, timeLimit) {
    if (this._rootId === -1) {
      this._rootId = RuleFactory.getCompiledRuleId(
        this._grammar.repository.$self,
        this,
        this._grammar.repository
      );
      this.getInjections();
    }
    let isFirstLine;
    if (!prevState || prevState === StateStackImpl.NULL) {
      isFirstLine = true;
      const rawDefaultMetadata = this._basicScopeAttributesProvider.getDefaultAttributes();
      const defaultStyle = this.themeProvider.getDefaults();
      const defaultMetadata = EncodedTokenMetadata.set(
        0,
        rawDefaultMetadata.languageId,
        rawDefaultMetadata.tokenType,
        null,
        defaultStyle.fontStyle,
        defaultStyle.foregroundId,
        defaultStyle.backgroundId
      );
      const rootScopeName = this.getRule(this._rootId).getName(
        null,
        null
      );
      let scopeList;
      if (rootScopeName) {
        scopeList = AttributedScopeStack.createRootAndLookUpScopeName(
          rootScopeName,
          defaultMetadata,
          this
        );
      } else {
        scopeList = AttributedScopeStack.createRoot(
          "unknown",
          defaultMetadata
        );
      }
      prevState = new StateStackImpl(
        null,
        this._rootId,
        -1,
        -1,
        false,
        null,
        scopeList,
        scopeList
      );
    } else {
      isFirstLine = false;
      prevState.reset();
    }
    lineText = lineText + "\n";
    const onigLineText = this.createOnigString(lineText);
    const lineLength = onigLineText.content.length;
    const lineTokens = new LineTokens(
      emitBinaryTokens,
      lineText,
      this._tokenTypeMatchers,
      this.balancedBracketSelectors
    );
    const r = _tokenizeString(
      this,
      onigLineText,
      isFirstLine,
      0,
      prevState,
      lineTokens,
      true,
      timeLimit
    );
    disposeOnigString(onigLineText);
    return {
      lineLength,
      lineTokens,
      ruleStack: r.stack,
      stoppedEarly: r.stoppedEarly
    };
  }
};
function initGrammar(grammar, base) {
  grammar = clone(grammar);
  grammar.repository = grammar.repository || {};
  grammar.repository.$self = {
    $vscodeTextmateLocation: grammar.$vscodeTextmateLocation,
    patterns: grammar.patterns,
    name: grammar.scopeName
  };
  grammar.repository.$base = base || grammar.repository.$self;
  return grammar;
}
var AttributedScopeStack = class _AttributedScopeStack {
  /**
   * Invariant:
   * ```
   * if (parent && !scopePath.extends(parent.scopePath)) {
   * 	throw new Error();
   * }
   * ```
   */
  constructor(parent, scopePath, tokenAttributes) {
    this.parent = parent;
    this.scopePath = scopePath;
    this.tokenAttributes = tokenAttributes;
  }
  static fromExtension(namesScopeList, contentNameScopesList) {
    let current = namesScopeList;
    let scopeNames = namesScopeList?.scopePath ?? null;
    for (const frame of contentNameScopesList) {
      scopeNames = ScopeStack.push(scopeNames, frame.scopeNames);
      current = new _AttributedScopeStack(current, scopeNames, frame.encodedTokenAttributes);
    }
    return current;
  }
  static createRoot(scopeName, tokenAttributes) {
    return new _AttributedScopeStack(null, new ScopeStack(null, scopeName), tokenAttributes);
  }
  static createRootAndLookUpScopeName(scopeName, tokenAttributes, grammar) {
    const rawRootMetadata = grammar.getMetadataForScope(scopeName);
    const scopePath = new ScopeStack(null, scopeName);
    const rootStyle = grammar.themeProvider.themeMatch(scopePath);
    const resolvedTokenAttributes = _AttributedScopeStack.mergeAttributes(
      tokenAttributes,
      rawRootMetadata,
      rootStyle
    );
    return new _AttributedScopeStack(null, scopePath, resolvedTokenAttributes);
  }
  get scopeName() {
    return this.scopePath.scopeName;
  }
  toString() {
    return this.getScopeNames().join(" ");
  }
  equals(other) {
    return _AttributedScopeStack.equals(this, other);
  }
  static equals(a, b) {
    do {
      if (a === b) {
        return true;
      }
      if (!a && !b) {
        return true;
      }
      if (!a || !b) {
        return false;
      }
      if (a.scopeName !== b.scopeName || a.tokenAttributes !== b.tokenAttributes) {
        return false;
      }
      a = a.parent;
      b = b.parent;
    } while (true);
  }
  static mergeAttributes(existingTokenAttributes, basicScopeAttributes, styleAttributes) {
    let fontStyle = -1;
    let foreground = 0;
    let background = 0;
    if (styleAttributes !== null) {
      fontStyle = styleAttributes.fontStyle;
      foreground = styleAttributes.foregroundId;
      background = styleAttributes.backgroundId;
    }
    return EncodedTokenMetadata.set(
      existingTokenAttributes,
      basicScopeAttributes.languageId,
      basicScopeAttributes.tokenType,
      null,
      fontStyle,
      foreground,
      background
    );
  }
  pushAttributed(scopePath, grammar) {
    if (scopePath === null) {
      return this;
    }
    if (scopePath.indexOf(" ") === -1) {
      return _AttributedScopeStack._pushAttributed(this, scopePath, grammar);
    }
    const scopes = scopePath.split(/ /g);
    let result = this;
    for (const scope of scopes) {
      result = _AttributedScopeStack._pushAttributed(result, scope, grammar);
    }
    return result;
  }
  static _pushAttributed(target, scopeName, grammar) {
    const rawMetadata = grammar.getMetadataForScope(scopeName);
    const newPath = target.scopePath.push(scopeName);
    const scopeThemeMatchResult = grammar.themeProvider.themeMatch(newPath);
    const metadata = _AttributedScopeStack.mergeAttributes(
      target.tokenAttributes,
      rawMetadata,
      scopeThemeMatchResult
    );
    return new _AttributedScopeStack(target, newPath, metadata);
  }
  getScopeNames() {
    return this.scopePath.getSegments();
  }
  getExtensionIfDefined(base) {
    const result = [];
    let self = this;
    while (self && self !== base) {
      result.push({
        encodedTokenAttributes: self.tokenAttributes,
        scopeNames: self.scopePath.getExtensionIfDefined(self.parent?.scopePath ?? null)
      });
      self = self.parent;
    }
    return self === base ? result.reverse() : undefined;
  }
};
var StateStackImpl = class _StateStackImpl {
  /**
   * Invariant:
   * ```
   * if (contentNameScopesList !== nameScopesList && contentNameScopesList?.parent !== nameScopesList) {
   * 	throw new Error();
   * }
   * if (this.parent && !nameScopesList.extends(this.parent.contentNameScopesList)) {
   * 	throw new Error();
   * }
   * ```
   */
  constructor(parent, ruleId, enterPos, anchorPos, beginRuleCapturedEOL, endRule, nameScopesList, contentNameScopesList) {
    this.parent = parent;
    this.ruleId = ruleId;
    this.beginRuleCapturedEOL = beginRuleCapturedEOL;
    this.endRule = endRule;
    this.nameScopesList = nameScopesList;
    this.contentNameScopesList = contentNameScopesList;
    this.depth = this.parent ? this.parent.depth + 1 : 1;
    this._enterPos = enterPos;
    this._anchorPos = anchorPos;
  }
  _stackElementBrand = undefined;
  // TODO remove me
  static NULL = new _StateStackImpl(
    null,
    0,
    0,
    0,
    false,
    null,
    null,
    null
  );
  /**
   * The position on the current line where this state was pushed.
   * This is relevant only while tokenizing a line, to detect endless loops.
   * Its value is meaningless across lines.
   */
  _enterPos;
  /**
   * The captured anchor position when this stack element was pushed.
   * This is relevant only while tokenizing a line, to restore the anchor position when popping.
   * Its value is meaningless across lines.
   */
  _anchorPos;
  /**
   * The depth of the stack.
   */
  depth;
  equals(other) {
    if (other === null) {
      return false;
    }
    return _StateStackImpl._equals(this, other);
  }
  static _equals(a, b) {
    if (a === b) {
      return true;
    }
    if (!this._structuralEquals(a, b)) {
      return false;
    }
    return AttributedScopeStack.equals(a.contentNameScopesList, b.contentNameScopesList);
  }
  /**
   * A structural equals check. Does not take into account `scopes`.
   */
  static _structuralEquals(a, b) {
    do {
      if (a === b) {
        return true;
      }
      if (!a && !b) {
        return true;
      }
      if (!a || !b) {
        return false;
      }
      if (a.depth !== b.depth || a.ruleId !== b.ruleId || a.endRule !== b.endRule) {
        return false;
      }
      a = a.parent;
      b = b.parent;
    } while (true);
  }
  clone() {
    return this;
  }
  static _reset(el) {
    while (el) {
      el._enterPos = -1;
      el._anchorPos = -1;
      el = el.parent;
    }
  }
  reset() {
    _StateStackImpl._reset(this);
  }
  pop() {
    return this.parent;
  }
  safePop() {
    if (this.parent) {
      return this.parent;
    }
    return this;
  }
  push(ruleId, enterPos, anchorPos, beginRuleCapturedEOL, endRule, nameScopesList, contentNameScopesList) {
    return new _StateStackImpl(
      this,
      ruleId,
      enterPos,
      anchorPos,
      beginRuleCapturedEOL,
      endRule,
      nameScopesList,
      contentNameScopesList
    );
  }
  getEnterPos() {
    return this._enterPos;
  }
  getAnchorPos() {
    return this._anchorPos;
  }
  getRule(grammar) {
    return grammar.getRule(this.ruleId);
  }
  toString() {
    const r = [];
    this._writeString(r, 0);
    return "[" + r.join(",") + "]";
  }
  _writeString(res, outIndex) {
    if (this.parent) {
      outIndex = this.parent._writeString(res, outIndex);
    }
    res[outIndex++] = `(${this.ruleId}, ${this.nameScopesList?.toString()}, ${this.contentNameScopesList?.toString()})`;
    return outIndex;
  }
  withContentNameScopesList(contentNameScopeStack) {
    if (this.contentNameScopesList === contentNameScopeStack) {
      return this;
    }
    return this.parent.push(
      this.ruleId,
      this._enterPos,
      this._anchorPos,
      this.beginRuleCapturedEOL,
      this.endRule,
      this.nameScopesList,
      contentNameScopeStack
    );
  }
  withEndRule(endRule) {
    if (this.endRule === endRule) {
      return this;
    }
    return new _StateStackImpl(
      this.parent,
      this.ruleId,
      this._enterPos,
      this._anchorPos,
      this.beginRuleCapturedEOL,
      endRule,
      this.nameScopesList,
      this.contentNameScopesList
    );
  }
  // Used to warn of endless loops
  hasSameRuleAs(other) {
    let el = this;
    while (el && el._enterPos === other._enterPos) {
      if (el.ruleId === other.ruleId) {
        return true;
      }
      el = el.parent;
    }
    return false;
  }
  toStateStackFrame() {
    return {
      ruleId: ruleIdToNumber(this.ruleId),
      beginRuleCapturedEOL: this.beginRuleCapturedEOL,
      endRule: this.endRule,
      nameScopesList: this.nameScopesList?.getExtensionIfDefined(this.parent?.nameScopesList ?? null) ?? [],
      contentNameScopesList: this.contentNameScopesList?.getExtensionIfDefined(this.nameScopesList) ?? []
    };
  }
  static pushFrame(self, frame) {
    const namesScopeList = AttributedScopeStack.fromExtension(self?.nameScopesList ?? null, frame.nameScopesList);
    return new _StateStackImpl(
      self,
      ruleIdFromNumber(frame.ruleId),
      frame.enterPos ?? -1,
      frame.anchorPos ?? -1,
      frame.beginRuleCapturedEOL,
      frame.endRule,
      namesScopeList,
      AttributedScopeStack.fromExtension(namesScopeList, frame.contentNameScopesList)
    );
  }
};
var BalancedBracketSelectors = class {
  balancedBracketScopes;
  unbalancedBracketScopes;
  allowAny = false;
  constructor(balancedBracketScopes, unbalancedBracketScopes) {
    this.balancedBracketScopes = balancedBracketScopes.flatMap(
      (selector) => {
        if (selector === "*") {
          this.allowAny = true;
          return [];
        }
        return createMatchers(selector, nameMatcher).map((m) => m.matcher);
      }
    );
    this.unbalancedBracketScopes = unbalancedBracketScopes.flatMap(
      (selector) => createMatchers(selector, nameMatcher).map((m) => m.matcher)
    );
  }
  get matchesAlways() {
    return this.allowAny && this.unbalancedBracketScopes.length === 0;
  }
  get matchesNever() {
    return this.balancedBracketScopes.length === 0 && !this.allowAny;
  }
  match(scopes) {
    for (const excluder of this.unbalancedBracketScopes) {
      if (excluder(scopes)) {
        return false;
      }
    }
    for (const includer of this.balancedBracketScopes) {
      if (includer(scopes)) {
        return true;
      }
    }
    return this.allowAny;
  }
};
var LineTokens = class {
  constructor(emitBinaryTokens, lineText, tokenTypeOverrides, balancedBracketSelectors) {
    this.balancedBracketSelectors = balancedBracketSelectors;
    this._emitBinaryTokens = emitBinaryTokens;
    this._tokenTypeOverrides = tokenTypeOverrides;
    {
      this._lineText = null;
    }
    this._tokens = [];
    this._binaryTokens = [];
    this._lastTokenEndIndex = 0;
  }
  _emitBinaryTokens;
  /**
   * defined only if `false`.
   */
  _lineText;
  /**
   * used only if `_emitBinaryTokens` is false.
   */
  _tokens;
  /**
   * used only if `_emitBinaryTokens` is true.
   */
  _binaryTokens;
  _lastTokenEndIndex;
  _tokenTypeOverrides;
  produce(stack, endIndex) {
    this.produceFromScopes(stack.contentNameScopesList, endIndex);
  }
  produceFromScopes(scopesList, endIndex) {
    if (this._lastTokenEndIndex >= endIndex) {
      return;
    }
    if (this._emitBinaryTokens) {
      let metadata = scopesList?.tokenAttributes ?? 0;
      let containsBalancedBrackets = false;
      if (this.balancedBracketSelectors?.matchesAlways) {
        containsBalancedBrackets = true;
      }
      if (this._tokenTypeOverrides.length > 0 || this.balancedBracketSelectors && !this.balancedBracketSelectors.matchesAlways && !this.balancedBracketSelectors.matchesNever) {
        const scopes2 = scopesList?.getScopeNames() ?? [];
        for (const tokenType of this._tokenTypeOverrides) {
          if (tokenType.matcher(scopes2)) {
            metadata = EncodedTokenMetadata.set(
              metadata,
              0,
              toOptionalTokenType(tokenType.type),
              null,
              -1,
              0,
              0
            );
          }
        }
        if (this.balancedBracketSelectors) {
          containsBalancedBrackets = this.balancedBracketSelectors.match(scopes2);
        }
      }
      if (containsBalancedBrackets) {
        metadata = EncodedTokenMetadata.set(
          metadata,
          0,
          8,
          containsBalancedBrackets,
          -1,
          0,
          0
        );
      }
      if (this._binaryTokens.length > 0 && this._binaryTokens[this._binaryTokens.length - 1] === metadata) {
        this._lastTokenEndIndex = endIndex;
        return;
      }
      this._binaryTokens.push(this._lastTokenEndIndex);
      this._binaryTokens.push(metadata);
      this._lastTokenEndIndex = endIndex;
      return;
    }
    const scopes = scopesList?.getScopeNames() ?? [];
    this._tokens.push({
      startIndex: this._lastTokenEndIndex,
      endIndex,
      // value: lineText.substring(lastTokenEndIndex, endIndex),
      scopes
    });
    this._lastTokenEndIndex = endIndex;
  }
  getResult(stack, lineLength) {
    if (this._tokens.length > 0 && this._tokens[this._tokens.length - 1].startIndex === lineLength - 1) {
      this._tokens.pop();
    }
    if (this._tokens.length === 0) {
      this._lastTokenEndIndex = -1;
      this.produce(stack, lineLength);
      this._tokens[this._tokens.length - 1].startIndex = 0;
    }
    return this._tokens;
  }
  getBinaryResult(stack, lineLength) {
    if (this._binaryTokens.length > 0 && this._binaryTokens[this._binaryTokens.length - 2] === lineLength - 1) {
      this._binaryTokens.pop();
      this._binaryTokens.pop();
    }
    if (this._binaryTokens.length === 0) {
      this._lastTokenEndIndex = -1;
      this.produce(stack, lineLength);
      this._binaryTokens[this._binaryTokens.length - 2] = 0;
    }
    const result = new Uint32Array(this._binaryTokens.length);
    for (let i = 0, len = this._binaryTokens.length; i < len; i++) {
      result[i] = this._binaryTokens[i];
    }
    return result;
  }
};
var SyncRegistry = class {
  constructor(theme, _onigLib) {
    this._onigLib = _onigLib;
    this._theme = theme;
  }
  _grammars = /* @__PURE__ */ new Map();
  _rawGrammars = /* @__PURE__ */ new Map();
  _injectionGrammars = /* @__PURE__ */ new Map();
  _theme;
  dispose() {
    for (const grammar of this._grammars.values()) {
      grammar.dispose();
    }
  }
  setTheme(theme) {
    this._theme = theme;
  }
  getColorMap() {
    return this._theme.getColorMap();
  }
  /**
   * Add `grammar` to registry and return a list of referenced scope names
   */
  addGrammar(grammar, injectionScopeNames) {
    this._rawGrammars.set(grammar.scopeName, grammar);
    if (injectionScopeNames) {
      this._injectionGrammars.set(grammar.scopeName, injectionScopeNames);
    }
  }
  /**
   * Lookup a raw grammar.
   */
  lookup(scopeName) {
    return this._rawGrammars.get(scopeName);
  }
  /**
   * Returns the injections for the given grammar
   */
  injections(targetScope) {
    return this._injectionGrammars.get(targetScope);
  }
  /**
   * Get the default theme settings
   */
  getDefaults() {
    return this._theme.getDefaults();
  }
  /**
   * Match a scope in the theme.
   */
  themeMatch(scopePath) {
    return this._theme.match(scopePath);
  }
  /**
   * Lookup a grammar.
   */
  grammarForScopeName(scopeName, initialLanguage, embeddedLanguages, tokenTypes, balancedBracketSelectors) {
    if (!this._grammars.has(scopeName)) {
      let rawGrammar = this._rawGrammars.get(scopeName);
      if (!rawGrammar) {
        return null;
      }
      this._grammars.set(scopeName, createGrammar(
        scopeName,
        rawGrammar,
        initialLanguage,
        embeddedLanguages,
        tokenTypes,
        balancedBracketSelectors,
        this,
        this._onigLib
      ));
    }
    return this._grammars.get(scopeName);
  }
};
var Registry$1 = class Registry {
  _options;
  _syncRegistry;
  _ensureGrammarCache;
  constructor(options) {
    this._options = options;
    this._syncRegistry = new SyncRegistry(
      Theme.createFromRawTheme(options.theme, options.colorMap),
      options.onigLib
    );
    this._ensureGrammarCache = /* @__PURE__ */ new Map();
  }
  dispose() {
    this._syncRegistry.dispose();
  }
  /**
   * Change the theme. Once called, no previous `ruleStack` should be used anymore.
   */
  setTheme(theme, colorMap) {
    this._syncRegistry.setTheme(Theme.createFromRawTheme(theme, colorMap));
  }
  /**
   * Returns a lookup array for color ids.
   */
  getColorMap() {
    return this._syncRegistry.getColorMap();
  }
  /**
   * Load the grammar for `scopeName` and all referenced included grammars asynchronously.
   * Please do not use language id 0.
   */
  loadGrammarWithEmbeddedLanguages(initialScopeName, initialLanguage, embeddedLanguages) {
    return this.loadGrammarWithConfiguration(initialScopeName, initialLanguage, { embeddedLanguages });
  }
  /**
   * Load the grammar for `scopeName` and all referenced included grammars asynchronously.
   * Please do not use language id 0.
   */
  loadGrammarWithConfiguration(initialScopeName, initialLanguage, configuration) {
    return this._loadGrammar(
      initialScopeName,
      initialLanguage,
      configuration.embeddedLanguages,
      configuration.tokenTypes,
      new BalancedBracketSelectors(
        configuration.balancedBracketSelectors || [],
        configuration.unbalancedBracketSelectors || []
      )
    );
  }
  /**
   * Load the grammar for `scopeName` and all referenced included grammars asynchronously.
   */
  loadGrammar(initialScopeName) {
    return this._loadGrammar(initialScopeName, 0, null, null, null);
  }
  _loadGrammar(initialScopeName, initialLanguage, embeddedLanguages, tokenTypes, balancedBracketSelectors) {
    const dependencyProcessor = new ScopeDependencyProcessor(this._syncRegistry, initialScopeName);
    while (dependencyProcessor.Q.length > 0) {
      dependencyProcessor.Q.map((request) => this._loadSingleGrammar(request.scopeName));
      dependencyProcessor.processQueue();
    }
    return this._grammarForScopeName(
      initialScopeName,
      initialLanguage,
      embeddedLanguages,
      tokenTypes,
      balancedBracketSelectors
    );
  }
  _loadSingleGrammar(scopeName) {
    if (!this._ensureGrammarCache.has(scopeName)) {
      this._doLoadSingleGrammar(scopeName);
      this._ensureGrammarCache.set(scopeName, true);
    }
  }
  _doLoadSingleGrammar(scopeName) {
    const grammar = this._options.loadGrammar(scopeName);
    if (grammar) {
      const injections = typeof this._options.getInjections === "function" ? this._options.getInjections(scopeName) : undefined;
      this._syncRegistry.addGrammar(grammar, injections);
    }
  }
  /**
   * Adds a rawGrammar.
   */
  addGrammar(rawGrammar, injections = [], initialLanguage = 0, embeddedLanguages = null) {
    this._syncRegistry.addGrammar(rawGrammar, injections);
    return this._grammarForScopeName(rawGrammar.scopeName, initialLanguage, embeddedLanguages);
  }
  /**
   * Get the grammar for `scopeName`. The grammar must first be created via `loadGrammar` or `addGrammar`.
   */
  _grammarForScopeName(scopeName, initialLanguage = 0, embeddedLanguages = null, tokenTypes = null, balancedBracketSelectors = null) {
    return this._syncRegistry.grammarForScopeName(
      scopeName,
      initialLanguage,
      embeddedLanguages,
      tokenTypes,
      balancedBracketSelectors
    );
  }
};
var INITIAL = StateStackImpl.NULL;

function toArray(x) {
  return Array.isArray(x) ? x : [x];
}
function splitLines(code, preserveEnding = false) {
  const parts = code.split(/(\r?\n)/g);
  let index = 0;
  const lines = [];
  for (let i = 0; i < parts.length; i += 2) {
    const line = preserveEnding ? parts[i] + (parts[i + 1] || "") : parts[i];
    lines.push([line, index]);
    index += parts[i].length;
    index += parts[i + 1]?.length || 0;
  }
  return lines;
}
function isPlainLang(lang) {
  return !lang || ["plaintext", "txt", "text", "plain"].includes(lang);
}
function isSpecialLang(lang) {
  return lang === "ansi" || isPlainLang(lang);
}
function isNoneTheme(theme) {
  return theme === "none";
}
function isSpecialTheme(theme) {
  return isNoneTheme(theme);
}
function addClassToHast(node, className) {
  if (!className)
    return node;
  node.properties ||= {};
  node.properties.class ||= [];
  if (typeof node.properties.class === "string")
    node.properties.class = node.properties.class.split(/\s+/g);
  if (!Array.isArray(node.properties.class))
    node.properties.class = [];
  const targets = Array.isArray(className) ? className : className.split(/\s+/g);
  for (const c of targets) {
    if (c && !node.properties.class.includes(c))
      node.properties.class.push(c);
  }
  return node;
}
function splitToken(token, offsets) {
  let lastOffset = 0;
  const tokens = [];
  for (const offset of offsets) {
    if (offset > lastOffset) {
      tokens.push({
        ...token,
        content: token.content.slice(lastOffset, offset),
        offset: token.offset + lastOffset
      });
    }
    lastOffset = offset;
  }
  if (lastOffset < token.content.length) {
    tokens.push({
      ...token,
      content: token.content.slice(lastOffset),
      offset: token.offset + lastOffset
    });
  }
  return tokens;
}
function splitTokens(tokens, breakpoints) {
  const sorted = Array.from(breakpoints instanceof Set ? breakpoints : new Set(breakpoints)).sort((a, b) => a - b);
  if (!sorted.length)
    return tokens;
  return tokens.map((line) => {
    return line.flatMap((token) => {
      const breakpointsInToken = sorted.filter((i) => token.offset < i && i < token.offset + token.content.length).map((i) => i - token.offset).sort((a, b) => a - b);
      if (!breakpointsInToken.length)
        return token;
      return splitToken(token, breakpointsInToken);
    });
  });
}
async function normalizeGetter(p) {
  return Promise.resolve(typeof p === "function" ? p() : p).then((r) => r.default || r);
}
function resolveColorReplacements(theme, options) {
  const replacements = typeof theme === "string" ? {} : { ...theme.colorReplacements };
  const themeName = typeof theme === "string" ? theme : theme.name;
  for (const [key, value] of Object.entries(options?.colorReplacements || {})) {
    if (typeof value === "string")
      replacements[key] = value;
    else if (key === themeName)
      Object.assign(replacements, value);
  }
  return replacements;
}
function applyColorReplacements(color, replacements) {
  if (!color)
    return color;
  return replacements?.[color?.toLowerCase()] || color;
}
function getTokenStyleObject(token) {
  const styles = {};
  if (token.color)
    styles.color = token.color;
  if (token.bgColor)
    styles["background-color"] = token.bgColor;
  if (token.fontStyle) {
    if (token.fontStyle & FontStyle.Italic)
      styles["font-style"] = "italic";
    if (token.fontStyle & FontStyle.Bold)
      styles["font-weight"] = "bold";
    if (token.fontStyle & FontStyle.Underline)
      styles["text-decoration"] = "underline";
  }
  return styles;
}
function stringifyTokenStyle(token) {
  if (typeof token === "string")
    return token;
  return Object.entries(token).map(([key, value]) => `${key}:${value}`).join(";");
}
function createPositionConverter(code) {
  const lines = splitLines(code, true).map(([line]) => line);
  function indexToPos(index) {
    if (index === code.length) {
      return {
        line: lines.length - 1,
        character: lines[lines.length - 1].length
      };
    }
    let character = index;
    let line = 0;
    for (const lineText of lines) {
      if (character < lineText.length)
        break;
      character -= lineText.length;
      line++;
    }
    return { line, character };
  }
  function posToIndex(line, character) {
    let index = 0;
    for (let i = 0; i < line; i++)
      index += lines[i].length;
    index += character;
    return index;
  }
  return {
    lines,
    indexToPos,
    posToIndex
  };
}

class ShikiError extends Error {
  constructor(message) {
    super(message);
    this.name = "ShikiError";
  }
}

const _grammarStateMap = /* @__PURE__ */ new WeakMap();
function setLastGrammarStateToMap(keys, state) {
  _grammarStateMap.set(keys, state);
}
function getLastGrammarStateFromMap(keys) {
  return _grammarStateMap.get(keys);
}
class GrammarState {
  /**
   * Theme to Stack mapping
   */
  _stacks = {};
  lang;
  get themes() {
    return Object.keys(this._stacks);
  }
  get theme() {
    return this.themes[0];
  }
  get _stack() {
    return this._stacks[this.theme];
  }
  /**
   * Static method to create a initial grammar state.
   */
  static initial(lang, themes) {
    return new GrammarState(
      Object.fromEntries(toArray(themes).map((theme) => [theme, INITIAL])),
      lang
    );
  }
  constructor(...args) {
    if (args.length === 2) {
      const [stacksMap, lang] = args;
      this.lang = lang;
      this._stacks = stacksMap;
    } else {
      const [stack, lang, theme] = args;
      this.lang = lang;
      this._stacks = { [theme]: stack };
    }
  }
  /**
   * Get the internal stack object.
   * @internal
   */
  getInternalStack(theme = this.theme) {
    return this._stacks[theme];
  }
  /**
   * @deprecated use `getScopes` instead
   */
  get scopes() {
    return getScopes(this._stacks[this.theme]);
  }
  getScopes(theme = this.theme) {
    return getScopes(this._stacks[theme]);
  }
  toJSON() {
    return {
      lang: this.lang,
      theme: this.theme,
      themes: this.themes,
      scopes: this.scopes
    };
  }
}
function getScopes(stack) {
  const scopes = [];
  const visited = /* @__PURE__ */ new Set();
  function pushScope(stack2) {
    if (visited.has(stack2))
      return;
    visited.add(stack2);
    const name = stack2?.nameScopesList?.scopeName;
    if (name)
      scopes.push(name);
    if (stack2.parent)
      pushScope(stack2.parent);
  }
  pushScope(stack);
  return scopes;
}
function getGrammarStack(state, theme) {
  if (!(state instanceof GrammarState))
    throw new ShikiError("Invalid grammar state");
  return state.getInternalStack(theme);
}

function transformerDecorations() {
  const map = /* @__PURE__ */ new WeakMap();
  function getContext(shiki) {
    if (!map.has(shiki.meta)) {
      let normalizePosition = function(p) {
        if (typeof p === "number") {
          if (p < 0 || p > shiki.source.length)
            throw new ShikiError(`Invalid decoration offset: ${p}. Code length: ${shiki.source.length}`);
          return {
            ...converter.indexToPos(p),
            offset: p
          };
        } else {
          const line = converter.lines[p.line];
          if (line === undefined)
            throw new ShikiError(`Invalid decoration position ${JSON.stringify(p)}. Lines length: ${converter.lines.length}`);
          if (p.character < 0 || p.character > line.length)
            throw new ShikiError(`Invalid decoration position ${JSON.stringify(p)}. Line ${p.line} length: ${line.length}`);
          return {
            ...p,
            offset: converter.posToIndex(p.line, p.character)
          };
        }
      };
      const converter = createPositionConverter(shiki.source);
      const decorations = (shiki.options.decorations || []).map((d) => ({
        ...d,
        start: normalizePosition(d.start),
        end: normalizePosition(d.end)
      }));
      verifyIntersections(decorations);
      map.set(shiki.meta, {
        decorations,
        converter,
        source: shiki.source
      });
    }
    return map.get(shiki.meta);
  }
  return {
    name: "shiki:decorations",
    tokens(tokens) {
      if (!this.options.decorations?.length)
        return;
      const ctx = getContext(this);
      const breakpoints = ctx.decorations.flatMap((d) => [d.start.offset, d.end.offset]);
      const splitted = splitTokens(tokens, breakpoints);
      return splitted;
    },
    code(codeEl) {
      if (!this.options.decorations?.length)
        return;
      const ctx = getContext(this);
      const lines = Array.from(codeEl.children).filter((i) => i.type === "element" && i.tagName === "span");
      if (lines.length !== ctx.converter.lines.length)
        throw new ShikiError(`Number of lines in code element (${lines.length}) does not match the number of lines in the source (${ctx.converter.lines.length}). Failed to apply decorations.`);
      function applyLineSection(line, start, end, decoration) {
        const lineEl = lines[line];
        let text = "";
        let startIndex = -1;
        let endIndex = -1;
        if (start === 0)
          startIndex = 0;
        if (end === 0)
          endIndex = 0;
        if (end === Number.POSITIVE_INFINITY)
          endIndex = lineEl.children.length;
        if (startIndex === -1 || endIndex === -1) {
          for (let i = 0; i < lineEl.children.length; i++) {
            text += stringify(lineEl.children[i]);
            if (startIndex === -1 && text.length === start)
              startIndex = i + 1;
            if (endIndex === -1 && text.length === end)
              endIndex = i + 1;
          }
        }
        if (startIndex === -1)
          throw new ShikiError(`Failed to find start index for decoration ${JSON.stringify(decoration.start)}`);
        if (endIndex === -1)
          throw new ShikiError(`Failed to find end index for decoration ${JSON.stringify(decoration.end)}`);
        const children = lineEl.children.slice(startIndex, endIndex);
        if (!decoration.alwaysWrap && children.length === lineEl.children.length) {
          applyDecoration(lineEl, decoration, "line");
        } else if (!decoration.alwaysWrap && children.length === 1 && children[0].type === "element") {
          applyDecoration(children[0], decoration, "token");
        } else {
          const wrapper = {
            type: "element",
            tagName: "span",
            properties: {},
            children
          };
          applyDecoration(wrapper, decoration, "wrapper");
          lineEl.children.splice(startIndex, children.length, wrapper);
        }
      }
      function applyLine(line, decoration) {
        lines[line] = applyDecoration(lines[line], decoration, "line");
      }
      function applyDecoration(el, decoration, type) {
        const properties = decoration.properties || {};
        const transform = decoration.transform || ((i) => i);
        el.tagName = decoration.tagName || "span";
        el.properties = {
          ...el.properties,
          ...properties,
          class: el.properties.class
        };
        if (decoration.properties?.class)
          addClassToHast(el, decoration.properties.class);
        el = transform(el, type) || el;
        return el;
      }
      const lineApplies = [];
      const sorted = ctx.decorations.sort((a, b) => b.start.offset - a.start.offset);
      for (const decoration of sorted) {
        const { start, end } = decoration;
        if (start.line === end.line) {
          applyLineSection(start.line, start.character, end.character, decoration);
        } else if (start.line < end.line) {
          applyLineSection(start.line, start.character, Number.POSITIVE_INFINITY, decoration);
          for (let i = start.line + 1; i < end.line; i++)
            lineApplies.unshift(() => applyLine(i, decoration));
          applyLineSection(end.line, 0, end.character, decoration);
        }
      }
      lineApplies.forEach((i) => i());
    }
  };
}
function verifyIntersections(items) {
  for (let i = 0; i < items.length; i++) {
    const foo = items[i];
    if (foo.start.offset > foo.end.offset)
      throw new ShikiError(`Invalid decoration range: ${JSON.stringify(foo.start)} - ${JSON.stringify(foo.end)}`);
    for (let j = i + 1; j < items.length; j++) {
      const bar = items[j];
      const isFooHasBarStart = foo.start.offset < bar.start.offset && bar.start.offset < foo.end.offset;
      const isFooHasBarEnd = foo.start.offset < bar.end.offset && bar.end.offset < foo.end.offset;
      const isBarHasFooStart = bar.start.offset < foo.start.offset && foo.start.offset < bar.end.offset;
      const isBarHasFooEnd = bar.start.offset < foo.end.offset && foo.end.offset < bar.end.offset;
      if (isFooHasBarStart || isFooHasBarEnd || isBarHasFooStart || isBarHasFooEnd) {
        if (isFooHasBarEnd && isFooHasBarEnd)
          continue;
        if (isBarHasFooStart && isBarHasFooEnd)
          continue;
        throw new ShikiError(`Decorations ${JSON.stringify(foo.start)} and ${JSON.stringify(bar.start)} intersect.`);
      }
    }
  }
}
function stringify(el) {
  if (el.type === "text")
    return el.value;
  if (el.type === "element")
    return el.children.map(stringify).join("");
  return "";
}

const builtInTransformers = [
  /* @__PURE__ */ transformerDecorations()
];
function getTransformers(options) {
  return [
    ...options.transformers || [],
    ...builtInTransformers
  ];
}

// src/colors.ts
var namedColors = [
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "brightBlack",
  "brightRed",
  "brightGreen",
  "brightYellow",
  "brightBlue",
  "brightMagenta",
  "brightCyan",
  "brightWhite"
];

// src/decorations.ts
var decorations = {
  1: "bold",
  2: "dim",
  3: "italic",
  4: "underline",
  7: "reverse",
  9: "strikethrough"
};

// src/parser.ts
function findSequence(value, position) {
  const nextEscape = value.indexOf("\x1B[", position);
  if (nextEscape !== -1) {
    const nextClose = value.indexOf("m", nextEscape);
    return {
      sequence: value.substring(nextEscape + 2, nextClose).split(";"),
      startPosition: nextEscape,
      position: nextClose + 1
    };
  }
  return {
    position: value.length
  };
}
function parseColor(sequence, index) {
  let offset = 1;
  const colorMode = sequence[index + offset++];
  let color;
  if (colorMode === "2") {
    const rgb = [
      sequence[index + offset++],
      sequence[index + offset++],
      sequence[index + offset]
    ].map((x) => Number.parseInt(x));
    if (rgb.length === 3 && !rgb.some((x) => Number.isNaN(x))) {
      color = {
        type: "rgb",
        rgb
      };
    }
  } else if (colorMode === "5") {
    const colorIndex = Number.parseInt(sequence[index + offset]);
    if (!Number.isNaN(colorIndex)) {
      color = { type: "table", index: Number(colorIndex) };
    }
  }
  return [offset, color];
}
function parseSequence(sequence) {
  const commands = [];
  for (let i = 0; i < sequence.length; i++) {
    const code = sequence[i];
    const codeInt = Number.parseInt(code);
    if (Number.isNaN(codeInt))
      continue;
    if (codeInt === 0) {
      commands.push({ type: "resetAll" });
    } else if (codeInt <= 9) {
      const decoration = decorations[codeInt];
      if (decoration) {
        commands.push({
          type: "setDecoration",
          value: decorations[codeInt]
        });
      }
    } else if (codeInt <= 29) {
      const decoration = decorations[codeInt - 20];
      if (decoration) {
        commands.push({
          type: "resetDecoration",
          value: decoration
        });
      }
    } else if (codeInt <= 37) {
      commands.push({
        type: "setForegroundColor",
        value: { type: "named", name: namedColors[codeInt - 30] }
      });
    } else if (codeInt === 38) {
      const [offset, color] = parseColor(sequence, i);
      if (color) {
        commands.push({
          type: "setForegroundColor",
          value: color
        });
      }
      i += offset;
    } else if (codeInt === 39) {
      commands.push({
        type: "resetForegroundColor"
      });
    } else if (codeInt <= 47) {
      commands.push({
        type: "setBackgroundColor",
        value: { type: "named", name: namedColors[codeInt - 40] }
      });
    } else if (codeInt === 48) {
      const [offset, color] = parseColor(sequence, i);
      if (color) {
        commands.push({
          type: "setBackgroundColor",
          value: color
        });
      }
      i += offset;
    } else if (codeInt === 49) {
      commands.push({
        type: "resetBackgroundColor"
      });
    } else if (codeInt >= 90 && codeInt <= 97) {
      commands.push({
        type: "setForegroundColor",
        value: { type: "named", name: namedColors[codeInt - 90 + 8] }
      });
    } else if (codeInt >= 100 && codeInt <= 107) {
      commands.push({
        type: "setBackgroundColor",
        value: { type: "named", name: namedColors[codeInt - 100 + 8] }
      });
    }
  }
  return commands;
}
function createAnsiSequenceParser() {
  let foreground = null;
  let background = null;
  let decorations2 = /* @__PURE__ */ new Set();
  return {
    parse(value) {
      const tokens = [];
      let position = 0;
      do {
        const findResult = findSequence(value, position);
        const text = findResult.sequence ? value.substring(position, findResult.startPosition) : value.substring(position);
        if (text.length > 0) {
          tokens.push({
            value: text,
            foreground,
            background,
            decorations: new Set(decorations2)
          });
        }
        if (findResult.sequence) {
          const commands = parseSequence(findResult.sequence);
          for (const styleToken of commands) {
            if (styleToken.type === "resetAll") {
              foreground = null;
              background = null;
              decorations2.clear();
            } else if (styleToken.type === "resetForegroundColor") {
              foreground = null;
            } else if (styleToken.type === "resetBackgroundColor") {
              background = null;
            } else if (styleToken.type === "resetDecoration") {
              decorations2.delete(styleToken.value);
            }
          }
          for (const styleToken of commands) {
            if (styleToken.type === "setForegroundColor") {
              foreground = styleToken.value;
            } else if (styleToken.type === "setBackgroundColor") {
              background = styleToken.value;
            } else if (styleToken.type === "setDecoration") {
              decorations2.add(styleToken.value);
            }
          }
        }
        position = findResult.position;
      } while (position < value.length);
      return tokens;
    }
  };
}

// src/palette.ts
var defaultNamedColorsMap = {
  black: "#000000",
  red: "#bb0000",
  green: "#00bb00",
  yellow: "#bbbb00",
  blue: "#0000bb",
  magenta: "#ff00ff",
  cyan: "#00bbbb",
  white: "#eeeeee",
  brightBlack: "#555555",
  brightRed: "#ff5555",
  brightGreen: "#00ff00",
  brightYellow: "#ffff55",
  brightBlue: "#5555ff",
  brightMagenta: "#ff55ff",
  brightCyan: "#55ffff",
  brightWhite: "#ffffff"
};
function createColorPalette(namedColorsMap = defaultNamedColorsMap) {
  function namedColor(name) {
    return namedColorsMap[name];
  }
  function rgbColor(rgb) {
    return `#${rgb.map((x) => Math.max(0, Math.min(x, 255)).toString(16).padStart(2, "0")).join("")}`;
  }
  let colorTable;
  function getColorTable() {
    if (colorTable) {
      return colorTable;
    }
    colorTable = [];
    for (let i = 0; i < namedColors.length; i++) {
      colorTable.push(namedColor(namedColors[i]));
    }
    let levels = [0, 95, 135, 175, 215, 255];
    for (let r = 0; r < 6; r++) {
      for (let g = 0; g < 6; g++) {
        for (let b = 0; b < 6; b++) {
          colorTable.push(rgbColor([levels[r], levels[g], levels[b]]));
        }
      }
    }
    let level = 8;
    for (let i = 0; i < 24; i++, level += 10) {
      colorTable.push(rgbColor([level, level, level]));
    }
    return colorTable;
  }
  function tableColor(index) {
    return getColorTable()[index];
  }
  function value(color) {
    switch (color.type) {
      case "named":
        return namedColor(color.name);
      case "rgb":
        return rgbColor(color.rgb);
      case "table":
        return tableColor(color.index);
    }
  }
  return {
    value
  };
}

function tokenizeAnsiWithTheme(theme, fileContents, options) {
  const colorReplacements = resolveColorReplacements(theme, options);
  const lines = splitLines(fileContents);
  const colorPalette = createColorPalette(
    Object.fromEntries(
      namedColors.map((name) => [
        name,
        theme.colors?.[`terminal.ansi${name[0].toUpperCase()}${name.substring(1)}`]
      ])
    )
  );
  const parser = createAnsiSequenceParser();
  return lines.map(
    (line) => parser.parse(line[0]).map((token) => {
      let color;
      let bgColor;
      if (token.decorations.has("reverse")) {
        color = token.background ? colorPalette.value(token.background) : theme.bg;
        bgColor = token.foreground ? colorPalette.value(token.foreground) : theme.fg;
      } else {
        color = token.foreground ? colorPalette.value(token.foreground) : theme.fg;
        bgColor = token.background ? colorPalette.value(token.background) : undefined;
      }
      color = applyColorReplacements(color, colorReplacements);
      bgColor = applyColorReplacements(bgColor, colorReplacements);
      if (token.decorations.has("dim"))
        color = dimColor(color);
      let fontStyle = FontStyle.None;
      if (token.decorations.has("bold"))
        fontStyle |= FontStyle.Bold;
      if (token.decorations.has("italic"))
        fontStyle |= FontStyle.Italic;
      if (token.decorations.has("underline"))
        fontStyle |= FontStyle.Underline;
      return {
        content: token.value,
        offset: line[1],
        // TODO: more accurate offset? might need to fork ansi-sequence-parser
        color,
        bgColor,
        fontStyle
      };
    })
  );
}
function dimColor(color) {
  const hexMatch = color.match(/#([0-9a-f]{3})([0-9a-f]{3})?([0-9a-f]{2})?/);
  if (hexMatch) {
    if (hexMatch[3]) {
      const alpha = Math.round(Number.parseInt(hexMatch[3], 16) / 2).toString(16).padStart(2, "0");
      return `#${hexMatch[1]}${hexMatch[2]}${alpha}`;
    } else if (hexMatch[2]) {
      return `#${hexMatch[1]}${hexMatch[2]}80`;
    } else {
      return `#${Array.from(hexMatch[1]).map((x) => `${x}${x}`).join("")}80`;
    }
  }
  const cssVarMatch = color.match(/var\((--[\w-]+-ansi-[\w-]+)\)/);
  if (cssVarMatch)
    return `var(${cssVarMatch[1]}-dim)`;
  return color;
}

function codeToTokensBase(internal, code, options = {}) {
  const {
    lang = "text",
    theme: themeName = internal.getLoadedThemes()[0]
  } = options;
  if (isPlainLang(lang) || isNoneTheme(themeName))
    return splitLines(code).map((line) => [{ content: line[0], offset: line[1] }]);
  const { theme, colorMap } = internal.setTheme(themeName);
  if (lang === "ansi")
    return tokenizeAnsiWithTheme(theme, code, options);
  const _grammar = internal.getLanguage(lang);
  if (options.grammarState) {
    if (options.grammarState.lang !== _grammar.name) {
      throw new ShikiError$2(`Grammar state language "${options.grammarState.lang}" does not match highlight language "${_grammar.name}"`);
    }
    if (!options.grammarState.themes.includes(theme.name)) {
      throw new ShikiError$2(`Grammar state themes "${options.grammarState.themes}" do not contain highlight theme "${theme.name}"`);
    }
  }
  return tokenizeWithTheme(code, _grammar, theme, colorMap, options);
}
function getLastGrammarState(...args) {
  if (args.length === 2) {
    return getLastGrammarStateFromMap(args[1]);
  }
  const [internal, code, options = {}] = args;
  const {
    lang = "text",
    theme: themeName = internal.getLoadedThemes()[0]
  } = options;
  if (isPlainLang(lang) || isNoneTheme(themeName))
    throw new ShikiError$2("Plain language does not have grammar state");
  if (lang === "ansi")
    throw new ShikiError$2("ANSI language does not have grammar state");
  const { theme, colorMap } = internal.setTheme(themeName);
  const _grammar = internal.getLanguage(lang);
  return new GrammarState(
    _tokenizeWithTheme(code, _grammar, theme, colorMap, options).stateStack,
    _grammar.name,
    theme.name
  );
}
function tokenizeWithTheme(code, grammar, theme, colorMap, options) {
  const result = _tokenizeWithTheme(code, grammar, theme, colorMap, options);
  const grammarState = new GrammarState(
    _tokenizeWithTheme(code, grammar, theme, colorMap, options).stateStack,
    grammar.name,
    theme.name
  );
  setLastGrammarStateToMap(result.tokens, grammarState);
  return result.tokens;
}
function _tokenizeWithTheme(code, grammar, theme, colorMap, options) {
  const colorReplacements = resolveColorReplacements(theme, options);
  const {
    tokenizeMaxLineLength = 0,
    tokenizeTimeLimit = 500
  } = options;
  const lines = splitLines(code);
  let stateStack = options.grammarState ? getGrammarStack(options.grammarState, theme.name) ?? INITIAL : options.grammarContextCode != null ? _tokenizeWithTheme(
    options.grammarContextCode,
    grammar,
    theme,
    colorMap,
    {
      ...options,
      grammarState: undefined,
      grammarContextCode: undefined
    }
  ).stateStack : INITIAL;
  let actual = [];
  const final = [];
  for (let i = 0, len = lines.length; i < len; i++) {
    const [line, lineOffset] = lines[i];
    if (line === "") {
      actual = [];
      final.push([]);
      continue;
    }
    if (tokenizeMaxLineLength > 0 && line.length >= tokenizeMaxLineLength) {
      actual = [];
      final.push([{
        content: line,
        offset: lineOffset,
        color: "",
        fontStyle: 0
      }]);
      continue;
    }
    let resultWithScopes;
    let tokensWithScopes;
    let tokensWithScopesIndex;
    if (options.includeExplanation) {
      resultWithScopes = grammar.tokenizeLine(line, stateStack);
      tokensWithScopes = resultWithScopes.tokens;
      tokensWithScopesIndex = 0;
    }
    const result = grammar.tokenizeLine2(line, stateStack, tokenizeTimeLimit);
    const tokensLength = result.tokens.length / 2;
    for (let j = 0; j < tokensLength; j++) {
      const startIndex = result.tokens[2 * j];
      const nextStartIndex = j + 1 < tokensLength ? result.tokens[2 * j + 2] : line.length;
      if (startIndex === nextStartIndex)
        continue;
      const metadata = result.tokens[2 * j + 1];
      const color = applyColorReplacements(
        colorMap[EncodedTokenMetadata.getForeground(metadata)],
        colorReplacements
      );
      const fontStyle = EncodedTokenMetadata.getFontStyle(metadata);
      const token = {
        content: line.substring(startIndex, nextStartIndex),
        offset: lineOffset + startIndex,
        color,
        fontStyle
      };
      if (options.includeExplanation) {
        const themeSettingsSelectors = [];
        if (options.includeExplanation !== "scopeName") {
          for (const setting of theme.settings) {
            let selectors;
            switch (typeof setting.scope) {
              case "string":
                selectors = setting.scope.split(/,/).map((scope) => scope.trim());
                break;
              case "object":
                selectors = setting.scope;
                break;
              default:
                continue;
            }
            themeSettingsSelectors.push({
              settings: setting,
              selectors: selectors.map((selector) => selector.split(/ /))
            });
          }
        }
        token.explanation = [];
        let offset = 0;
        while (startIndex + offset < nextStartIndex) {
          const tokenWithScopes = tokensWithScopes[tokensWithScopesIndex];
          const tokenWithScopesText = line.substring(
            tokenWithScopes.startIndex,
            tokenWithScopes.endIndex
          );
          offset += tokenWithScopesText.length;
          token.explanation.push({
            content: tokenWithScopesText,
            scopes: options.includeExplanation === "scopeName" ? explainThemeScopesNameOnly(
              tokenWithScopes.scopes
            ) : explainThemeScopesFull(
              themeSettingsSelectors,
              tokenWithScopes.scopes
            )
          });
          tokensWithScopesIndex += 1;
        }
      }
      actual.push(token);
    }
    final.push(actual);
    actual = [];
    stateStack = result.ruleStack;
  }
  return {
    tokens: final,
    stateStack
  };
}
function explainThemeScopesNameOnly(scopes) {
  return scopes.map((scope) => ({ scopeName: scope }));
}
function explainThemeScopesFull(themeSelectors, scopes) {
  const result = [];
  for (let i = 0, len = scopes.length; i < len; i++) {
    const scope = scopes[i];
    result[i] = {
      scopeName: scope,
      themeMatches: explainThemeScope(themeSelectors, scope, scopes.slice(0, i))
    };
  }
  return result;
}
function matchesOne(selector, scope) {
  return selector === scope || scope.substring(0, selector.length) === selector && scope[selector.length] === ".";
}
function matches(selectors, scope, parentScopes) {
  if (!matchesOne(selectors[selectors.length - 1], scope))
    return false;
  let selectorParentIndex = selectors.length - 2;
  let parentIndex = parentScopes.length - 1;
  while (selectorParentIndex >= 0 && parentIndex >= 0) {
    if (matchesOne(selectors[selectorParentIndex], parentScopes[parentIndex]))
      selectorParentIndex -= 1;
    parentIndex -= 1;
  }
  if (selectorParentIndex === -1)
    return true;
  return false;
}
function explainThemeScope(themeSettingsSelectors, scope, parentScopes) {
  const result = [];
  for (const { selectors, settings } of themeSettingsSelectors) {
    for (const selectorPieces of selectors) {
      if (matches(selectorPieces, scope, parentScopes)) {
        result.push(settings);
        break;
      }
    }
  }
  return result;
}

function codeToTokensWithThemes(internal, code, options) {
  const themes = Object.entries(options.themes).filter((i) => i[1]).map((i) => ({ color: i[0], theme: i[1] }));
  const themedTokens = themes.map((t) => {
    const tokens2 = codeToTokensBase(internal, code, {
      ...options,
      theme: t.theme
    });
    const state = getLastGrammarStateFromMap(tokens2);
    const theme = typeof t.theme === "string" ? t.theme : t.theme.name;
    return {
      tokens: tokens2,
      state,
      theme
    };
  });
  const tokens = syncThemesTokenization(
    ...themedTokens.map((i) => i.tokens)
  );
  const mergedTokens = tokens[0].map(
    (line, lineIdx) => line.map((_token, tokenIdx) => {
      const mergedToken = {
        content: _token.content,
        variants: {},
        offset: _token.offset
      };
      if ("includeExplanation" in options && options.includeExplanation) {
        mergedToken.explanation = _token.explanation;
      }
      tokens.forEach((t, themeIdx) => {
        const {
          content: _,
          explanation: __,
          offset: ___,
          ...styles
        } = t[lineIdx][tokenIdx];
        mergedToken.variants[themes[themeIdx].color] = styles;
      });
      return mergedToken;
    })
  );
  const mergedGrammarState = themedTokens[0].state ? new GrammarState(
    Object.fromEntries(themedTokens.map((s) => [s.theme, s.state?.getInternalStack(s.theme)])),
    themedTokens[0].state.lang
  ) : undefined;
  if (mergedGrammarState)
    setLastGrammarStateToMap(mergedTokens, mergedGrammarState);
  return mergedTokens;
}
function syncThemesTokenization(...themes) {
  const outThemes = themes.map(() => []);
  const count = themes.length;
  for (let i = 0; i < themes[0].length; i++) {
    const lines = themes.map((t) => t[i]);
    const outLines = outThemes.map(() => []);
    outThemes.forEach((t, i2) => t.push(outLines[i2]));
    const indexes = lines.map(() => 0);
    const current = lines.map((l) => l[0]);
    while (current.every((t) => t)) {
      const minLength = Math.min(...current.map((t) => t.content.length));
      for (let n = 0; n < count; n++) {
        const token = current[n];
        if (token.content.length === minLength) {
          outLines[n].push(token);
          indexes[n] += 1;
          current[n] = lines[n][indexes[n]];
        } else {
          outLines[n].push({
            ...token,
            content: token.content.slice(0, minLength)
          });
          current[n] = {
            ...token,
            content: token.content.slice(minLength),
            offset: token.offset + minLength
          };
        }
      }
    }
  }
  return outThemes;
}

function codeToTokens(internal, code, options) {
  let bg;
  let fg;
  let tokens;
  let themeName;
  let rootStyle;
  let grammarState;
  if ("themes" in options) {
    const {
      defaultColor = "light",
      cssVariablePrefix = "--shiki-"
    } = options;
    const themes = Object.entries(options.themes).filter((i) => i[1]).map((i) => ({ color: i[0], theme: i[1] })).sort((a, b) => a.color === defaultColor ? -1 : b.color === defaultColor ? 1 : 0);
    if (themes.length === 0)
      throw new ShikiError$2("`themes` option must not be empty");
    const themeTokens = codeToTokensWithThemes(
      internal,
      code,
      options
    );
    grammarState = getLastGrammarStateFromMap(themeTokens);
    if (defaultColor && !themes.find((t) => t.color === defaultColor))
      throw new ShikiError$2(`\`themes\` option must contain the defaultColor key \`${defaultColor}\``);
    const themeRegs = themes.map((t) => internal.getTheme(t.theme));
    const themesOrder = themes.map((t) => t.color);
    tokens = themeTokens.map((line) => line.map((token) => mergeToken(token, themesOrder, cssVariablePrefix, defaultColor)));
    if (grammarState)
      setLastGrammarStateToMap(tokens, grammarState);
    const themeColorReplacements = themes.map((t) => resolveColorReplacements(t.theme, options));
    fg = themes.map((t, idx) => (idx === 0 && defaultColor ? "" : `${cssVariablePrefix + t.color}:`) + (applyColorReplacements(themeRegs[idx].fg, themeColorReplacements[idx]) || "inherit")).join(";");
    bg = themes.map((t, idx) => (idx === 0 && defaultColor ? "" : `${cssVariablePrefix + t.color}-bg:`) + (applyColorReplacements(themeRegs[idx].bg, themeColorReplacements[idx]) || "inherit")).join(";");
    themeName = `shiki-themes ${themeRegs.map((t) => t.name).join(" ")}`;
    rootStyle = defaultColor ? undefined : [fg, bg].join(";");
  } else if ("theme" in options) {
    const colorReplacements = resolveColorReplacements(options.theme, options);
    tokens = codeToTokensBase(
      internal,
      code,
      options
    );
    const _theme = internal.getTheme(options.theme);
    bg = applyColorReplacements(_theme.bg, colorReplacements);
    fg = applyColorReplacements(_theme.fg, colorReplacements);
    themeName = _theme.name;
    grammarState = getLastGrammarStateFromMap(tokens);
  } else {
    throw new ShikiError$2("Invalid options, either `theme` or `themes` must be provided");
  }
  return {
    tokens,
    fg,
    bg,
    themeName,
    rootStyle,
    grammarState
  };
}
function mergeToken(merged, variantsOrder, cssVariablePrefix, defaultColor) {
  const token = {
    content: merged.content,
    explanation: merged.explanation,
    offset: merged.offset
  };
  const styles = variantsOrder.map((t) => getTokenStyleObject(merged.variants[t]));
  const styleKeys = new Set(styles.flatMap((t) => Object.keys(t)));
  const mergedStyles = {};
  styles.forEach((cur, idx) => {
    for (const key of styleKeys) {
      const value = cur[key] || "inherit";
      if (idx === 0 && defaultColor) {
        mergedStyles[key] = value;
      } else {
        const keyName = key === "color" ? "" : key === "background-color" ? "-bg" : `-${key}`;
        const varKey = cssVariablePrefix + variantsOrder[idx] + (key === "color" ? "" : keyName);
        mergedStyles[varKey] = value;
      }
    }
  });
  token.htmlStyle = mergedStyles;
  return token;
}

function codeToHast(internal, code, options, transformerContext = {
  meta: {},
  options,
  codeToHast: (_code, _options) => codeToHast(internal, _code, _options),
  codeToTokens: (_code, _options) => codeToTokens(internal, _code, _options)
}) {
  let input = code;
  for (const transformer of getTransformers(options))
    input = transformer.preprocess?.call(transformerContext, input, options) || input;
  let {
    tokens,
    fg,
    bg,
    themeName,
    rootStyle,
    grammarState
  } = codeToTokens(internal, input, options);
  const {
    mergeWhitespaces = true
  } = options;
  if (mergeWhitespaces === true)
    tokens = mergeWhitespaceTokens(tokens);
  else if (mergeWhitespaces === "never")
    tokens = splitWhitespaceTokens(tokens);
  const contextSource = {
    ...transformerContext,
    get source() {
      return input;
    }
  };
  for (const transformer of getTransformers(options))
    tokens = transformer.tokens?.call(contextSource, tokens) || tokens;
  return tokensToHast(
    tokens,
    {
      ...options,
      fg,
      bg,
      themeName,
      rootStyle
    },
    contextSource,
    grammarState
  );
}
function tokensToHast(tokens, options, transformerContext, grammarState = getLastGrammarStateFromMap(tokens)) {
  const transformers = getTransformers(options);
  const lines = [];
  const root = {
    type: "root",
    children: []
  };
  const {
    structure = "classic",
    tabindex = "0"
  } = options;
  let preNode = {
    type: "element",
    tagName: "pre",
    properties: {
      class: `shiki ${options.themeName || ""}`,
      style: options.rootStyle || `background-color:${options.bg};color:${options.fg}`,
      ...tabindex !== false && tabindex != null ? {
        tabindex: tabindex.toString()
      } : {},
      ...Object.fromEntries(
        Array.from(
          Object.entries(options.meta || {})
        ).filter(([key]) => !key.startsWith("_"))
      )
    },
    children: []
  };
  let codeNode = {
    type: "element",
    tagName: "code",
    properties: {},
    children: lines
  };
  const lineNodes = [];
  const context = {
    ...transformerContext,
    structure,
    addClassToHast,
    get source() {
      return transformerContext.source;
    },
    get tokens() {
      return tokens;
    },
    get options() {
      return options;
    },
    get root() {
      return root;
    },
    get pre() {
      return preNode;
    },
    get code() {
      return codeNode;
    },
    get lines() {
      return lineNodes;
    }
  };
  tokens.forEach((line, idx) => {
    if (idx) {
      if (structure === "inline")
        root.children.push({ type: "element", tagName: "br", properties: {}, children: [] });
      else if (structure === "classic")
        lines.push({ type: "text", value: "\n" });
    }
    let lineNode = {
      type: "element",
      tagName: "span",
      properties: { class: "line" },
      children: []
    };
    let col = 0;
    for (const token of line) {
      let tokenNode = {
        type: "element",
        tagName: "span",
        properties: {
          ...token.htmlAttrs
        },
        children: [{ type: "text", value: token.content }]
      };
      if (typeof token.htmlStyle === "string")
        ;
      const style = stringifyTokenStyle(token.htmlStyle || getTokenStyleObject(token));
      if (style)
        tokenNode.properties.style = style;
      for (const transformer of transformers)
        tokenNode = transformer?.span?.call(context, tokenNode, idx + 1, col, lineNode, token) || tokenNode;
      if (structure === "inline")
        root.children.push(tokenNode);
      else if (structure === "classic")
        lineNode.children.push(tokenNode);
      col += token.content.length;
    }
    if (structure === "classic") {
      for (const transformer of transformers)
        lineNode = transformer?.line?.call(context, lineNode, idx + 1) || lineNode;
      lineNodes.push(lineNode);
      lines.push(lineNode);
    }
  });
  if (structure === "classic") {
    for (const transformer of transformers)
      codeNode = transformer?.code?.call(context, codeNode) || codeNode;
    preNode.children.push(codeNode);
    for (const transformer of transformers)
      preNode = transformer?.pre?.call(context, preNode) || preNode;
    root.children.push(preNode);
  }
  let result = root;
  for (const transformer of transformers)
    result = transformer?.root?.call(context, result) || result;
  if (grammarState)
    setLastGrammarStateToMap(result, grammarState);
  return result;
}
function mergeWhitespaceTokens(tokens) {
  return tokens.map((line) => {
    const newLine = [];
    let carryOnContent = "";
    let firstOffset = 0;
    line.forEach((token, idx) => {
      const isUnderline = token.fontStyle && token.fontStyle & FontStyle.Underline;
      const couldMerge = !isUnderline;
      if (couldMerge && token.content.match(/^\s+$/) && line[idx + 1]) {
        if (!firstOffset)
          firstOffset = token.offset;
        carryOnContent += token.content;
      } else {
        if (carryOnContent) {
          if (couldMerge) {
            newLine.push({
              ...token,
              offset: firstOffset,
              content: carryOnContent + token.content
            });
          } else {
            newLine.push(
              {
                content: carryOnContent,
                offset: firstOffset
              },
              token
            );
          }
          firstOffset = 0;
          carryOnContent = "";
        } else {
          newLine.push(token);
        }
      }
    });
    return newLine;
  });
}
function splitWhitespaceTokens(tokens) {
  return tokens.map((line) => {
    return line.flatMap((token) => {
      if (token.content.match(/^\s+$/))
        return token;
      const match = token.content.match(/^(\s*)(.*?)(\s*)$/);
      if (!match)
        return token;
      const [, leading, content, trailing] = match;
      if (!leading && !trailing)
        return token;
      const expanded = [{
        ...token,
        offset: token.offset + leading.length,
        content
      }];
      if (leading) {
        expanded.unshift({
          content: leading,
          offset: token.offset
        });
      }
      if (trailing) {
        expanded.push({
          content: trailing,
          offset: token.offset + leading.length + content.length
        });
      }
      return expanded;
    });
  });
}

function codeToHtml(internal, code, options) {
  const context = {
    meta: {},
    options,
    codeToHast: (_code, _options) => codeToHast(internal, _code, _options),
    codeToTokens: (_code, _options) => codeToTokens(internal, _code, _options)
  };
  let result = toHtml(codeToHast(internal, code, options, context));
  for (const transformer of getTransformers(options))
    result = transformer.postprocess?.call(context, result, options) || result;
  return result;
}

const VSCODE_FALLBACK_EDITOR_FG = { light: "#333333", dark: "#bbbbbb" };
const VSCODE_FALLBACK_EDITOR_BG = { light: "#fffffe", dark: "#1e1e1e" };
const RESOLVED_KEY = "__shiki_resolved";
function normalizeTheme(rawTheme) {
  if (rawTheme?.[RESOLVED_KEY])
    return rawTheme;
  const theme = {
    ...rawTheme
  };
  if (theme.tokenColors && !theme.settings) {
    theme.settings = theme.tokenColors;
    delete theme.tokenColors;
  }
  theme.type ||= "dark";
  theme.colorReplacements = { ...theme.colorReplacements };
  theme.settings ||= [];
  let { bg, fg } = theme;
  if (!bg || !fg) {
    const globalSetting = theme.settings ? theme.settings.find((s) => !s.name && !s.scope) : undefined;
    if (globalSetting?.settings?.foreground)
      fg = globalSetting.settings.foreground;
    if (globalSetting?.settings?.background)
      bg = globalSetting.settings.background;
    if (!fg && theme?.colors?.["editor.foreground"])
      fg = theme.colors["editor.foreground"];
    if (!bg && theme?.colors?.["editor.background"])
      bg = theme.colors["editor.background"];
    if (!fg)
      fg = theme.type === "light" ? VSCODE_FALLBACK_EDITOR_FG.light : VSCODE_FALLBACK_EDITOR_FG.dark;
    if (!bg)
      bg = theme.type === "light" ? VSCODE_FALLBACK_EDITOR_BG.light : VSCODE_FALLBACK_EDITOR_BG.dark;
    theme.fg = fg;
    theme.bg = bg;
  }
  if (!(theme.settings[0] && theme.settings[0].settings && !theme.settings[0].scope)) {
    theme.settings.unshift({
      settings: {
        foreground: theme.fg,
        background: theme.bg
      }
    });
  }
  let replacementCount = 0;
  const replacementMap = /* @__PURE__ */ new Map();
  function getReplacementColor(value) {
    if (replacementMap.has(value))
      return replacementMap.get(value);
    replacementCount += 1;
    const hex = `#${replacementCount.toString(16).padStart(8, "0").toLowerCase()}`;
    if (theme.colorReplacements?.[`#${hex}`])
      return getReplacementColor(value);
    replacementMap.set(value, hex);
    return hex;
  }
  theme.settings = theme.settings.map((setting) => {
    const replaceFg = setting.settings?.foreground && !setting.settings.foreground.startsWith("#");
    const replaceBg = setting.settings?.background && !setting.settings.background.startsWith("#");
    if (!replaceFg && !replaceBg)
      return setting;
    const clone = {
      ...setting,
      settings: {
        ...setting.settings
      }
    };
    if (replaceFg) {
      const replacement = getReplacementColor(setting.settings.foreground);
      theme.colorReplacements[replacement] = setting.settings.foreground;
      clone.settings.foreground = replacement;
    }
    if (replaceBg) {
      const replacement = getReplacementColor(setting.settings.background);
      theme.colorReplacements[replacement] = setting.settings.background;
      clone.settings.background = replacement;
    }
    return clone;
  });
  for (const key of Object.keys(theme.colors || {})) {
    if (key === "editor.foreground" || key === "editor.background" || key.startsWith("terminal.ansi")) {
      if (!theme.colors[key]?.startsWith("#")) {
        const replacement = getReplacementColor(theme.colors[key]);
        theme.colorReplacements[replacement] = theme.colors[key];
        theme.colors[key] = replacement;
      }
    }
  }
  Object.defineProperty(theme, RESOLVED_KEY, {
    enumerable: false,
    writable: false,
    value: true
  });
  return theme;
}

async function resolveLangs(langs) {
  return Array.from(new Set((await Promise.all(
    langs.filter((l) => !isSpecialLang(l)).map(async (lang) => await normalizeGetter(lang).then((r) => Array.isArray(r) ? r : [r]))
  )).flat()));
}
async function resolveThemes(themes) {
  const resolved = await Promise.all(
    themes.map(
      async (theme) => isSpecialTheme(theme) ? null : normalizeTheme(await normalizeGetter(theme))
    )
  );
  return resolved.filter((i) => !!i);
}

class Registry extends Registry$1 {
  constructor(_resolver, _themes, _langs, _alias = {}) {
    super(_resolver);
    this._resolver = _resolver;
    this._themes = _themes;
    this._langs = _langs;
    this._alias = _alias;
    this._themes.map((t) => this.loadTheme(t));
    this.loadLanguages(this._langs);
  }
  _resolvedThemes = /* @__PURE__ */ new Map();
  _resolvedGrammars = /* @__PURE__ */ new Map();
  _langMap = /* @__PURE__ */ new Map();
  _langGraph = /* @__PURE__ */ new Map();
  _textmateThemeCache = /* @__PURE__ */ new WeakMap();
  _loadedThemesCache = null;
  _loadedLanguagesCache = null;
  getTheme(theme) {
    if (typeof theme === "string")
      return this._resolvedThemes.get(theme);
    else
      return this.loadTheme(theme);
  }
  loadTheme(theme) {
    const _theme = normalizeTheme(theme);
    if (_theme.name) {
      this._resolvedThemes.set(_theme.name, _theme);
      this._loadedThemesCache = null;
    }
    return _theme;
  }
  getLoadedThemes() {
    if (!this._loadedThemesCache)
      this._loadedThemesCache = [...this._resolvedThemes.keys()];
    return this._loadedThemesCache;
  }
  // Override and re-implement this method to cache the textmate themes as `TextMateTheme.createFromRawTheme`
  // is expensive. Themes can switch often especially for dual-theme support.
  //
  // The parent class also accepts `colorMap` as the second parameter, but since we don't use that,
  // we omit here so it's easier to cache the themes.
  setTheme(theme) {
    let textmateTheme = this._textmateThemeCache.get(theme);
    if (!textmateTheme) {
      textmateTheme = Theme.createFromRawTheme(theme);
      this._textmateThemeCache.set(theme, textmateTheme);
    }
    this._syncRegistry.setTheme(textmateTheme);
  }
  getGrammar(name) {
    if (this._alias[name]) {
      const resolved = /* @__PURE__ */ new Set([name]);
      while (this._alias[name]) {
        name = this._alias[name];
        if (resolved.has(name))
          throw new ShikiError(`Circular alias \`${Array.from(resolved).join(" -> ")} -> ${name}\``);
        resolved.add(name);
      }
    }
    return this._resolvedGrammars.get(name);
  }
  loadLanguage(lang) {
    if (this.getGrammar(lang.name))
      return;
    const embeddedLazilyBy = new Set(
      [...this._langMap.values()].filter((i) => i.embeddedLangsLazy?.includes(lang.name))
    );
    this._resolver.addLanguage(lang);
    const grammarConfig = {
      balancedBracketSelectors: lang.balancedBracketSelectors || ["*"],
      unbalancedBracketSelectors: lang.unbalancedBracketSelectors || []
    };
    this._syncRegistry._rawGrammars.set(lang.scopeName, lang);
    const g = this.loadGrammarWithConfiguration(lang.scopeName, 1, grammarConfig);
    g.name = lang.name;
    this._resolvedGrammars.set(lang.name, g);
    if (lang.aliases) {
      lang.aliases.forEach((alias) => {
        this._alias[alias] = lang.name;
      });
    }
    this._loadedLanguagesCache = null;
    if (embeddedLazilyBy.size) {
      for (const e of embeddedLazilyBy) {
        this._resolvedGrammars.delete(e.name);
        this._loadedLanguagesCache = null;
        this._syncRegistry?._injectionGrammars?.delete(e.scopeName);
        this._syncRegistry?._grammars?.delete(e.scopeName);
        this.loadLanguage(this._langMap.get(e.name));
      }
    }
  }
  dispose() {
    super.dispose();
    this._resolvedThemes.clear();
    this._resolvedGrammars.clear();
    this._langMap.clear();
    this._langGraph.clear();
    this._loadedThemesCache = null;
  }
  loadLanguages(langs) {
    for (const lang of langs)
      this.resolveEmbeddedLanguages(lang);
    const langsGraphArray = Array.from(this._langGraph.entries());
    const missingLangs = langsGraphArray.filter(([_, lang]) => !lang);
    if (missingLangs.length) {
      const dependents = langsGraphArray.filter(([_, lang]) => lang && lang.embeddedLangs?.some((l) => missingLangs.map(([name]) => name).includes(l))).filter((lang) => !missingLangs.includes(lang));
      throw new ShikiError(`Missing languages ${missingLangs.map(([name]) => `\`${name}\``).join(", ")}, required by ${dependents.map(([name]) => `\`${name}\``).join(", ")}`);
    }
    for (const [_, lang] of langsGraphArray)
      this._resolver.addLanguage(lang);
    for (const [_, lang] of langsGraphArray)
      this.loadLanguage(lang);
  }
  getLoadedLanguages() {
    if (!this._loadedLanguagesCache) {
      this._loadedLanguagesCache = [
        .../* @__PURE__ */ new Set([...this._resolvedGrammars.keys(), ...Object.keys(this._alias)])
      ];
    }
    return this._loadedLanguagesCache;
  }
  resolveEmbeddedLanguages(lang) {
    this._langMap.set(lang.name, lang);
    this._langGraph.set(lang.name, lang);
    if (lang.embeddedLangs) {
      for (const embeddedLang of lang.embeddedLangs)
        this._langGraph.set(embeddedLang, this._langMap.get(embeddedLang));
    }
  }
}

class Resolver {
  _langs = /* @__PURE__ */ new Map();
  _scopeToLang = /* @__PURE__ */ new Map();
  _injections = /* @__PURE__ */ new Map();
  _onigLib;
  constructor(engine, langs) {
    this._onigLib = {
      createOnigScanner: (patterns) => engine.createScanner(patterns),
      createOnigString: (s) => engine.createString(s)
    };
    langs.forEach((i) => this.addLanguage(i));
  }
  get onigLib() {
    return this._onigLib;
  }
  getLangRegistration(langIdOrAlias) {
    return this._langs.get(langIdOrAlias);
  }
  loadGrammar(scopeName) {
    return this._scopeToLang.get(scopeName);
  }
  addLanguage(l) {
    this._langs.set(l.name, l);
    if (l.aliases) {
      l.aliases.forEach((a) => {
        this._langs.set(a, l);
      });
    }
    this._scopeToLang.set(l.scopeName, l);
    if (l.injectTo) {
      l.injectTo.forEach((i) => {
        if (!this._injections.get(i))
          this._injections.set(i, []);
        this._injections.get(i).push(l.scopeName);
      });
    }
  }
  getInjections(scopeName) {
    const scopeParts = scopeName.split(".");
    let injections = [];
    for (let i = 1; i <= scopeParts.length; i++) {
      const subScopeName = scopeParts.slice(0, i).join(".");
      injections = [...injections, ...this._injections.get(subScopeName) || []];
    }
    return injections;
  }
}

let instancesCount = 0;
function createShikiInternalSync(options) {
  instancesCount += 1;
  if (options.warnings !== false && instancesCount >= 10 && instancesCount % 10 === 0)
    console.warn(`[Shiki] ${instancesCount} instances have been created. Shiki is supposed to be used as a singleton, consider refactoring your code to cache your highlighter instance; Or call \`highlighter.dispose()\` to release unused instances.`);
  let isDisposed = false;
  if (!options.engine)
    throw new ShikiError("`engine` option is required for synchronous mode");
  const langs = (options.langs || []).flat(1);
  const themes = (options.themes || []).flat(1).map(normalizeTheme);
  const resolver = new Resolver(options.engine, langs);
  const _registry = new Registry(resolver, themes, langs, options.langAlias);
  let _lastTheme;
  function getLanguage(name) {
    ensureNotDisposed();
    const _lang = _registry.getGrammar(typeof name === "string" ? name : name.name);
    if (!_lang)
      throw new ShikiError(`Language \`${name}\` not found, you may need to load it first`);
    return _lang;
  }
  function getTheme(name) {
    if (name === "none")
      return { bg: "", fg: "", name: "none", settings: [], type: "dark" };
    ensureNotDisposed();
    const _theme = _registry.getTheme(name);
    if (!_theme)
      throw new ShikiError(`Theme \`${name}\` not found, you may need to load it first`);
    return _theme;
  }
  function setTheme(name) {
    ensureNotDisposed();
    const theme = getTheme(name);
    if (_lastTheme !== name) {
      _registry.setTheme(theme);
      _lastTheme = name;
    }
    const colorMap = _registry.getColorMap();
    return {
      theme,
      colorMap
    };
  }
  function getLoadedThemes() {
    ensureNotDisposed();
    return _registry.getLoadedThemes();
  }
  function getLoadedLanguages() {
    ensureNotDisposed();
    return _registry.getLoadedLanguages();
  }
  function loadLanguageSync(...langs2) {
    ensureNotDisposed();
    _registry.loadLanguages(langs2.flat(1));
  }
  async function loadLanguage(...langs2) {
    return loadLanguageSync(await resolveLangs(langs2));
  }
  function loadThemeSync(...themes2) {
    ensureNotDisposed();
    for (const theme of themes2.flat(1)) {
      _registry.loadTheme(theme);
    }
  }
  async function loadTheme(...themes2) {
    ensureNotDisposed();
    return loadThemeSync(await resolveThemes(themes2));
  }
  function ensureNotDisposed() {
    if (isDisposed)
      throw new ShikiError("Shiki instance has been disposed");
  }
  function dispose() {
    if (isDisposed)
      return;
    isDisposed = true;
    _registry.dispose();
    instancesCount -= 1;
  }
  return {
    setTheme,
    getTheme,
    getLanguage,
    getLoadedThemes,
    getLoadedLanguages,
    loadLanguage,
    loadLanguageSync,
    loadTheme,
    loadThemeSync,
    dispose,
    [Symbol.dispose]: dispose
  };
}

async function createShikiInternal(options = {}) {
  if (options.loadWasm) ;
  const [
    themes,
    langs,
    engine
  ] = await Promise.all([
    resolveThemes(options.themes || []),
    resolveLangs(options.langs || []),
    options.engine || createOnigurumaEngine(options.loadWasm || getDefaultWasmLoader())
  ]);
  return createShikiInternalSync({
    ...options,
    loadWasm: undefined,
    themes,
    langs,
    engine
  });
}

async function createHighlighterCore(options = {}) {
  const internal = await createShikiInternal(options);
  return {
    getLastGrammarState: (...args) => getLastGrammarState(internal, ...args),
    codeToTokensBase: (code, options2) => codeToTokensBase(internal, code, options2),
    codeToTokensWithThemes: (code, options2) => codeToTokensWithThemes(internal, code, options2),
    codeToTokens: (code, options2) => codeToTokens(internal, code, options2),
    codeToHast: (code, options2) => codeToHast(internal, code, options2),
    codeToHtml: (code, options2) => codeToHtml(internal, code, options2),
    ...internal,
    getInternalContext: () => internal
  };
}

export { FontStyle, ShikiError$2 as ShikiError, EncodedTokenMetadata as StackElementMetadata, addClassToHast, applyColorReplacements, codeToHast, codeToHtml, codeToTokens, codeToTokensBase, codeToTokensWithThemes, createHighlighterCore, createPositionConverter, createShikiInternal, createShikiInternalSync, getTokenStyleObject, toHtml as hastToHtml, isNoneTheme, isPlainLang, isSpecialLang, isSpecialTheme, normalizeGetter, normalizeTheme, resolveColorReplacements, splitLines, splitToken, splitTokens, stringifyTokenStyle, toArray, tokenizeAnsiWithTheme, tokenizeWithTheme, tokensToHast, transformerDecorations };
//# sourceMappingURL=data:application/json;charset=utf-8;base64,eyJ2ZXJzaW9uIjozLCJmaWxlIjoiY29yZTIuanMiLCJzb3VyY2VzIjpbIi4uLy4uLy4uLy4uLy4uL25vZGVfbW9kdWxlcy8ucG5wbS9Ac2hpa2lqcyt0eXBlc0AxLjI2LjEvbm9kZV9tb2R1bGVzL0BzaGlraWpzL3R5cGVzL2Rpc3QvaW5kZXgubWpzIiwiLi4vLi4vLi4vLi4vLi4vbm9kZV9tb2R1bGVzLy5wbnBtL0BzaGlraWpzK2VuZ2luZS1vbmlndXJ1bWFAMS4yNi4xL25vZGVfbW9kdWxlcy9Ac2hpa2lqcy9lbmdpbmUtb25pZ3VydW1hL2Rpc3QvaW5kZXgubWpzIiwiLi4vLi4vLi4vLi4vLi4vbm9kZV9tb2R1bGVzLy5wbnBtL0BzaGlraWpzK3ZzY29kZS10ZXh0bWF0ZUAxMC4wLjEvbm9kZV9tb2R1bGVzL0BzaGlraWpzL3ZzY29kZS10ZXh0bWF0ZS9kaXN0L2luZGV4LmpzIiwiLi4vLi4vLi4vLi4vLi4vbm9kZV9tb2R1bGVzLy5wbnBtL0BzaGlraWpzK2NvcmVAMS4yNi4xL25vZGVfbW9kdWxlcy9Ac2hpa2lqcy9jb3JlL2Rpc3QvaW5kZXgubWpzIl0sInNvdXJjZXNDb250ZW50IjpbImNsYXNzIFNoaWtpRXJyb3IgZXh0ZW5kcyBFcnJvciB7XG4gIGNvbnN0cnVjdG9yKG1lc3NhZ2UpIHtcbiAgICBzdXBlcihtZXNzYWdlKTtcbiAgICB0aGlzLm5hbWUgPSBcIlNoaWtpRXJyb3JcIjtcbiAgfVxufVxuXG5leHBvcnQgeyBTaGlraUVycm9yIH07XG4iLCJjbGFzcyBTaGlraUVycm9yIGV4dGVuZHMgRXJyb3Ige1xuICBjb25zdHJ1Y3RvcihtZXNzYWdlKSB7XG4gICAgc3VwZXIobWVzc2FnZSk7XG4gICAgdGhpcy5uYW1lID0gXCJTaGlraUVycm9yXCI7XG4gIH1cbn1cblxuZnVuY3Rpb24gZ2V0SGVhcE1heCgpIHtcbiAgcmV0dXJuIDIxNDc0ODM2NDg7XG59XG5mdW5jdGlvbiBfZW1zY3JpcHRlbl9nZXRfbm93KCkge1xuICByZXR1cm4gdHlwZW9mIHBlcmZvcm1hbmNlICE9PSBcInVuZGVmaW5lZFwiID8gcGVyZm9ybWFuY2Uubm93KCkgOiBEYXRlLm5vdygpO1xufVxuY29uc3QgYWxpZ25VcCA9ICh4LCBtdWx0aXBsZSkgPT4geCArIChtdWx0aXBsZSAtIHggJSBtdWx0aXBsZSkgJSBtdWx0aXBsZTtcbmFzeW5jIGZ1bmN0aW9uIG1haW4oaW5pdCkge1xuICBsZXQgd2FzbU1lbW9yeTtcbiAgbGV0IGJ1ZmZlcjtcbiAgY29uc3QgYmluZGluZyA9IHt9O1xuICBmdW5jdGlvbiB1cGRhdGVHbG9iYWxCdWZmZXJBbmRWaWV3cyhidWYpIHtcbiAgICBidWZmZXIgPSBidWY7XG4gICAgYmluZGluZy5IRUFQVTggPSBuZXcgVWludDhBcnJheShidWYpO1xuICAgIGJpbmRpbmcuSEVBUFUzMiA9IG5ldyBVaW50MzJBcnJheShidWYpO1xuICB9XG4gIGZ1bmN0aW9uIF9lbXNjcmlwdGVuX21lbWNweV9iaWcoZGVzdCwgc3JjLCBudW0pIHtcbiAgICBiaW5kaW5nLkhFQVBVOC5jb3B5V2l0aGluKGRlc3QsIHNyYywgc3JjICsgbnVtKTtcbiAgfVxuICBmdW5jdGlvbiBlbXNjcmlwdGVuX3JlYWxsb2NfYnVmZmVyKHNpemUpIHtcbiAgICB0cnkge1xuICAgICAgd2FzbU1lbW9yeS5ncm93KHNpemUgLSBidWZmZXIuYnl0ZUxlbmd0aCArIDY1NTM1ID4+PiAxNik7XG4gICAgICB1cGRhdGVHbG9iYWxCdWZmZXJBbmRWaWV3cyh3YXNtTWVtb3J5LmJ1ZmZlcik7XG4gICAgICByZXR1cm4gMTtcbiAgICB9IGNhdGNoIHtcbiAgICB9XG4gIH1cbiAgZnVuY3Rpb24gX2Vtc2NyaXB0ZW5fcmVzaXplX2hlYXAocmVxdWVzdGVkU2l6ZSkge1xuICAgIGNvbnN0IG9sZFNpemUgPSBiaW5kaW5nLkhFQVBVOC5sZW5ndGg7XG4gICAgcmVxdWVzdGVkU2l6ZSA9IHJlcXVlc3RlZFNpemUgPj4+IDA7XG4gICAgY29uc3QgbWF4SGVhcFNpemUgPSBnZXRIZWFwTWF4KCk7XG4gICAgaWYgKHJlcXVlc3RlZFNpemUgPiBtYXhIZWFwU2l6ZSlcbiAgICAgIHJldHVybiBmYWxzZTtcbiAgICBmb3IgKGxldCBjdXREb3duID0gMTsgY3V0RG93biA8PSA0OyBjdXREb3duICo9IDIpIHtcbiAgICAgIGxldCBvdmVyR3Jvd25IZWFwU2l6ZSA9IG9sZFNpemUgKiAoMSArIDAuMiAvIGN1dERvd24pO1xuICAgICAgb3Zlckdyb3duSGVhcFNpemUgPSBNYXRoLm1pbihvdmVyR3Jvd25IZWFwU2l6ZSwgcmVxdWVzdGVkU2l6ZSArIDEwMDY2MzI5Nik7XG4gICAgICBjb25zdCBuZXdTaXplID0gTWF0aC5taW4obWF4SGVhcFNpemUsIGFsaWduVXAoTWF0aC5tYXgocmVxdWVzdGVkU2l6ZSwgb3Zlckdyb3duSGVhcFNpemUpLCA2NTUzNikpO1xuICAgICAgY29uc3QgcmVwbGFjZW1lbnQgPSBlbXNjcmlwdGVuX3JlYWxsb2NfYnVmZmVyKG5ld1NpemUpO1xuICAgICAgaWYgKHJlcGxhY2VtZW50KVxuICAgICAgICByZXR1cm4gdHJ1ZTtcbiAgICB9XG4gICAgcmV0dXJuIGZhbHNlO1xuICB9XG4gIGNvbnN0IFVURjhEZWNvZGVyID0gdHlwZW9mIFRleHREZWNvZGVyICE9IFwidW5kZWZpbmVkXCIgPyBuZXcgVGV4dERlY29kZXIoXCJ1dGY4XCIpIDogdm9pZCAwO1xuICBmdW5jdGlvbiBVVEY4QXJyYXlUb1N0cmluZyhoZWFwT3JBcnJheSwgaWR4LCBtYXhCeXRlc1RvUmVhZCA9IDEwMjQpIHtcbiAgICBjb25zdCBlbmRJZHggPSBpZHggKyBtYXhCeXRlc1RvUmVhZDtcbiAgICBsZXQgZW5kUHRyID0gaWR4O1xuICAgIHdoaWxlIChoZWFwT3JBcnJheVtlbmRQdHJdICYmICEoZW5kUHRyID49IGVuZElkeCkpXG4gICAgICArK2VuZFB0cjtcbiAgICBpZiAoZW5kUHRyIC0gaWR4ID4gMTYgJiYgaGVhcE9yQXJyYXkuYnVmZmVyICYmIFVURjhEZWNvZGVyKSB7XG4gICAgICByZXR1cm4gVVRGOERlY29kZXIuZGVjb2RlKGhlYXBPckFycmF5LnN1YmFycmF5KGlkeCwgZW5kUHRyKSk7XG4gICAgfVxuICAgIGxldCBzdHIgPSBcIlwiO1xuICAgIHdoaWxlIChpZHggPCBlbmRQdHIpIHtcbiAgICAgIGxldCB1MCA9IGhlYXBPckFycmF5W2lkeCsrXTtcbiAgICAgIGlmICghKHUwICYgMTI4KSkge1xuICAgICAgICBzdHIgKz0gU3RyaW5nLmZyb21DaGFyQ29kZSh1MCk7XG4gICAgICAgIGNvbnRpbnVlO1xuICAgICAgfVxuICAgICAgY29uc3QgdTEgPSBoZWFwT3JBcnJheVtpZHgrK10gJiA2MztcbiAgICAgIGlmICgodTAgJiAyMjQpID09PSAxOTIpIHtcbiAgICAgICAgc3RyICs9IFN0cmluZy5mcm9tQ2hhckNvZGUoKHUwICYgMzEpIDw8IDYgfCB1MSk7XG4gICAgICAgIGNvbnRpbnVlO1xuICAgICAgfVxuICAgICAgY29uc3QgdTIgPSBoZWFwT3JBcnJheVtpZHgrK10gJiA2MztcbiAgICAgIGlmICgodTAgJiAyNDApID09PSAyMjQpIHtcbiAgICAgICAgdTAgPSAodTAgJiAxNSkgPDwgMTIgfCB1MSA8PCA2IHwgdTI7XG4gICAgICB9IGVsc2Uge1xuICAgICAgICB1MCA9ICh1MCAmIDcpIDw8IDE4IHwgdTEgPDwgMTIgfCB1MiA8PCA2IHwgaGVhcE9yQXJyYXlbaWR4KytdICYgNjM7XG4gICAgICB9XG4gICAgICBpZiAodTAgPCA2NTUzNikge1xuICAgICAgICBzdHIgKz0gU3RyaW5nLmZyb21DaGFyQ29kZSh1MCk7XG4gICAgICB9IGVsc2Uge1xuICAgICAgICBjb25zdCBjaCA9IHUwIC0gNjU1MzY7XG4gICAgICAgIHN0ciArPSBTdHJpbmcuZnJvbUNoYXJDb2RlKDU1Mjk2IHwgY2ggPj4gMTAsIDU2MzIwIHwgY2ggJiAxMDIzKTtcbiAgICAgIH1cbiAgICB9XG4gICAgcmV0dXJuIHN0cjtcbiAgfVxuICBmdW5jdGlvbiBVVEY4VG9TdHJpbmcocHRyLCBtYXhCeXRlc1RvUmVhZCkge1xuICAgIHJldHVybiBwdHIgPyBVVEY4QXJyYXlUb1N0cmluZyhiaW5kaW5nLkhFQVBVOCwgcHRyLCBtYXhCeXRlc1RvUmVhZCkgOiBcIlwiO1xuICB9XG4gIGNvbnN0IGFzbUxpYnJhcnlBcmcgPSB7XG4gICAgZW1zY3JpcHRlbl9nZXRfbm93OiBfZW1zY3JpcHRlbl9nZXRfbm93LFxuICAgIGVtc2NyaXB0ZW5fbWVtY3B5X2JpZzogX2Vtc2NyaXB0ZW5fbWVtY3B5X2JpZyxcbiAgICBlbXNjcmlwdGVuX3Jlc2l6ZV9oZWFwOiBfZW1zY3JpcHRlbl9yZXNpemVfaGVhcCxcbiAgICBmZF93cml0ZTogKCkgPT4gMFxuICB9O1xuICBhc3luYyBmdW5jdGlvbiBjcmVhdGVXYXNtKCkge1xuICAgIGNvbnN0IGluZm8gPSB7XG4gICAgICBlbnY6IGFzbUxpYnJhcnlBcmcsXG4gICAgICB3YXNpX3NuYXBzaG90X3ByZXZpZXcxOiBhc21MaWJyYXJ5QXJnXG4gICAgfTtcbiAgICBjb25zdCBleHBvcnRzID0gYXdhaXQgaW5pdChpbmZvKTtcbiAgICB3YXNtTWVtb3J5ID0gZXhwb3J0cy5tZW1vcnk7XG4gICAgdXBkYXRlR2xvYmFsQnVmZmVyQW5kVmlld3Mod2FzbU1lbW9yeS5idWZmZXIpO1xuICAgIE9iamVjdC5hc3NpZ24oYmluZGluZywgZXhwb3J0cyk7XG4gICAgYmluZGluZy5VVEY4VG9TdHJpbmcgPSBVVEY4VG9TdHJpbmc7XG4gIH1cbiAgYXdhaXQgY3JlYXRlV2FzbSgpO1xuICByZXR1cm4gYmluZGluZztcbn1cblxudmFyIF9fZGVmUHJvcCA9IE9iamVjdC5kZWZpbmVQcm9wZXJ0eTtcbnZhciBfX2RlZk5vcm1hbFByb3AgPSAob2JqLCBrZXksIHZhbHVlKSA9PiBrZXkgaW4gb2JqID8gX19kZWZQcm9wKG9iaiwga2V5LCB7IGVudW1lcmFibGU6IHRydWUsIGNvbmZpZ3VyYWJsZTogdHJ1ZSwgd3JpdGFibGU6IHRydWUsIHZhbHVlIH0pIDogb2JqW2tleV0gPSB2YWx1ZTtcbnZhciBfX3B1YmxpY0ZpZWxkID0gKG9iaiwga2V5LCB2YWx1ZSkgPT4ge1xuICBfX2RlZk5vcm1hbFByb3Aob2JqLCB0eXBlb2Yga2V5ICE9PSBcInN5bWJvbFwiID8ga2V5ICsgXCJcIiA6IGtleSwgdmFsdWUpO1xuICByZXR1cm4gdmFsdWU7XG59O1xubGV0IG9uaWdCaW5kaW5nID0gbnVsbDtcbmZ1bmN0aW9uIHRocm93TGFzdE9uaWdFcnJvcihvbmlnQmluZGluZzIpIHtcbiAgdGhyb3cgbmV3IFNoaWtpRXJyb3Iob25pZ0JpbmRpbmcyLlVURjhUb1N0cmluZyhvbmlnQmluZGluZzIuZ2V0TGFzdE9uaWdFcnJvcigpKSk7XG59XG5jbGFzcyBVdGZTdHJpbmcge1xuICBjb25zdHJ1Y3RvcihzdHIpIHtcbiAgICBfX3B1YmxpY0ZpZWxkKHRoaXMsIFwidXRmMTZMZW5ndGhcIik7XG4gICAgX19wdWJsaWNGaWVsZCh0aGlzLCBcInV0ZjhMZW5ndGhcIik7XG4gICAgX19wdWJsaWNGaWVsZCh0aGlzLCBcInV0ZjE2VmFsdWVcIik7XG4gICAgX19wdWJsaWNGaWVsZCh0aGlzLCBcInV0ZjhWYWx1ZVwiKTtcbiAgICBfX3B1YmxpY0ZpZWxkKHRoaXMsIFwidXRmMTZPZmZzZXRUb1V0ZjhcIik7XG4gICAgX19wdWJsaWNGaWVsZCh0aGlzLCBcInV0ZjhPZmZzZXRUb1V0ZjE2XCIpO1xuICAgIGNvbnN0IHV0ZjE2TGVuZ3RoID0gc3RyLmxlbmd0aDtcbiAgICBjb25zdCB1dGY4TGVuZ3RoID0gVXRmU3RyaW5nLl91dGY4Qnl0ZUxlbmd0aChzdHIpO1xuICAgIGNvbnN0IGNvbXB1dGVJbmRpY2VzTWFwcGluZyA9IHV0ZjhMZW5ndGggIT09IHV0ZjE2TGVuZ3RoO1xuICAgIGNvbnN0IHV0ZjE2T2Zmc2V0VG9VdGY4ID0gY29tcHV0ZUluZGljZXNNYXBwaW5nID8gbmV3IFVpbnQzMkFycmF5KHV0ZjE2TGVuZ3RoICsgMSkgOiBudWxsO1xuICAgIGlmIChjb21wdXRlSW5kaWNlc01hcHBpbmcpXG4gICAgICB1dGYxNk9mZnNldFRvVXRmOFt1dGYxNkxlbmd0aF0gPSB1dGY4TGVuZ3RoO1xuICAgIGNvbnN0IHV0ZjhPZmZzZXRUb1V0ZjE2ID0gY29tcHV0ZUluZGljZXNNYXBwaW5nID8gbmV3IFVpbnQzMkFycmF5KHV0ZjhMZW5ndGggKyAxKSA6IG51bGw7XG4gICAgaWYgKGNvbXB1dGVJbmRpY2VzTWFwcGluZylcbiAgICAgIHV0ZjhPZmZzZXRUb1V0ZjE2W3V0ZjhMZW5ndGhdID0gdXRmMTZMZW5ndGg7XG4gICAgY29uc3QgdXRmOFZhbHVlID0gbmV3IFVpbnQ4QXJyYXkodXRmOExlbmd0aCk7XG4gICAgbGV0IGk4ID0gMDtcbiAgICBmb3IgKGxldCBpMTYgPSAwOyBpMTYgPCB1dGYxNkxlbmd0aDsgaTE2KyspIHtcbiAgICAgIGNvbnN0IGNoYXJDb2RlID0gc3RyLmNoYXJDb2RlQXQoaTE2KTtcbiAgICAgIGxldCBjb2RlUG9pbnQgPSBjaGFyQ29kZTtcbiAgICAgIGxldCB3YXNTdXJyb2dhdGVQYWlyID0gZmFsc2U7XG4gICAgICBpZiAoY2hhckNvZGUgPj0gNTUyOTYgJiYgY2hhckNvZGUgPD0gNTYzMTkpIHtcbiAgICAgICAgaWYgKGkxNiArIDEgPCB1dGYxNkxlbmd0aCkge1xuICAgICAgICAgIGNvbnN0IG5leHRDaGFyQ29kZSA9IHN0ci5jaGFyQ29kZUF0KGkxNiArIDEpO1xuICAgICAgICAgIGlmIChuZXh0Q2hhckNvZGUgPj0gNTYzMjAgJiYgbmV4dENoYXJDb2RlIDw9IDU3MzQzKSB7XG4gICAgICAgICAgICBjb2RlUG9pbnQgPSAoY2hhckNvZGUgLSA1NTI5NiA8PCAxMCkgKyA2NTUzNiB8IG5leHRDaGFyQ29kZSAtIDU2MzIwO1xuICAgICAgICAgICAgd2FzU3Vycm9nYXRlUGFpciA9IHRydWU7XG4gICAgICAgICAgfVxuICAgICAgICB9XG4gICAgICB9XG4gICAgICBpZiAoY29tcHV0ZUluZGljZXNNYXBwaW5nKSB7XG4gICAgICAgIHV0ZjE2T2Zmc2V0VG9VdGY4W2kxNl0gPSBpODtcbiAgICAgICAgaWYgKHdhc1N1cnJvZ2F0ZVBhaXIpXG4gICAgICAgICAgdXRmMTZPZmZzZXRUb1V0ZjhbaTE2ICsgMV0gPSBpODtcbiAgICAgICAgaWYgKGNvZGVQb2ludCA8PSAxMjcpIHtcbiAgICAgICAgICB1dGY4T2Zmc2V0VG9VdGYxNltpOCArIDBdID0gaTE2O1xuICAgICAgICB9IGVsc2UgaWYgKGNvZGVQb2ludCA8PSAyMDQ3KSB7XG4gICAgICAgICAgdXRmOE9mZnNldFRvVXRmMTZbaTggKyAwXSA9IGkxNjtcbiAgICAgICAgICB1dGY4T2Zmc2V0VG9VdGYxNltpOCArIDFdID0gaTE2O1xuICAgICAgICB9IGVsc2UgaWYgKGNvZGVQb2ludCA8PSA2NTUzNSkge1xuICAgICAgICAgIHV0ZjhPZmZzZXRUb1V0ZjE2W2k4ICsgMF0gPSBpMTY7XG4gICAgICAgICAgdXRmOE9mZnNldFRvVXRmMTZbaTggKyAxXSA9IGkxNjtcbiAgICAgICAgICB1dGY4T2Zmc2V0VG9VdGYxNltpOCArIDJdID0gaTE2O1xuICAgICAgICB9IGVsc2Uge1xuICAgICAgICAgIHV0ZjhPZmZzZXRUb1V0ZjE2W2k4ICsgMF0gPSBpMTY7XG4gICAgICAgICAgdXRmOE9mZnNldFRvVXRmMTZbaTggKyAxXSA9IGkxNjtcbiAgICAgICAgICB1dGY4T2Zmc2V0VG9VdGYxNltpOCArIDJdID0gaTE2O1xuICAgICAgICAgIHV0ZjhPZmZzZXRUb1V0ZjE2W2k4ICsgM10gPSBpMTY7XG4gICAgICAgIH1cbiAgICAgIH1cbiAgICAgIGlmIChjb2RlUG9pbnQgPD0gMTI3KSB7XG4gICAgICAgIHV0ZjhWYWx1ZVtpOCsrXSA9IGNvZGVQb2ludDtcbiAgICAgIH0gZWxzZSBpZiAoY29kZVBvaW50IDw9IDIwNDcpIHtcbiAgICAgICAgdXRmOFZhbHVlW2k4KytdID0gMTkyIHwgKGNvZGVQb2ludCAmIDE5ODQpID4+PiA2O1xuICAgICAgICB1dGY4VmFsdWVbaTgrK10gPSAxMjggfCAoY29kZVBvaW50ICYgNjMpID4+PiAwO1xuICAgICAgfSBlbHNlIGlmIChjb2RlUG9pbnQgPD0gNjU1MzUpIHtcbiAgICAgICAgdXRmOFZhbHVlW2k4KytdID0gMjI0IHwgKGNvZGVQb2ludCAmIDYxNDQwKSA+Pj4gMTI7XG4gICAgICAgIHV0ZjhWYWx1ZVtpOCsrXSA9IDEyOCB8IChjb2RlUG9pbnQgJiA0MDMyKSA+Pj4gNjtcbiAgICAgICAgdXRmOFZhbHVlW2k4KytdID0gMTI4IHwgKGNvZGVQb2ludCAmIDYzKSA+Pj4gMDtcbiAgICAgIH0gZWxzZSB7XG4gICAgICAgIHV0ZjhWYWx1ZVtpOCsrXSA9IDI0MCB8IChjb2RlUG9pbnQgJiAxODM1MDA4KSA+Pj4gMTg7XG4gICAgICAgIHV0ZjhWYWx1ZVtpOCsrXSA9IDEyOCB8IChjb2RlUG9pbnQgJiAyNTgwNDgpID4+PiAxMjtcbiAgICAgICAgdXRmOFZhbHVlW2k4KytdID0gMTI4IHwgKGNvZGVQb2ludCAmIDQwMzIpID4+PiA2O1xuICAgICAgICB1dGY4VmFsdWVbaTgrK10gPSAxMjggfCAoY29kZVBvaW50ICYgNjMpID4+PiAwO1xuICAgICAgfVxuICAgICAgaWYgKHdhc1N1cnJvZ2F0ZVBhaXIpXG4gICAgICAgIGkxNisrO1xuICAgIH1cbiAgICB0aGlzLnV0ZjE2TGVuZ3RoID0gdXRmMTZMZW5ndGg7XG4gICAgdGhpcy51dGY4TGVuZ3RoID0gdXRmOExlbmd0aDtcbiAgICB0aGlzLnV0ZjE2VmFsdWUgPSBzdHI7XG4gICAgdGhpcy51dGY4VmFsdWUgPSB1dGY4VmFsdWU7XG4gICAgdGhpcy51dGYxNk9mZnNldFRvVXRmOCA9IHV0ZjE2T2Zmc2V0VG9VdGY4O1xuICAgIHRoaXMudXRmOE9mZnNldFRvVXRmMTYgPSB1dGY4T2Zmc2V0VG9VdGYxNjtcbiAgfVxuICBzdGF0aWMgX3V0ZjhCeXRlTGVuZ3RoKHN0cikge1xuICAgIGxldCByZXN1bHQgPSAwO1xuICAgIGZvciAobGV0IGkgPSAwLCBsZW4gPSBzdHIubGVuZ3RoOyBpIDwgbGVuOyBpKyspIHtcbiAgICAgIGNvbnN0IGNoYXJDb2RlID0gc3RyLmNoYXJDb2RlQXQoaSk7XG4gICAgICBsZXQgY29kZXBvaW50ID0gY2hhckNvZGU7XG4gICAgICBsZXQgd2FzU3Vycm9nYXRlUGFpciA9IGZhbHNlO1xuICAgICAgaWYgKGNoYXJDb2RlID49IDU1Mjk2ICYmIGNoYXJDb2RlIDw9IDU2MzE5KSB7XG4gICAgICAgIGlmIChpICsgMSA8IGxlbikge1xuICAgICAgICAgIGNvbnN0IG5leHRDaGFyQ29kZSA9IHN0ci5jaGFyQ29kZUF0KGkgKyAxKTtcbiAgICAgICAgICBpZiAobmV4dENoYXJDb2RlID49IDU2MzIwICYmIG5leHRDaGFyQ29kZSA8PSA1NzM0Mykge1xuICAgICAgICAgICAgY29kZXBvaW50ID0gKGNoYXJDb2RlIC0gNTUyOTYgPDwgMTApICsgNjU1MzYgfCBuZXh0Q2hhckNvZGUgLSA1NjMyMDtcbiAgICAgICAgICAgIHdhc1N1cnJvZ2F0ZVBhaXIgPSB0cnVlO1xuICAgICAgICAgIH1cbiAgICAgICAgfVxuICAgICAgfVxuICAgICAgaWYgKGNvZGVwb2ludCA8PSAxMjcpXG4gICAgICAgIHJlc3VsdCArPSAxO1xuICAgICAgZWxzZSBpZiAoY29kZXBvaW50IDw9IDIwNDcpXG4gICAgICAgIHJlc3VsdCArPSAyO1xuICAgICAgZWxzZSBpZiAoY29kZXBvaW50IDw9IDY1NTM1KVxuICAgICAgICByZXN1bHQgKz0gMztcbiAgICAgIGVsc2VcbiAgICAgICAgcmVzdWx0ICs9IDQ7XG4gICAgICBpZiAod2FzU3Vycm9nYXRlUGFpcilcbiAgICAgICAgaSsrO1xuICAgIH1cbiAgICByZXR1cm4gcmVzdWx0O1xuICB9XG4gIGNyZWF0ZVN0cmluZyhvbmlnQmluZGluZzIpIHtcbiAgICBjb25zdCByZXN1bHQgPSBvbmlnQmluZGluZzIub21hbGxvYyh0aGlzLnV0ZjhMZW5ndGgpO1xuICAgIG9uaWdCaW5kaW5nMi5IRUFQVTguc2V0KHRoaXMudXRmOFZhbHVlLCByZXN1bHQpO1xuICAgIHJldHVybiByZXN1bHQ7XG4gIH1cbn1cbmNvbnN0IF9PbmlnU3RyaW5nID0gY2xhc3Mge1xuICBjb25zdHJ1Y3RvcihzdHIpIHtcbiAgICBfX3B1YmxpY0ZpZWxkKHRoaXMsIFwiaWRcIiwgKytfT25pZ1N0cmluZy5MQVNUX0lEKTtcbiAgICBfX3B1YmxpY0ZpZWxkKHRoaXMsIFwiX29uaWdCaW5kaW5nXCIpO1xuICAgIF9fcHVibGljRmllbGQodGhpcywgXCJjb250ZW50XCIpO1xuICAgIF9fcHVibGljRmllbGQodGhpcywgXCJ1dGYxNkxlbmd0aFwiKTtcbiAgICBfX3B1YmxpY0ZpZWxkKHRoaXMsIFwidXRmOExlbmd0aFwiKTtcbiAgICBfX3B1YmxpY0ZpZWxkKHRoaXMsIFwidXRmMTZPZmZzZXRUb1V0ZjhcIik7XG4gICAgX19wdWJsaWNGaWVsZCh0aGlzLCBcInV0ZjhPZmZzZXRUb1V0ZjE2XCIpO1xuICAgIF9fcHVibGljRmllbGQodGhpcywgXCJwdHJcIik7XG4gICAgaWYgKCFvbmlnQmluZGluZylcbiAgICAgIHRocm93IG5ldyBTaGlraUVycm9yKFwiTXVzdCBpbnZva2UgbG9hZFdhc20gZmlyc3QuXCIpO1xuICAgIHRoaXMuX29uaWdCaW5kaW5nID0gb25pZ0JpbmRpbmc7XG4gICAgdGhpcy5jb250ZW50ID0gc3RyO1xuICAgIGNvbnN0IHV0ZlN0cmluZyA9IG5ldyBVdGZTdHJpbmcoc3RyKTtcbiAgICB0aGlzLnV0ZjE2TGVuZ3RoID0gdXRmU3RyaW5nLnV0ZjE2TGVuZ3RoO1xuICAgIHRoaXMudXRmOExlbmd0aCA9IHV0ZlN0cmluZy51dGY4TGVuZ3RoO1xuICAgIHRoaXMudXRmMTZPZmZzZXRUb1V0ZjggPSB1dGZTdHJpbmcudXRmMTZPZmZzZXRUb1V0Zjg7XG4gICAgdGhpcy51dGY4T2Zmc2V0VG9VdGYxNiA9IHV0ZlN0cmluZy51dGY4T2Zmc2V0VG9VdGYxNjtcbiAgICBpZiAodGhpcy51dGY4TGVuZ3RoIDwgMWU0ICYmICFfT25pZ1N0cmluZy5fc2hhcmVkUHRySW5Vc2UpIHtcbiAgICAgIGlmICghX09uaWdTdHJpbmcuX3NoYXJlZFB0cilcbiAgICAgICAgX09uaWdTdHJpbmcuX3NoYXJlZFB0ciA9IG9uaWdCaW5kaW5nLm9tYWxsb2MoMWU0KTtcbiAgICAgIF9PbmlnU3RyaW5nLl9zaGFyZWRQdHJJblVzZSA9IHRydWU7XG4gICAgICBvbmlnQmluZGluZy5IRUFQVTguc2V0KHV0ZlN0cmluZy51dGY4VmFsdWUsIF9PbmlnU3RyaW5nLl9zaGFyZWRQdHIpO1xuICAgICAgdGhpcy5wdHIgPSBfT25pZ1N0cmluZy5fc2hhcmVkUHRyO1xuICAgIH0gZWxzZSB7XG4gICAgICB0aGlzLnB0ciA9IHV0ZlN0cmluZy5jcmVhdGVTdHJpbmcob25pZ0JpbmRpbmcpO1xuICAgIH1cbiAgfVxuICBjb252ZXJ0VXRmOE9mZnNldFRvVXRmMTYodXRmOE9mZnNldCkge1xuICAgIGlmICh0aGlzLnV0ZjhPZmZzZXRUb1V0ZjE2KSB7XG4gICAgICBpZiAodXRmOE9mZnNldCA8IDApXG4gICAgICAgIHJldHVybiAwO1xuICAgICAgaWYgKHV0ZjhPZmZzZXQgPiB0aGlzLnV0ZjhMZW5ndGgpXG4gICAgICAgIHJldHVybiB0aGlzLnV0ZjE2TGVuZ3RoO1xuICAgICAgcmV0dXJuIHRoaXMudXRmOE9mZnNldFRvVXRmMTZbdXRmOE9mZnNldF07XG4gICAgfVxuICAgIHJldHVybiB1dGY4T2Zmc2V0O1xuICB9XG4gIGNvbnZlcnRVdGYxNk9mZnNldFRvVXRmOCh1dGYxNk9mZnNldCkge1xuICAgIGlmICh0aGlzLnV0ZjE2T2Zmc2V0VG9VdGY4KSB7XG4gICAgICBpZiAodXRmMTZPZmZzZXQgPCAwKVxuICAgICAgICByZXR1cm4gMDtcbiAgICAgIGlmICh1dGYxNk9mZnNldCA+IHRoaXMudXRmMTZMZW5ndGgpXG4gICAgICAgIHJldHVybiB0aGlzLnV0ZjhMZW5ndGg7XG4gICAgICByZXR1cm4gdGhpcy51dGYxNk9mZnNldFRvVXRmOFt1dGYxNk9mZnNldF07XG4gICAgfVxuICAgIHJldHVybiB1dGYxNk9mZnNldDtcbiAgfVxuICBkaXNwb3NlKCkge1xuICAgIGlmICh0aGlzLnB0ciA9PT0gX09uaWdTdHJpbmcuX3NoYXJlZFB0cilcbiAgICAgIF9PbmlnU3RyaW5nLl9zaGFyZWRQdHJJblVzZSA9IGZhbHNlO1xuICAgIGVsc2VcbiAgICAgIHRoaXMuX29uaWdCaW5kaW5nLm9mcmVlKHRoaXMucHRyKTtcbiAgfVxufTtcbmxldCBPbmlnU3RyaW5nID0gX09uaWdTdHJpbmc7XG5fX3B1YmxpY0ZpZWxkKE9uaWdTdHJpbmcsIFwiTEFTVF9JRFwiLCAwKTtcbl9fcHVibGljRmllbGQoT25pZ1N0cmluZywgXCJfc2hhcmVkUHRyXCIsIDApO1xuLy8gYSBwb2ludGVyIHRvIGEgc3RyaW5nIG9mIDEwMDAwIGJ5dGVzXG5fX3B1YmxpY0ZpZWxkKE9uaWdTdHJpbmcsIFwiX3NoYXJlZFB0ckluVXNlXCIsIGZhbHNlKTtcbmNsYXNzIE9uaWdTY2FubmVyIHtcbiAgY29uc3RydWN0b3IocGF0dGVybnMpIHtcbiAgICBfX3B1YmxpY0ZpZWxkKHRoaXMsIFwiX29uaWdCaW5kaW5nXCIpO1xuICAgIF9fcHVibGljRmllbGQodGhpcywgXCJfcHRyXCIpO1xuICAgIGlmICghb25pZ0JpbmRpbmcpXG4gICAgICB0aHJvdyBuZXcgU2hpa2lFcnJvcihcIk11c3QgaW52b2tlIGxvYWRXYXNtIGZpcnN0LlwiKTtcbiAgICBjb25zdCBzdHJQdHJzQXJyID0gW107XG4gICAgY29uc3Qgc3RyTGVuQXJyID0gW107XG4gICAgZm9yIChsZXQgaSA9IDAsIGxlbiA9IHBhdHRlcm5zLmxlbmd0aDsgaSA8IGxlbjsgaSsrKSB7XG4gICAgICBjb25zdCB1dGZTdHJpbmcgPSBuZXcgVXRmU3RyaW5nKHBhdHRlcm5zW2ldKTtcbiAgICAgIHN0clB0cnNBcnJbaV0gPSB1dGZTdHJpbmcuY3JlYXRlU3RyaW5nKG9uaWdCaW5kaW5nKTtcbiAgICAgIHN0ckxlbkFycltpXSA9IHV0ZlN0cmluZy51dGY4TGVuZ3RoO1xuICAgIH1cbiAgICBjb25zdCBzdHJQdHJzUHRyID0gb25pZ0JpbmRpbmcub21hbGxvYyg0ICogcGF0dGVybnMubGVuZ3RoKTtcbiAgICBvbmlnQmluZGluZy5IRUFQVTMyLnNldChzdHJQdHJzQXJyLCBzdHJQdHJzUHRyIC8gNCk7XG4gICAgY29uc3Qgc3RyTGVuUHRyID0gb25pZ0JpbmRpbmcub21hbGxvYyg0ICogcGF0dGVybnMubGVuZ3RoKTtcbiAgICBvbmlnQmluZGluZy5IRUFQVTMyLnNldChzdHJMZW5BcnIsIHN0ckxlblB0ciAvIDQpO1xuICAgIGNvbnN0IHNjYW5uZXJQdHIgPSBvbmlnQmluZGluZy5jcmVhdGVPbmlnU2Nhbm5lcihzdHJQdHJzUHRyLCBzdHJMZW5QdHIsIHBhdHRlcm5zLmxlbmd0aCk7XG4gICAgZm9yIChsZXQgaSA9IDAsIGxlbiA9IHBhdHRlcm5zLmxlbmd0aDsgaSA8IGxlbjsgaSsrKVxuICAgICAgb25pZ0JpbmRpbmcub2ZyZWUoc3RyUHRyc0FycltpXSk7XG4gICAgb25pZ0JpbmRpbmcub2ZyZWUoc3RyTGVuUHRyKTtcbiAgICBvbmlnQmluZGluZy5vZnJlZShzdHJQdHJzUHRyKTtcbiAgICBpZiAoc2Nhbm5lclB0ciA9PT0gMClcbiAgICAgIHRocm93TGFzdE9uaWdFcnJvcihvbmlnQmluZGluZyk7XG4gICAgdGhpcy5fb25pZ0JpbmRpbmcgPSBvbmlnQmluZGluZztcbiAgICB0aGlzLl9wdHIgPSBzY2FubmVyUHRyO1xuICB9XG4gIGRpc3Bvc2UoKSB7XG4gICAgdGhpcy5fb25pZ0JpbmRpbmcuZnJlZU9uaWdTY2FubmVyKHRoaXMuX3B0cik7XG4gIH1cbiAgZmluZE5leHRNYXRjaFN5bmMoc3RyaW5nLCBzdGFydFBvc2l0aW9uLCBhcmcpIHtcbiAgICBsZXQgb3B0aW9ucyA9IDAgLyogTm9uZSAqLztcbiAgICBpZiAodHlwZW9mIGFyZyA9PT0gXCJudW1iZXJcIikge1xuICAgICAgb3B0aW9ucyA9IGFyZztcbiAgICB9XG4gICAgaWYgKHR5cGVvZiBzdHJpbmcgPT09IFwic3RyaW5nXCIpIHtcbiAgICAgIHN0cmluZyA9IG5ldyBPbmlnU3RyaW5nKHN0cmluZyk7XG4gICAgICBjb25zdCByZXN1bHQgPSB0aGlzLl9maW5kTmV4dE1hdGNoU3luYyhzdHJpbmcsIHN0YXJ0UG9zaXRpb24sIGZhbHNlLCBvcHRpb25zKTtcbiAgICAgIHN0cmluZy5kaXNwb3NlKCk7XG4gICAgICByZXR1cm4gcmVzdWx0O1xuICAgIH1cbiAgICByZXR1cm4gdGhpcy5fZmluZE5leHRNYXRjaFN5bmMoc3RyaW5nLCBzdGFydFBvc2l0aW9uLCBmYWxzZSwgb3B0aW9ucyk7XG4gIH1cbiAgX2ZpbmROZXh0TWF0Y2hTeW5jKHN0cmluZywgc3RhcnRQb3NpdGlvbiwgZGVidWdDYWxsLCBvcHRpb25zKSB7XG4gICAgY29uc3Qgb25pZ0JpbmRpbmcyID0gdGhpcy5fb25pZ0JpbmRpbmc7XG4gICAgY29uc3QgcmVzdWx0UHRyID0gb25pZ0JpbmRpbmcyLmZpbmROZXh0T25pZ1NjYW5uZXJNYXRjaCh0aGlzLl9wdHIsIHN0cmluZy5pZCwgc3RyaW5nLnB0ciwgc3RyaW5nLnV0ZjhMZW5ndGgsIHN0cmluZy5jb252ZXJ0VXRmMTZPZmZzZXRUb1V0Zjgoc3RhcnRQb3NpdGlvbiksIG9wdGlvbnMpO1xuICAgIGlmIChyZXN1bHRQdHIgPT09IDApIHtcbiAgICAgIHJldHVybiBudWxsO1xuICAgIH1cbiAgICBjb25zdCBIRUFQVTMyID0gb25pZ0JpbmRpbmcyLkhFQVBVMzI7XG4gICAgbGV0IG9mZnNldCA9IHJlc3VsdFB0ciAvIDQ7XG4gICAgY29uc3QgaW5kZXggPSBIRUFQVTMyW29mZnNldCsrXTtcbiAgICBjb25zdCBjb3VudCA9IEhFQVBVMzJbb2Zmc2V0KytdO1xuICAgIGNvbnN0IGNhcHR1cmVJbmRpY2VzID0gW107XG4gICAgZm9yIChsZXQgaSA9IDA7IGkgPCBjb3VudDsgaSsrKSB7XG4gICAgICBjb25zdCBiZWcgPSBzdHJpbmcuY29udmVydFV0ZjhPZmZzZXRUb1V0ZjE2KEhFQVBVMzJbb2Zmc2V0KytdKTtcbiAgICAgIGNvbnN0IGVuZCA9IHN0cmluZy5jb252ZXJ0VXRmOE9mZnNldFRvVXRmMTYoSEVBUFUzMltvZmZzZXQrK10pO1xuICAgICAgY2FwdHVyZUluZGljZXNbaV0gPSB7XG4gICAgICAgIHN0YXJ0OiBiZWcsXG4gICAgICAgIGVuZCxcbiAgICAgICAgbGVuZ3RoOiBlbmQgLSBiZWdcbiAgICAgIH07XG4gICAgfVxuICAgIHJldHVybiB7XG4gICAgICBpbmRleCxcbiAgICAgIGNhcHR1cmVJbmRpY2VzXG4gICAgfTtcbiAgfVxufVxuZnVuY3Rpb24gaXNJbnN0YW50aWF0b3JPcHRpb25zT2JqZWN0KGRhdGFPck9wdGlvbnMpIHtcbiAgcmV0dXJuIHR5cGVvZiBkYXRhT3JPcHRpb25zLmluc3RhbnRpYXRvciA9PT0gXCJmdW5jdGlvblwiO1xufVxuZnVuY3Rpb24gaXNJbnN0YW50aWF0b3JNb2R1bGUoZGF0YU9yT3B0aW9ucykge1xuICByZXR1cm4gdHlwZW9mIGRhdGFPck9wdGlvbnMuZGVmYXVsdCA9PT0gXCJmdW5jdGlvblwiO1xufVxuZnVuY3Rpb24gaXNEYXRhT3B0aW9uc09iamVjdChkYXRhT3JPcHRpb25zKSB7XG4gIHJldHVybiB0eXBlb2YgZGF0YU9yT3B0aW9ucy5kYXRhICE9PSBcInVuZGVmaW5lZFwiO1xufVxuZnVuY3Rpb24gaXNSZXNwb25zZShkYXRhT3JPcHRpb25zKSB7XG4gIHJldHVybiB0eXBlb2YgUmVzcG9uc2UgIT09IFwidW5kZWZpbmVkXCIgJiYgZGF0YU9yT3B0aW9ucyBpbnN0YW5jZW9mIFJlc3BvbnNlO1xufVxuZnVuY3Rpb24gaXNBcnJheUJ1ZmZlcihkYXRhKSB7XG4gIHJldHVybiB0eXBlb2YgQXJyYXlCdWZmZXIgIT09IFwidW5kZWZpbmVkXCIgJiYgKGRhdGEgaW5zdGFuY2VvZiBBcnJheUJ1ZmZlciB8fCBBcnJheUJ1ZmZlci5pc1ZpZXcoZGF0YSkpIHx8IHR5cGVvZiBCdWZmZXIgIT09IFwidW5kZWZpbmVkXCIgJiYgQnVmZmVyLmlzQnVmZmVyPy4oZGF0YSkgfHwgdHlwZW9mIFNoYXJlZEFycmF5QnVmZmVyICE9PSBcInVuZGVmaW5lZFwiICYmIGRhdGEgaW5zdGFuY2VvZiBTaGFyZWRBcnJheUJ1ZmZlciB8fCB0eXBlb2YgVWludDMyQXJyYXkgIT09IFwidW5kZWZpbmVkXCIgJiYgZGF0YSBpbnN0YW5jZW9mIFVpbnQzMkFycmF5O1xufVxubGV0IGluaXRQcm9taXNlO1xuZnVuY3Rpb24gbG9hZFdhc20ob3B0aW9ucykge1xuICBpZiAoaW5pdFByb21pc2UpXG4gICAgcmV0dXJuIGluaXRQcm9taXNlO1xuICBhc3luYyBmdW5jdGlvbiBfbG9hZCgpIHtcbiAgICBvbmlnQmluZGluZyA9IGF3YWl0IG1haW4oYXN5bmMgKGluZm8pID0+IHtcbiAgICAgIGxldCBpbnN0YW5jZSA9IG9wdGlvbnM7XG4gICAgICBpbnN0YW5jZSA9IGF3YWl0IGluc3RhbmNlO1xuICAgICAgaWYgKHR5cGVvZiBpbnN0YW5jZSA9PT0gXCJmdW5jdGlvblwiKVxuICAgICAgICBpbnN0YW5jZSA9IGF3YWl0IGluc3RhbmNlKGluZm8pO1xuICAgICAgaWYgKHR5cGVvZiBpbnN0YW5jZSA9PT0gXCJmdW5jdGlvblwiKVxuICAgICAgICBpbnN0YW5jZSA9IGF3YWl0IGluc3RhbmNlKGluZm8pO1xuICAgICAgaWYgKGlzSW5zdGFudGlhdG9yT3B0aW9uc09iamVjdChpbnN0YW5jZSkpIHtcbiAgICAgICAgaW5zdGFuY2UgPSBhd2FpdCBpbnN0YW5jZS5pbnN0YW50aWF0b3IoaW5mbyk7XG4gICAgICB9IGVsc2UgaWYgKGlzSW5zdGFudGlhdG9yTW9kdWxlKGluc3RhbmNlKSkge1xuICAgICAgICBpbnN0YW5jZSA9IGF3YWl0IGluc3RhbmNlLmRlZmF1bHQoaW5mbyk7XG4gICAgICB9IGVsc2Uge1xuICAgICAgICBpZiAoaXNEYXRhT3B0aW9uc09iamVjdChpbnN0YW5jZSkpXG4gICAgICAgICAgaW5zdGFuY2UgPSBpbnN0YW5jZS5kYXRhO1xuICAgICAgICBpZiAoaXNSZXNwb25zZShpbnN0YW5jZSkpIHtcbiAgICAgICAgICBpZiAodHlwZW9mIFdlYkFzc2VtYmx5Lmluc3RhbnRpYXRlU3RyZWFtaW5nID09PSBcImZ1bmN0aW9uXCIpXG4gICAgICAgICAgICBpbnN0YW5jZSA9IGF3YWl0IF9tYWtlUmVzcG9uc2VTdHJlYW1pbmdMb2FkZXIoaW5zdGFuY2UpKGluZm8pO1xuICAgICAgICAgIGVsc2VcbiAgICAgICAgICAgIGluc3RhbmNlID0gYXdhaXQgX21ha2VSZXNwb25zZU5vblN0cmVhbWluZ0xvYWRlcihpbnN0YW5jZSkoaW5mbyk7XG4gICAgICAgIH0gZWxzZSBpZiAoaXNBcnJheUJ1ZmZlcihpbnN0YW5jZSkpIHtcbiAgICAgICAgICBpbnN0YW5jZSA9IGF3YWl0IF9tYWtlQXJyYXlCdWZmZXJMb2FkZXIoaW5zdGFuY2UpKGluZm8pO1xuICAgICAgICB9IGVsc2UgaWYgKGluc3RhbmNlIGluc3RhbmNlb2YgV2ViQXNzZW1ibHkuTW9kdWxlKSB7XG4gICAgICAgICAgaW5zdGFuY2UgPSBhd2FpdCBfbWFrZUFycmF5QnVmZmVyTG9hZGVyKGluc3RhbmNlKShpbmZvKTtcbiAgICAgICAgfSBlbHNlIGlmIChcImRlZmF1bHRcIiBpbiBpbnN0YW5jZSAmJiBpbnN0YW5jZS5kZWZhdWx0IGluc3RhbmNlb2YgV2ViQXNzZW1ibHkuTW9kdWxlKSB7XG4gICAgICAgICAgaW5zdGFuY2UgPSBhd2FpdCBfbWFrZUFycmF5QnVmZmVyTG9hZGVyKGluc3RhbmNlLmRlZmF1bHQpKGluZm8pO1xuICAgICAgICB9XG4gICAgICB9XG4gICAgICBpZiAoXCJpbnN0YW5jZVwiIGluIGluc3RhbmNlKVxuICAgICAgICBpbnN0YW5jZSA9IGluc3RhbmNlLmluc3RhbmNlO1xuICAgICAgaWYgKFwiZXhwb3J0c1wiIGluIGluc3RhbmNlKVxuICAgICAgICBpbnN0YW5jZSA9IGluc3RhbmNlLmV4cG9ydHM7XG4gICAgICByZXR1cm4gaW5zdGFuY2U7XG4gICAgfSk7XG4gIH1cbiAgaW5pdFByb21pc2UgPSBfbG9hZCgpO1xuICByZXR1cm4gaW5pdFByb21pc2U7XG59XG5mdW5jdGlvbiBfbWFrZUFycmF5QnVmZmVyTG9hZGVyKGRhdGEpIHtcbiAgcmV0dXJuIChpbXBvcnRPYmplY3QpID0+IFdlYkFzc2VtYmx5Lmluc3RhbnRpYXRlKGRhdGEsIGltcG9ydE9iamVjdCk7XG59XG5mdW5jdGlvbiBfbWFrZVJlc3BvbnNlU3RyZWFtaW5nTG9hZGVyKGRhdGEpIHtcbiAgcmV0dXJuIChpbXBvcnRPYmplY3QpID0+IFdlYkFzc2VtYmx5Lmluc3RhbnRpYXRlU3RyZWFtaW5nKGRhdGEsIGltcG9ydE9iamVjdCk7XG59XG5mdW5jdGlvbiBfbWFrZVJlc3BvbnNlTm9uU3RyZWFtaW5nTG9hZGVyKGRhdGEpIHtcbiAgcmV0dXJuIGFzeW5jIChpbXBvcnRPYmplY3QpID0+IHtcbiAgICBjb25zdCBhcnJheUJ1ZmZlciA9IGF3YWl0IGRhdGEuYXJyYXlCdWZmZXIoKTtcbiAgICByZXR1cm4gV2ViQXNzZW1ibHkuaW5zdGFudGlhdGUoYXJyYXlCdWZmZXIsIGltcG9ydE9iamVjdCk7XG4gIH07XG59XG5cbmxldCBfZGVmYXVsdFdhc21Mb2FkZXI7XG5mdW5jdGlvbiBzZXREZWZhdWx0V2FzbUxvYWRlcihfbG9hZGVyKSB7XG4gIF9kZWZhdWx0V2FzbUxvYWRlciA9IF9sb2FkZXI7XG59XG5mdW5jdGlvbiBnZXREZWZhdWx0V2FzbUxvYWRlcigpIHtcbiAgcmV0dXJuIF9kZWZhdWx0V2FzbUxvYWRlcjtcbn1cbmFzeW5jIGZ1bmN0aW9uIGNyZWF0ZU9uaWd1cnVtYUVuZ2luZShvcHRpb25zKSB7XG4gIGlmIChvcHRpb25zKVxuICAgIGF3YWl0IGxvYWRXYXNtKG9wdGlvbnMpO1xuICByZXR1cm4ge1xuICAgIGNyZWF0ZVNjYW5uZXIocGF0dGVybnMpIHtcbiAgICAgIHJldHVybiBuZXcgT25pZ1NjYW5uZXIocGF0dGVybnMubWFwKChwKSA9PiB0eXBlb2YgcCA9PT0gXCJzdHJpbmdcIiA/IHAgOiBwLnNvdXJjZSkpO1xuICAgIH0sXG4gICAgY3JlYXRlU3RyaW5nKHMpIHtcbiAgICAgIHJldHVybiBuZXcgT25pZ1N0cmluZyhzKTtcbiAgICB9XG4gIH07XG59XG5hc3luYyBmdW5jdGlvbiBjcmVhdGVXYXNtT25pZ0VuZ2luZShvcHRpb25zKSB7XG4gIHJldHVybiBjcmVhdGVPbmlndXJ1bWFFbmdpbmUob3B0aW9ucyk7XG59XG5cbmV4cG9ydCB7IGNyZWF0ZU9uaWd1cnVtYUVuZ2luZSwgY3JlYXRlV2FzbU9uaWdFbmdpbmUsIGdldERlZmF1bHRXYXNtTG9hZGVyLCBsb2FkV2FzbSwgc2V0RGVmYXVsdFdhc21Mb2FkZXIgfTtcbiIsIi8vIHNyYy91dGlscy50c1xuZnVuY3Rpb24gY2xvbmUoc29tZXRoaW5nKSB7XG4gIHJldHVybiBkb0Nsb25lKHNvbWV0aGluZyk7XG59XG5mdW5jdGlvbiBkb0Nsb25lKHNvbWV0aGluZykge1xuICBpZiAoQXJyYXkuaXNBcnJheShzb21ldGhpbmcpKSB7XG4gICAgcmV0dXJuIGNsb25lQXJyYXkoc29tZXRoaW5nKTtcbiAgfVxuICBpZiAoc29tZXRoaW5nIGluc3RhbmNlb2YgUmVnRXhwKSB7XG4gICAgcmV0dXJuIHNvbWV0aGluZztcbiAgfVxuICBpZiAodHlwZW9mIHNvbWV0aGluZyA9PT0gXCJvYmplY3RcIikge1xuICAgIHJldHVybiBjbG9uZU9iaihzb21ldGhpbmcpO1xuICB9XG4gIHJldHVybiBzb21ldGhpbmc7XG59XG5mdW5jdGlvbiBjbG9uZUFycmF5KGFycikge1xuICBsZXQgciA9IFtdO1xuICBmb3IgKGxldCBpID0gMCwgbGVuID0gYXJyLmxlbmd0aDsgaSA8IGxlbjsgaSsrKSB7XG4gICAgcltpXSA9IGRvQ2xvbmUoYXJyW2ldKTtcbiAgfVxuICByZXR1cm4gcjtcbn1cbmZ1bmN0aW9uIGNsb25lT2JqKG9iaikge1xuICBsZXQgciA9IHt9O1xuICBmb3IgKGxldCBrZXkgaW4gb2JqKSB7XG4gICAgcltrZXldID0gZG9DbG9uZShvYmpba2V5XSk7XG4gIH1cbiAgcmV0dXJuIHI7XG59XG5mdW5jdGlvbiBtZXJnZU9iamVjdHModGFyZ2V0LCAuLi5zb3VyY2VzKSB7XG4gIHNvdXJjZXMuZm9yRWFjaCgoc291cmNlKSA9PiB7XG4gICAgZm9yIChsZXQga2V5IGluIHNvdXJjZSkge1xuICAgICAgdGFyZ2V0W2tleV0gPSBzb3VyY2Vba2V5XTtcbiAgICB9XG4gIH0pO1xuICByZXR1cm4gdGFyZ2V0O1xufVxuZnVuY3Rpb24gYmFzZW5hbWUocGF0aCkge1xuICBjb25zdCBpZHggPSB+cGF0aC5sYXN0SW5kZXhPZihcIi9cIikgfHwgfnBhdGgubGFzdEluZGV4T2YoXCJcXFxcXCIpO1xuICBpZiAoaWR4ID09PSAwKSB7XG4gICAgcmV0dXJuIHBhdGg7XG4gIH0gZWxzZSBpZiAofmlkeCA9PT0gcGF0aC5sZW5ndGggLSAxKSB7XG4gICAgcmV0dXJuIGJhc2VuYW1lKHBhdGguc3Vic3RyaW5nKDAsIHBhdGgubGVuZ3RoIC0gMSkpO1xuICB9IGVsc2Uge1xuICAgIHJldHVybiBwYXRoLnN1YnN0cih+aWR4ICsgMSk7XG4gIH1cbn1cbnZhciBDQVBUVVJJTkdfUkVHRVhfU09VUkNFID0gL1xcJChcXGQrKXxcXCR7KFxcZCspOlxcLyhkb3duY2FzZXx1cGNhc2UpfS9nO1xudmFyIFJlZ2V4U291cmNlID0gY2xhc3Mge1xuICBzdGF0aWMgaGFzQ2FwdHVyZXMocmVnZXhTb3VyY2UpIHtcbiAgICBpZiAocmVnZXhTb3VyY2UgPT09IG51bGwpIHtcbiAgICAgIHJldHVybiBmYWxzZTtcbiAgICB9XG4gICAgQ0FQVFVSSU5HX1JFR0VYX1NPVVJDRS5sYXN0SW5kZXggPSAwO1xuICAgIHJldHVybiBDQVBUVVJJTkdfUkVHRVhfU09VUkNFLnRlc3QocmVnZXhTb3VyY2UpO1xuICB9XG4gIHN0YXRpYyByZXBsYWNlQ2FwdHVyZXMocmVnZXhTb3VyY2UsIGNhcHR1cmVTb3VyY2UsIGNhcHR1cmVJbmRpY2VzKSB7XG4gICAgcmV0dXJuIHJlZ2V4U291cmNlLnJlcGxhY2UoQ0FQVFVSSU5HX1JFR0VYX1NPVVJDRSwgKG1hdGNoLCBpbmRleCwgY29tbWFuZEluZGV4LCBjb21tYW5kKSA9PiB7XG4gICAgICBsZXQgY2FwdHVyZSA9IGNhcHR1cmVJbmRpY2VzW3BhcnNlSW50KGluZGV4IHx8IGNvbW1hbmRJbmRleCwgMTApXTtcbiAgICAgIGlmIChjYXB0dXJlKSB7XG4gICAgICAgIGxldCByZXN1bHQgPSBjYXB0dXJlU291cmNlLnN1YnN0cmluZyhjYXB0dXJlLnN0YXJ0LCBjYXB0dXJlLmVuZCk7XG4gICAgICAgIHdoaWxlIChyZXN1bHRbMF0gPT09IFwiLlwiKSB7XG4gICAgICAgICAgcmVzdWx0ID0gcmVzdWx0LnN1YnN0cmluZygxKTtcbiAgICAgICAgfVxuICAgICAgICBzd2l0Y2ggKGNvbW1hbmQpIHtcbiAgICAgICAgICBjYXNlIFwiZG93bmNhc2VcIjpcbiAgICAgICAgICAgIHJldHVybiByZXN1bHQudG9Mb3dlckNhc2UoKTtcbiAgICAgICAgICBjYXNlIFwidXBjYXNlXCI6XG4gICAgICAgICAgICByZXR1cm4gcmVzdWx0LnRvVXBwZXJDYXNlKCk7XG4gICAgICAgICAgZGVmYXVsdDpcbiAgICAgICAgICAgIHJldHVybiByZXN1bHQ7XG4gICAgICAgIH1cbiAgICAgIH0gZWxzZSB7XG4gICAgICAgIHJldHVybiBtYXRjaDtcbiAgICAgIH1cbiAgICB9KTtcbiAgfVxufTtcbmZ1bmN0aW9uIHN0cmNtcChhLCBiKSB7XG4gIGlmIChhIDwgYikge1xuICAgIHJldHVybiAtMTtcbiAgfVxuICBpZiAoYSA+IGIpIHtcbiAgICByZXR1cm4gMTtcbiAgfVxuICByZXR1cm4gMDtcbn1cbmZ1bmN0aW9uIHN0ckFyckNtcChhLCBiKSB7XG4gIGlmIChhID09PSBudWxsICYmIGIgPT09IG51bGwpIHtcbiAgICByZXR1cm4gMDtcbiAgfVxuICBpZiAoIWEpIHtcbiAgICByZXR1cm4gLTE7XG4gIH1cbiAgaWYgKCFiKSB7XG4gICAgcmV0dXJuIDE7XG4gIH1cbiAgbGV0IGxlbjEgPSBhLmxlbmd0aDtcbiAgbGV0IGxlbjIgPSBiLmxlbmd0aDtcbiAgaWYgKGxlbjEgPT09IGxlbjIpIHtcbiAgICBmb3IgKGxldCBpID0gMDsgaSA8IGxlbjE7IGkrKykge1xuICAgICAgbGV0IHJlcyA9IHN0cmNtcChhW2ldLCBiW2ldKTtcbiAgICAgIGlmIChyZXMgIT09IDApIHtcbiAgICAgICAgcmV0dXJuIHJlcztcbiAgICAgIH1cbiAgICB9XG4gICAgcmV0dXJuIDA7XG4gIH1cbiAgcmV0dXJuIGxlbjEgLSBsZW4yO1xufVxuZnVuY3Rpb24gaXNWYWxpZEhleENvbG9yKGhleCkge1xuICBpZiAoL14jWzAtOWEtZl17Nn0kL2kudGVzdChoZXgpKSB7XG4gICAgcmV0dXJuIHRydWU7XG4gIH1cbiAgaWYgKC9eI1swLTlhLWZdezh9JC9pLnRlc3QoaGV4KSkge1xuICAgIHJldHVybiB0cnVlO1xuICB9XG4gIGlmICgvXiNbMC05YS1mXXszfSQvaS50ZXN0KGhleCkpIHtcbiAgICByZXR1cm4gdHJ1ZTtcbiAgfVxuICBpZiAoL14jWzAtOWEtZl17NH0kL2kudGVzdChoZXgpKSB7XG4gICAgcmV0dXJuIHRydWU7XG4gIH1cbiAgcmV0dXJuIGZhbHNlO1xufVxuZnVuY3Rpb24gZXNjYXBlUmVnRXhwQ2hhcmFjdGVycyh2YWx1ZSkge1xuICByZXR1cm4gdmFsdWUucmVwbGFjZSgvW1xcLVxcXFxcXHtcXH1cXCpcXCtcXD9cXHxcXF5cXCRcXC5cXCxcXFtcXF1cXChcXClcXCNcXHNdL2csIFwiXFxcXCQmXCIpO1xufVxudmFyIENhY2hlZEZuID0gY2xhc3Mge1xuICBjb25zdHJ1Y3Rvcihmbikge1xuICAgIHRoaXMuZm4gPSBmbjtcbiAgfVxuICBjYWNoZSA9IC8qIEBfX1BVUkVfXyAqLyBuZXcgTWFwKCk7XG4gIGdldChrZXkpIHtcbiAgICBpZiAodGhpcy5jYWNoZS5oYXMoa2V5KSkge1xuICAgICAgcmV0dXJuIHRoaXMuY2FjaGUuZ2V0KGtleSk7XG4gICAgfVxuICAgIGNvbnN0IHZhbHVlID0gdGhpcy5mbihrZXkpO1xuICAgIHRoaXMuY2FjaGUuc2V0KGtleSwgdmFsdWUpO1xuICAgIHJldHVybiB2YWx1ZTtcbiAgfVxufTtcblxuLy8gc3JjL3RoZW1lLnRzXG52YXIgVGhlbWUgPSBjbGFzcyB7XG4gIGNvbnN0cnVjdG9yKF9jb2xvck1hcCwgX2RlZmF1bHRzLCBfcm9vdCkge1xuICAgIHRoaXMuX2NvbG9yTWFwID0gX2NvbG9yTWFwO1xuICAgIHRoaXMuX2RlZmF1bHRzID0gX2RlZmF1bHRzO1xuICAgIHRoaXMuX3Jvb3QgPSBfcm9vdDtcbiAgfVxuICBzdGF0aWMgY3JlYXRlRnJvbVJhd1RoZW1lKHNvdXJjZSwgY29sb3JNYXApIHtcbiAgICByZXR1cm4gdGhpcy5jcmVhdGVGcm9tUGFyc2VkVGhlbWUocGFyc2VUaGVtZShzb3VyY2UpLCBjb2xvck1hcCk7XG4gIH1cbiAgc3RhdGljIGNyZWF0ZUZyb21QYXJzZWRUaGVtZShzb3VyY2UsIGNvbG9yTWFwKSB7XG4gICAgcmV0dXJuIHJlc29sdmVQYXJzZWRUaGVtZVJ1bGVzKHNvdXJjZSwgY29sb3JNYXApO1xuICB9XG4gIF9jYWNoZWRNYXRjaFJvb3QgPSBuZXcgQ2FjaGVkRm4oXG4gICAgKHNjb3BlTmFtZSkgPT4gdGhpcy5fcm9vdC5tYXRjaChzY29wZU5hbWUpXG4gICk7XG4gIGdldENvbG9yTWFwKCkge1xuICAgIHJldHVybiB0aGlzLl9jb2xvck1hcC5nZXRDb2xvck1hcCgpO1xuICB9XG4gIGdldERlZmF1bHRzKCkge1xuICAgIHJldHVybiB0aGlzLl9kZWZhdWx0cztcbiAgfVxuICBtYXRjaChzY29wZVBhdGgpIHtcbiAgICBpZiAoc2NvcGVQYXRoID09PSBudWxsKSB7XG4gICAgICByZXR1cm4gdGhpcy5fZGVmYXVsdHM7XG4gICAgfVxuICAgIGNvbnN0IHNjb3BlTmFtZSA9IHNjb3BlUGF0aC5zY29wZU5hbWU7XG4gICAgY29uc3QgbWF0Y2hpbmdUcmllRWxlbWVudHMgPSB0aGlzLl9jYWNoZWRNYXRjaFJvb3QuZ2V0KHNjb3BlTmFtZSk7XG4gICAgY29uc3QgZWZmZWN0aXZlUnVsZSA9IG1hdGNoaW5nVHJpZUVsZW1lbnRzLmZpbmQoXG4gICAgICAodikgPT4gX3Njb3BlUGF0aE1hdGNoZXNQYXJlbnRTY29wZXMoc2NvcGVQYXRoLnBhcmVudCwgdi5wYXJlbnRTY29wZXMpXG4gICAgKTtcbiAgICBpZiAoIWVmZmVjdGl2ZVJ1bGUpIHtcbiAgICAgIHJldHVybiBudWxsO1xuICAgIH1cbiAgICByZXR1cm4gbmV3IFN0eWxlQXR0cmlidXRlcyhcbiAgICAgIGVmZmVjdGl2ZVJ1bGUuZm9udFN0eWxlLFxuICAgICAgZWZmZWN0aXZlUnVsZS5mb3JlZ3JvdW5kLFxuICAgICAgZWZmZWN0aXZlUnVsZS5iYWNrZ3JvdW5kXG4gICAgKTtcbiAgfVxufTtcbnZhciBTY29wZVN0YWNrID0gY2xhc3MgX1Njb3BlU3RhY2sge1xuICBjb25zdHJ1Y3RvcihwYXJlbnQsIHNjb3BlTmFtZSkge1xuICAgIHRoaXMucGFyZW50ID0gcGFyZW50O1xuICAgIHRoaXMuc2NvcGVOYW1lID0gc2NvcGVOYW1lO1xuICB9XG4gIHN0YXRpYyBwdXNoKHBhdGgsIHNjb3BlTmFtZXMpIHtcbiAgICBmb3IgKGNvbnN0IG5hbWUgb2Ygc2NvcGVOYW1lcykge1xuICAgICAgcGF0aCA9IG5ldyBfU2NvcGVTdGFjayhwYXRoLCBuYW1lKTtcbiAgICB9XG4gICAgcmV0dXJuIHBhdGg7XG4gIH1cbiAgc3RhdGljIGZyb20oLi4uc2VnbWVudHMpIHtcbiAgICBsZXQgcmVzdWx0ID0gbnVsbDtcbiAgICBmb3IgKGxldCBpID0gMDsgaSA8IHNlZ21lbnRzLmxlbmd0aDsgaSsrKSB7XG4gICAgICByZXN1bHQgPSBuZXcgX1Njb3BlU3RhY2socmVzdWx0LCBzZWdtZW50c1tpXSk7XG4gICAgfVxuICAgIHJldHVybiByZXN1bHQ7XG4gIH1cbiAgcHVzaChzY29wZU5hbWUpIHtcbiAgICByZXR1cm4gbmV3IF9TY29wZVN0YWNrKHRoaXMsIHNjb3BlTmFtZSk7XG4gIH1cbiAgZ2V0U2VnbWVudHMoKSB7XG4gICAgbGV0IGl0ZW0gPSB0aGlzO1xuICAgIGNvbnN0IHJlc3VsdCA9IFtdO1xuICAgIHdoaWxlIChpdGVtKSB7XG4gICAgICByZXN1bHQucHVzaChpdGVtLnNjb3BlTmFtZSk7XG4gICAgICBpdGVtID0gaXRlbS5wYXJlbnQ7XG4gICAgfVxuICAgIHJlc3VsdC5yZXZlcnNlKCk7XG4gICAgcmV0dXJuIHJlc3VsdDtcbiAgfVxuICB0b1N0cmluZygpIHtcbiAgICByZXR1cm4gdGhpcy5nZXRTZWdtZW50cygpLmpvaW4oXCIgXCIpO1xuICB9XG4gIGV4dGVuZHMob3RoZXIpIHtcbiAgICBpZiAodGhpcyA9PT0gb3RoZXIpIHtcbiAgICAgIHJldHVybiB0cnVlO1xuICAgIH1cbiAgICBpZiAodGhpcy5wYXJlbnQgPT09IG51bGwpIHtcbiAgICAgIHJldHVybiBmYWxzZTtcbiAgICB9XG4gICAgcmV0dXJuIHRoaXMucGFyZW50LmV4dGVuZHMob3RoZXIpO1xuICB9XG4gIGdldEV4dGVuc2lvbklmRGVmaW5lZChiYXNlKSB7XG4gICAgY29uc3QgcmVzdWx0ID0gW107XG4gICAgbGV0IGl0ZW0gPSB0aGlzO1xuICAgIHdoaWxlIChpdGVtICYmIGl0ZW0gIT09IGJhc2UpIHtcbiAgICAgIHJlc3VsdC5wdXNoKGl0ZW0uc2NvcGVOYW1lKTtcbiAgICAgIGl0ZW0gPSBpdGVtLnBhcmVudDtcbiAgICB9XG4gICAgcmV0dXJuIGl0ZW0gPT09IGJhc2UgPyByZXN1bHQucmV2ZXJzZSgpIDogdm9pZCAwO1xuICB9XG59O1xuZnVuY3Rpb24gX3Njb3BlUGF0aE1hdGNoZXNQYXJlbnRTY29wZXMoc2NvcGVQYXRoLCBwYXJlbnRTY29wZXMpIHtcbiAgaWYgKHBhcmVudFNjb3Blcy5sZW5ndGggPT09IDApIHtcbiAgICByZXR1cm4gdHJ1ZTtcbiAgfVxuICBmb3IgKGxldCBpbmRleCA9IDA7IGluZGV4IDwgcGFyZW50U2NvcGVzLmxlbmd0aDsgaW5kZXgrKykge1xuICAgIGxldCBzY29wZVBhdHRlcm4gPSBwYXJlbnRTY29wZXNbaW5kZXhdO1xuICAgIGxldCBzY29wZU11c3RNYXRjaCA9IGZhbHNlO1xuICAgIGlmIChzY29wZVBhdHRlcm4gPT09IFwiPlwiKSB7XG4gICAgICBpZiAoaW5kZXggPT09IHBhcmVudFNjb3Blcy5sZW5ndGggLSAxKSB7XG4gICAgICAgIHJldHVybiBmYWxzZTtcbiAgICAgIH1cbiAgICAgIHNjb3BlUGF0dGVybiA9IHBhcmVudFNjb3Blc1srK2luZGV4XTtcbiAgICAgIHNjb3BlTXVzdE1hdGNoID0gdHJ1ZTtcbiAgICB9XG4gICAgd2hpbGUgKHNjb3BlUGF0aCkge1xuICAgICAgaWYgKF9tYXRjaGVzU2NvcGUoc2NvcGVQYXRoLnNjb3BlTmFtZSwgc2NvcGVQYXR0ZXJuKSkge1xuICAgICAgICBicmVhaztcbiAgICAgIH1cbiAgICAgIGlmIChzY29wZU11c3RNYXRjaCkge1xuICAgICAgICByZXR1cm4gZmFsc2U7XG4gICAgICB9XG4gICAgICBzY29wZVBhdGggPSBzY29wZVBhdGgucGFyZW50O1xuICAgIH1cbiAgICBpZiAoIXNjb3BlUGF0aCkge1xuICAgICAgcmV0dXJuIGZhbHNlO1xuICAgIH1cbiAgICBzY29wZVBhdGggPSBzY29wZVBhdGgucGFyZW50O1xuICB9XG4gIHJldHVybiB0cnVlO1xufVxuZnVuY3Rpb24gX21hdGNoZXNTY29wZShzY29wZU5hbWUsIHNjb3BlUGF0dGVybikge1xuICByZXR1cm4gc2NvcGVQYXR0ZXJuID09PSBzY29wZU5hbWUgfHwgc2NvcGVOYW1lLnN0YXJ0c1dpdGgoc2NvcGVQYXR0ZXJuKSAmJiBzY29wZU5hbWVbc2NvcGVQYXR0ZXJuLmxlbmd0aF0gPT09IFwiLlwiO1xufVxudmFyIFN0eWxlQXR0cmlidXRlcyA9IGNsYXNzIHtcbiAgY29uc3RydWN0b3IoZm9udFN0eWxlLCBmb3JlZ3JvdW5kSWQsIGJhY2tncm91bmRJZCkge1xuICAgIHRoaXMuZm9udFN0eWxlID0gZm9udFN0eWxlO1xuICAgIHRoaXMuZm9yZWdyb3VuZElkID0gZm9yZWdyb3VuZElkO1xuICAgIHRoaXMuYmFja2dyb3VuZElkID0gYmFja2dyb3VuZElkO1xuICB9XG59O1xuZnVuY3Rpb24gcGFyc2VUaGVtZShzb3VyY2UpIHtcbiAgaWYgKCFzb3VyY2UpIHtcbiAgICByZXR1cm4gW107XG4gIH1cbiAgaWYgKCFzb3VyY2Uuc2V0dGluZ3MgfHwgIUFycmF5LmlzQXJyYXkoc291cmNlLnNldHRpbmdzKSkge1xuICAgIHJldHVybiBbXTtcbiAgfVxuICBsZXQgc2V0dGluZ3MgPSBzb3VyY2Uuc2V0dGluZ3M7XG4gIGxldCByZXN1bHQgPSBbXSwgcmVzdWx0TGVuID0gMDtcbiAgZm9yIChsZXQgaSA9IDAsIGxlbiA9IHNldHRpbmdzLmxlbmd0aDsgaSA8IGxlbjsgaSsrKSB7XG4gICAgbGV0IGVudHJ5ID0gc2V0dGluZ3NbaV07XG4gICAgaWYgKCFlbnRyeS5zZXR0aW5ncykge1xuICAgICAgY29udGludWU7XG4gICAgfVxuICAgIGxldCBzY29wZXM7XG4gICAgaWYgKHR5cGVvZiBlbnRyeS5zY29wZSA9PT0gXCJzdHJpbmdcIikge1xuICAgICAgbGV0IF9zY29wZSA9IGVudHJ5LnNjb3BlO1xuICAgICAgX3Njb3BlID0gX3Njb3BlLnJlcGxhY2UoL15bLF0rLywgXCJcIik7XG4gICAgICBfc2NvcGUgPSBfc2NvcGUucmVwbGFjZSgvWyxdKyQvLCBcIlwiKTtcbiAgICAgIHNjb3BlcyA9IF9zY29wZS5zcGxpdChcIixcIik7XG4gICAgfSBlbHNlIGlmIChBcnJheS5pc0FycmF5KGVudHJ5LnNjb3BlKSkge1xuICAgICAgc2NvcGVzID0gZW50cnkuc2NvcGU7XG4gICAgfSBlbHNlIHtcbiAgICAgIHNjb3BlcyA9IFtcIlwiXTtcbiAgICB9XG4gICAgbGV0IGZvbnRTdHlsZSA9IC0xIC8qIE5vdFNldCAqLztcbiAgICBpZiAodHlwZW9mIGVudHJ5LnNldHRpbmdzLmZvbnRTdHlsZSA9PT0gXCJzdHJpbmdcIikge1xuICAgICAgZm9udFN0eWxlID0gMCAvKiBOb25lICovO1xuICAgICAgbGV0IHNlZ21lbnRzID0gZW50cnkuc2V0dGluZ3MuZm9udFN0eWxlLnNwbGl0KFwiIFwiKTtcbiAgICAgIGZvciAobGV0IGogPSAwLCBsZW5KID0gc2VnbWVudHMubGVuZ3RoOyBqIDwgbGVuSjsgaisrKSB7XG4gICAgICAgIGxldCBzZWdtZW50ID0gc2VnbWVudHNbal07XG4gICAgICAgIHN3aXRjaCAoc2VnbWVudCkge1xuICAgICAgICAgIGNhc2UgXCJpdGFsaWNcIjpcbiAgICAgICAgICAgIGZvbnRTdHlsZSA9IGZvbnRTdHlsZSB8IDEgLyogSXRhbGljICovO1xuICAgICAgICAgICAgYnJlYWs7XG4gICAgICAgICAgY2FzZSBcImJvbGRcIjpcbiAgICAgICAgICAgIGZvbnRTdHlsZSA9IGZvbnRTdHlsZSB8IDIgLyogQm9sZCAqLztcbiAgICAgICAgICAgIGJyZWFrO1xuICAgICAgICAgIGNhc2UgXCJ1bmRlcmxpbmVcIjpcbiAgICAgICAgICAgIGZvbnRTdHlsZSA9IGZvbnRTdHlsZSB8IDQgLyogVW5kZXJsaW5lICovO1xuICAgICAgICAgICAgYnJlYWs7XG4gICAgICAgICAgY2FzZSBcInN0cmlrZXRocm91Z2hcIjpcbiAgICAgICAgICAgIGZvbnRTdHlsZSA9IGZvbnRTdHlsZSB8IDggLyogU3RyaWtldGhyb3VnaCAqLztcbiAgICAgICAgICAgIGJyZWFrO1xuICAgICAgICB9XG4gICAgICB9XG4gICAgfVxuICAgIGxldCBmb3JlZ3JvdW5kID0gbnVsbDtcbiAgICBpZiAodHlwZW9mIGVudHJ5LnNldHRpbmdzLmZvcmVncm91bmQgPT09IFwic3RyaW5nXCIgJiYgaXNWYWxpZEhleENvbG9yKGVudHJ5LnNldHRpbmdzLmZvcmVncm91bmQpKSB7XG4gICAgICBmb3JlZ3JvdW5kID0gZW50cnkuc2V0dGluZ3MuZm9yZWdyb3VuZDtcbiAgICB9XG4gICAgbGV0IGJhY2tncm91bmQgPSBudWxsO1xuICAgIGlmICh0eXBlb2YgZW50cnkuc2V0dGluZ3MuYmFja2dyb3VuZCA9PT0gXCJzdHJpbmdcIiAmJiBpc1ZhbGlkSGV4Q29sb3IoZW50cnkuc2V0dGluZ3MuYmFja2dyb3VuZCkpIHtcbiAgICAgIGJhY2tncm91bmQgPSBlbnRyeS5zZXR0aW5ncy5iYWNrZ3JvdW5kO1xuICAgIH1cbiAgICBmb3IgKGxldCBqID0gMCwgbGVuSiA9IHNjb3Blcy5sZW5ndGg7IGogPCBsZW5KOyBqKyspIHtcbiAgICAgIGxldCBfc2NvcGUgPSBzY29wZXNbal0udHJpbSgpO1xuICAgICAgbGV0IHNlZ21lbnRzID0gX3Njb3BlLnNwbGl0KFwiIFwiKTtcbiAgICAgIGxldCBzY29wZSA9IHNlZ21lbnRzW3NlZ21lbnRzLmxlbmd0aCAtIDFdO1xuICAgICAgbGV0IHBhcmVudFNjb3BlcyA9IG51bGw7XG4gICAgICBpZiAoc2VnbWVudHMubGVuZ3RoID4gMSkge1xuICAgICAgICBwYXJlbnRTY29wZXMgPSBzZWdtZW50cy5zbGljZSgwLCBzZWdtZW50cy5sZW5ndGggLSAxKTtcbiAgICAgICAgcGFyZW50U2NvcGVzLnJldmVyc2UoKTtcbiAgICAgIH1cbiAgICAgIHJlc3VsdFtyZXN1bHRMZW4rK10gPSBuZXcgUGFyc2VkVGhlbWVSdWxlKFxuICAgICAgICBzY29wZSxcbiAgICAgICAgcGFyZW50U2NvcGVzLFxuICAgICAgICBpLFxuICAgICAgICBmb250U3R5bGUsXG4gICAgICAgIGZvcmVncm91bmQsXG4gICAgICAgIGJhY2tncm91bmRcbiAgICAgICk7XG4gICAgfVxuICB9XG4gIHJldHVybiByZXN1bHQ7XG59XG52YXIgUGFyc2VkVGhlbWVSdWxlID0gY2xhc3Mge1xuICBjb25zdHJ1Y3RvcihzY29wZSwgcGFyZW50U2NvcGVzLCBpbmRleCwgZm9udFN0eWxlLCBmb3JlZ3JvdW5kLCBiYWNrZ3JvdW5kKSB7XG4gICAgdGhpcy5zY29wZSA9IHNjb3BlO1xuICAgIHRoaXMucGFyZW50U2NvcGVzID0gcGFyZW50U2NvcGVzO1xuICAgIHRoaXMuaW5kZXggPSBpbmRleDtcbiAgICB0aGlzLmZvbnRTdHlsZSA9IGZvbnRTdHlsZTtcbiAgICB0aGlzLmZvcmVncm91bmQgPSBmb3JlZ3JvdW5kO1xuICAgIHRoaXMuYmFja2dyb3VuZCA9IGJhY2tncm91bmQ7XG4gIH1cbn07XG52YXIgRm9udFN0eWxlID0gLyogQF9fUFVSRV9fICovICgoRm9udFN0eWxlMikgPT4ge1xuICBGb250U3R5bGUyW0ZvbnRTdHlsZTJbXCJOb3RTZXRcIl0gPSAtMV0gPSBcIk5vdFNldFwiO1xuICBGb250U3R5bGUyW0ZvbnRTdHlsZTJbXCJOb25lXCJdID0gMF0gPSBcIk5vbmVcIjtcbiAgRm9udFN0eWxlMltGb250U3R5bGUyW1wiSXRhbGljXCJdID0gMV0gPSBcIkl0YWxpY1wiO1xuICBGb250U3R5bGUyW0ZvbnRTdHlsZTJbXCJCb2xkXCJdID0gMl0gPSBcIkJvbGRcIjtcbiAgRm9udFN0eWxlMltGb250U3R5bGUyW1wiVW5kZXJsaW5lXCJdID0gNF0gPSBcIlVuZGVybGluZVwiO1xuICBGb250U3R5bGUyW0ZvbnRTdHlsZTJbXCJTdHJpa2V0aHJvdWdoXCJdID0gOF0gPSBcIlN0cmlrZXRocm91Z2hcIjtcbiAgcmV0dXJuIEZvbnRTdHlsZTI7XG59KShGb250U3R5bGUgfHwge30pO1xuZnVuY3Rpb24gcmVzb2x2ZVBhcnNlZFRoZW1lUnVsZXMocGFyc2VkVGhlbWVSdWxlcywgX2NvbG9yTWFwKSB7XG4gIHBhcnNlZFRoZW1lUnVsZXMuc29ydCgoYSwgYikgPT4ge1xuICAgIGxldCByID0gc3RyY21wKGEuc2NvcGUsIGIuc2NvcGUpO1xuICAgIGlmIChyICE9PSAwKSB7XG4gICAgICByZXR1cm4gcjtcbiAgICB9XG4gICAgciA9IHN0ckFyckNtcChhLnBhcmVudFNjb3BlcywgYi5wYXJlbnRTY29wZXMpO1xuICAgIGlmIChyICE9PSAwKSB7XG4gICAgICByZXR1cm4gcjtcbiAgICB9XG4gICAgcmV0dXJuIGEuaW5kZXggLSBiLmluZGV4O1xuICB9KTtcbiAgbGV0IGRlZmF1bHRGb250U3R5bGUgPSAwIC8qIE5vbmUgKi87XG4gIGxldCBkZWZhdWx0Rm9yZWdyb3VuZCA9IFwiIzAwMDAwMFwiO1xuICBsZXQgZGVmYXVsdEJhY2tncm91bmQgPSBcIiNmZmZmZmZcIjtcbiAgd2hpbGUgKHBhcnNlZFRoZW1lUnVsZXMubGVuZ3RoID49IDEgJiYgcGFyc2VkVGhlbWVSdWxlc1swXS5zY29wZSA9PT0gXCJcIikge1xuICAgIGxldCBpbmNvbWluZ0RlZmF1bHRzID0gcGFyc2VkVGhlbWVSdWxlcy5zaGlmdCgpO1xuICAgIGlmIChpbmNvbWluZ0RlZmF1bHRzLmZvbnRTdHlsZSAhPT0gLTEgLyogTm90U2V0ICovKSB7XG4gICAgICBkZWZhdWx0Rm9udFN0eWxlID0gaW5jb21pbmdEZWZhdWx0cy5mb250U3R5bGU7XG4gICAgfVxuICAgIGlmIChpbmNvbWluZ0RlZmF1bHRzLmZvcmVncm91bmQgIT09IG51bGwpIHtcbiAgICAgIGRlZmF1bHRGb3JlZ3JvdW5kID0gaW5jb21pbmdEZWZhdWx0cy5mb3JlZ3JvdW5kO1xuICAgIH1cbiAgICBpZiAoaW5jb21pbmdEZWZhdWx0cy5iYWNrZ3JvdW5kICE9PSBudWxsKSB7XG4gICAgICBkZWZhdWx0QmFja2dyb3VuZCA9IGluY29taW5nRGVmYXVsdHMuYmFja2dyb3VuZDtcbiAgICB9XG4gIH1cbiAgbGV0IGNvbG9yTWFwID0gbmV3IENvbG9yTWFwKF9jb2xvck1hcCk7XG4gIGxldCBkZWZhdWx0cyA9IG5ldyBTdHlsZUF0dHJpYnV0ZXMoZGVmYXVsdEZvbnRTdHlsZSwgY29sb3JNYXAuZ2V0SWQoZGVmYXVsdEZvcmVncm91bmQpLCBjb2xvck1hcC5nZXRJZChkZWZhdWx0QmFja2dyb3VuZCkpO1xuICBsZXQgcm9vdCA9IG5ldyBUaGVtZVRyaWVFbGVtZW50KG5ldyBUaGVtZVRyaWVFbGVtZW50UnVsZSgwLCBudWxsLCAtMSAvKiBOb3RTZXQgKi8sIDAsIDApLCBbXSk7XG4gIGZvciAobGV0IGkgPSAwLCBsZW4gPSBwYXJzZWRUaGVtZVJ1bGVzLmxlbmd0aDsgaSA8IGxlbjsgaSsrKSB7XG4gICAgbGV0IHJ1bGUgPSBwYXJzZWRUaGVtZVJ1bGVzW2ldO1xuICAgIHJvb3QuaW5zZXJ0KDAsIHJ1bGUuc2NvcGUsIHJ1bGUucGFyZW50U2NvcGVzLCBydWxlLmZvbnRTdHlsZSwgY29sb3JNYXAuZ2V0SWQocnVsZS5mb3JlZ3JvdW5kKSwgY29sb3JNYXAuZ2V0SWQocnVsZS5iYWNrZ3JvdW5kKSk7XG4gIH1cbiAgcmV0dXJuIG5ldyBUaGVtZShjb2xvck1hcCwgZGVmYXVsdHMsIHJvb3QpO1xufVxudmFyIENvbG9yTWFwID0gY2xhc3Mge1xuICBfaXNGcm96ZW47XG4gIF9sYXN0Q29sb3JJZDtcbiAgX2lkMmNvbG9yO1xuICBfY29sb3IyaWQ7XG4gIGNvbnN0cnVjdG9yKF9jb2xvck1hcCkge1xuICAgIHRoaXMuX2xhc3RDb2xvcklkID0gMDtcbiAgICB0aGlzLl9pZDJjb2xvciA9IFtdO1xuICAgIHRoaXMuX2NvbG9yMmlkID0gLyogQF9fUFVSRV9fICovIE9iamVjdC5jcmVhdGUobnVsbCk7XG4gICAgaWYgKEFycmF5LmlzQXJyYXkoX2NvbG9yTWFwKSkge1xuICAgICAgdGhpcy5faXNGcm96ZW4gPSB0cnVlO1xuICAgICAgZm9yIChsZXQgaSA9IDAsIGxlbiA9IF9jb2xvck1hcC5sZW5ndGg7IGkgPCBsZW47IGkrKykge1xuICAgICAgICB0aGlzLl9jb2xvcjJpZFtfY29sb3JNYXBbaV1dID0gaTtcbiAgICAgICAgdGhpcy5faWQyY29sb3JbaV0gPSBfY29sb3JNYXBbaV07XG4gICAgICB9XG4gICAgfSBlbHNlIHtcbiAgICAgIHRoaXMuX2lzRnJvemVuID0gZmFsc2U7XG4gICAgfVxuICB9XG4gIGdldElkKGNvbG9yKSB7XG4gICAgaWYgKGNvbG9yID09PSBudWxsKSB7XG4gICAgICByZXR1cm4gMDtcbiAgICB9XG4gICAgY29sb3IgPSBjb2xvci50b1VwcGVyQ2FzZSgpO1xuICAgIGxldCB2YWx1ZSA9IHRoaXMuX2NvbG9yMmlkW2NvbG9yXTtcbiAgICBpZiAodmFsdWUpIHtcbiAgICAgIHJldHVybiB2YWx1ZTtcbiAgICB9XG4gICAgaWYgKHRoaXMuX2lzRnJvemVuKSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoYE1pc3NpbmcgY29sb3IgaW4gY29sb3IgbWFwIC0gJHtjb2xvcn1gKTtcbiAgICB9XG4gICAgdmFsdWUgPSArK3RoaXMuX2xhc3RDb2xvcklkO1xuICAgIHRoaXMuX2NvbG9yMmlkW2NvbG9yXSA9IHZhbHVlO1xuICAgIHRoaXMuX2lkMmNvbG9yW3ZhbHVlXSA9IGNvbG9yO1xuICAgIHJldHVybiB2YWx1ZTtcbiAgfVxuICBnZXRDb2xvck1hcCgpIHtcbiAgICByZXR1cm4gdGhpcy5faWQyY29sb3Iuc2xpY2UoMCk7XG4gIH1cbn07XG52YXIgZW1wdHlQYXJlbnRTY29wZXMgPSBPYmplY3QuZnJlZXplKFtdKTtcbnZhciBUaGVtZVRyaWVFbGVtZW50UnVsZSA9IGNsYXNzIF9UaGVtZVRyaWVFbGVtZW50UnVsZSB7XG4gIHNjb3BlRGVwdGg7XG4gIHBhcmVudFNjb3BlcztcbiAgZm9udFN0eWxlO1xuICBmb3JlZ3JvdW5kO1xuICBiYWNrZ3JvdW5kO1xuICBjb25zdHJ1Y3RvcihzY29wZURlcHRoLCBwYXJlbnRTY29wZXMsIGZvbnRTdHlsZSwgZm9yZWdyb3VuZCwgYmFja2dyb3VuZCkge1xuICAgIHRoaXMuc2NvcGVEZXB0aCA9IHNjb3BlRGVwdGg7XG4gICAgdGhpcy5wYXJlbnRTY29wZXMgPSBwYXJlbnRTY29wZXMgfHwgZW1wdHlQYXJlbnRTY29wZXM7XG4gICAgdGhpcy5mb250U3R5bGUgPSBmb250U3R5bGU7XG4gICAgdGhpcy5mb3JlZ3JvdW5kID0gZm9yZWdyb3VuZDtcbiAgICB0aGlzLmJhY2tncm91bmQgPSBiYWNrZ3JvdW5kO1xuICB9XG4gIGNsb25lKCkge1xuICAgIHJldHVybiBuZXcgX1RoZW1lVHJpZUVsZW1lbnRSdWxlKHRoaXMuc2NvcGVEZXB0aCwgdGhpcy5wYXJlbnRTY29wZXMsIHRoaXMuZm9udFN0eWxlLCB0aGlzLmZvcmVncm91bmQsIHRoaXMuYmFja2dyb3VuZCk7XG4gIH1cbiAgc3RhdGljIGNsb25lQXJyKGFycikge1xuICAgIGxldCByID0gW107XG4gICAgZm9yIChsZXQgaSA9IDAsIGxlbiA9IGFyci5sZW5ndGg7IGkgPCBsZW47IGkrKykge1xuICAgICAgcltpXSA9IGFycltpXS5jbG9uZSgpO1xuICAgIH1cbiAgICByZXR1cm4gcjtcbiAgfVxuICBhY2NlcHRPdmVyd3JpdGUoc2NvcGVEZXB0aCwgZm9udFN0eWxlLCBmb3JlZ3JvdW5kLCBiYWNrZ3JvdW5kKSB7XG4gICAgaWYgKHRoaXMuc2NvcGVEZXB0aCA+IHNjb3BlRGVwdGgpIHtcbiAgICAgIGNvbnNvbGUubG9nKFwiaG93IGRpZCB0aGlzIGhhcHBlbj9cIik7XG4gICAgfSBlbHNlIHtcbiAgICAgIHRoaXMuc2NvcGVEZXB0aCA9IHNjb3BlRGVwdGg7XG4gICAgfVxuICAgIGlmIChmb250U3R5bGUgIT09IC0xIC8qIE5vdFNldCAqLykge1xuICAgICAgdGhpcy5mb250U3R5bGUgPSBmb250U3R5bGU7XG4gICAgfVxuICAgIGlmIChmb3JlZ3JvdW5kICE9PSAwKSB7XG4gICAgICB0aGlzLmZvcmVncm91bmQgPSBmb3JlZ3JvdW5kO1xuICAgIH1cbiAgICBpZiAoYmFja2dyb3VuZCAhPT0gMCkge1xuICAgICAgdGhpcy5iYWNrZ3JvdW5kID0gYmFja2dyb3VuZDtcbiAgICB9XG4gIH1cbn07XG52YXIgVGhlbWVUcmllRWxlbWVudCA9IGNsYXNzIF9UaGVtZVRyaWVFbGVtZW50IHtcbiAgY29uc3RydWN0b3IoX21haW5SdWxlLCBydWxlc1dpdGhQYXJlbnRTY29wZXMgPSBbXSwgX2NoaWxkcmVuID0ge30pIHtcbiAgICB0aGlzLl9tYWluUnVsZSA9IF9tYWluUnVsZTtcbiAgICB0aGlzLl9jaGlsZHJlbiA9IF9jaGlsZHJlbjtcbiAgICB0aGlzLl9ydWxlc1dpdGhQYXJlbnRTY29wZXMgPSBydWxlc1dpdGhQYXJlbnRTY29wZXM7XG4gIH1cbiAgX3J1bGVzV2l0aFBhcmVudFNjb3BlcztcbiAgc3RhdGljIF9jbXBCeVNwZWNpZmljaXR5KGEsIGIpIHtcbiAgICBpZiAoYS5zY29wZURlcHRoICE9PSBiLnNjb3BlRGVwdGgpIHtcbiAgICAgIHJldHVybiBiLnNjb3BlRGVwdGggLSBhLnNjb3BlRGVwdGg7XG4gICAgfVxuICAgIGxldCBhUGFyZW50SW5kZXggPSAwO1xuICAgIGxldCBiUGFyZW50SW5kZXggPSAwO1xuICAgIHdoaWxlICh0cnVlKSB7XG4gICAgICBpZiAoYS5wYXJlbnRTY29wZXNbYVBhcmVudEluZGV4XSA9PT0gXCI+XCIpIHtcbiAgICAgICAgYVBhcmVudEluZGV4Kys7XG4gICAgICB9XG4gICAgICBpZiAoYi5wYXJlbnRTY29wZXNbYlBhcmVudEluZGV4XSA9PT0gXCI+XCIpIHtcbiAgICAgICAgYlBhcmVudEluZGV4Kys7XG4gICAgICB9XG4gICAgICBpZiAoYVBhcmVudEluZGV4ID49IGEucGFyZW50U2NvcGVzLmxlbmd0aCB8fCBiUGFyZW50SW5kZXggPj0gYi5wYXJlbnRTY29wZXMubGVuZ3RoKSB7XG4gICAgICAgIGJyZWFrO1xuICAgICAgfVxuICAgICAgY29uc3QgcGFyZW50U2NvcGVMZW5ndGhEaWZmID0gYi5wYXJlbnRTY29wZXNbYlBhcmVudEluZGV4XS5sZW5ndGggLSBhLnBhcmVudFNjb3Blc1thUGFyZW50SW5kZXhdLmxlbmd0aDtcbiAgICAgIGlmIChwYXJlbnRTY29wZUxlbmd0aERpZmYgIT09IDApIHtcbiAgICAgICAgcmV0dXJuIHBhcmVudFNjb3BlTGVuZ3RoRGlmZjtcbiAgICAgIH1cbiAgICAgIGFQYXJlbnRJbmRleCsrO1xuICAgICAgYlBhcmVudEluZGV4Kys7XG4gICAgfVxuICAgIHJldHVybiBiLnBhcmVudFNjb3Blcy5sZW5ndGggLSBhLnBhcmVudFNjb3Blcy5sZW5ndGg7XG4gIH1cbiAgbWF0Y2goc2NvcGUpIHtcbiAgICBpZiAoc2NvcGUgIT09IFwiXCIpIHtcbiAgICAgIGxldCBkb3RJbmRleCA9IHNjb3BlLmluZGV4T2YoXCIuXCIpO1xuICAgICAgbGV0IGhlYWQ7XG4gICAgICBsZXQgdGFpbDtcbiAgICAgIGlmIChkb3RJbmRleCA9PT0gLTEpIHtcbiAgICAgICAgaGVhZCA9IHNjb3BlO1xuICAgICAgICB0YWlsID0gXCJcIjtcbiAgICAgIH0gZWxzZSB7XG4gICAgICAgIGhlYWQgPSBzY29wZS5zdWJzdHJpbmcoMCwgZG90SW5kZXgpO1xuICAgICAgICB0YWlsID0gc2NvcGUuc3Vic3RyaW5nKGRvdEluZGV4ICsgMSk7XG4gICAgICB9XG4gICAgICBpZiAodGhpcy5fY2hpbGRyZW4uaGFzT3duUHJvcGVydHkoaGVhZCkpIHtcbiAgICAgICAgcmV0dXJuIHRoaXMuX2NoaWxkcmVuW2hlYWRdLm1hdGNoKHRhaWwpO1xuICAgICAgfVxuICAgIH1cbiAgICBjb25zdCBydWxlcyA9IHRoaXMuX3J1bGVzV2l0aFBhcmVudFNjb3Blcy5jb25jYXQodGhpcy5fbWFpblJ1bGUpO1xuICAgIHJ1bGVzLnNvcnQoX1RoZW1lVHJpZUVsZW1lbnQuX2NtcEJ5U3BlY2lmaWNpdHkpO1xuICAgIHJldHVybiBydWxlcztcbiAgfVxuICBpbnNlcnQoc2NvcGVEZXB0aCwgc2NvcGUsIHBhcmVudFNjb3BlcywgZm9udFN0eWxlLCBmb3JlZ3JvdW5kLCBiYWNrZ3JvdW5kKSB7XG4gICAgaWYgKHNjb3BlID09PSBcIlwiKSB7XG4gICAgICB0aGlzLl9kb0luc2VydEhlcmUoc2NvcGVEZXB0aCwgcGFyZW50U2NvcGVzLCBmb250U3R5bGUsIGZvcmVncm91bmQsIGJhY2tncm91bmQpO1xuICAgICAgcmV0dXJuO1xuICAgIH1cbiAgICBsZXQgZG90SW5kZXggPSBzY29wZS5pbmRleE9mKFwiLlwiKTtcbiAgICBsZXQgaGVhZDtcbiAgICBsZXQgdGFpbDtcbiAgICBpZiAoZG90SW5kZXggPT09IC0xKSB7XG4gICAgICBoZWFkID0gc2NvcGU7XG4gICAgICB0YWlsID0gXCJcIjtcbiAgICB9IGVsc2Uge1xuICAgICAgaGVhZCA9IHNjb3BlLnN1YnN0cmluZygwLCBkb3RJbmRleCk7XG4gICAgICB0YWlsID0gc2NvcGUuc3Vic3RyaW5nKGRvdEluZGV4ICsgMSk7XG4gICAgfVxuICAgIGxldCBjaGlsZDtcbiAgICBpZiAodGhpcy5fY2hpbGRyZW4uaGFzT3duUHJvcGVydHkoaGVhZCkpIHtcbiAgICAgIGNoaWxkID0gdGhpcy5fY2hpbGRyZW5baGVhZF07XG4gICAgfSBlbHNlIHtcbiAgICAgIGNoaWxkID0gbmV3IF9UaGVtZVRyaWVFbGVtZW50KHRoaXMuX21haW5SdWxlLmNsb25lKCksIFRoZW1lVHJpZUVsZW1lbnRSdWxlLmNsb25lQXJyKHRoaXMuX3J1bGVzV2l0aFBhcmVudFNjb3BlcykpO1xuICAgICAgdGhpcy5fY2hpbGRyZW5baGVhZF0gPSBjaGlsZDtcbiAgICB9XG4gICAgY2hpbGQuaW5zZXJ0KHNjb3BlRGVwdGggKyAxLCB0YWlsLCBwYXJlbnRTY29wZXMsIGZvbnRTdHlsZSwgZm9yZWdyb3VuZCwgYmFja2dyb3VuZCk7XG4gIH1cbiAgX2RvSW5zZXJ0SGVyZShzY29wZURlcHRoLCBwYXJlbnRTY29wZXMsIGZvbnRTdHlsZSwgZm9yZWdyb3VuZCwgYmFja2dyb3VuZCkge1xuICAgIGlmIChwYXJlbnRTY29wZXMgPT09IG51bGwpIHtcbiAgICAgIHRoaXMuX21haW5SdWxlLmFjY2VwdE92ZXJ3cml0ZShzY29wZURlcHRoLCBmb250U3R5bGUsIGZvcmVncm91bmQsIGJhY2tncm91bmQpO1xuICAgICAgcmV0dXJuO1xuICAgIH1cbiAgICBmb3IgKGxldCBpID0gMCwgbGVuID0gdGhpcy5fcnVsZXNXaXRoUGFyZW50U2NvcGVzLmxlbmd0aDsgaSA8IGxlbjsgaSsrKSB7XG4gICAgICBsZXQgcnVsZSA9IHRoaXMuX3J1bGVzV2l0aFBhcmVudFNjb3Blc1tpXTtcbiAgICAgIGlmIChzdHJBcnJDbXAocnVsZS5wYXJlbnRTY29wZXMsIHBhcmVudFNjb3BlcykgPT09IDApIHtcbiAgICAgICAgcnVsZS5hY2NlcHRPdmVyd3JpdGUoc2NvcGVEZXB0aCwgZm9udFN0eWxlLCBmb3JlZ3JvdW5kLCBiYWNrZ3JvdW5kKTtcbiAgICAgICAgcmV0dXJuO1xuICAgICAgfVxuICAgIH1cbiAgICBpZiAoZm9udFN0eWxlID09PSAtMSAvKiBOb3RTZXQgKi8pIHtcbiAgICAgIGZvbnRTdHlsZSA9IHRoaXMuX21haW5SdWxlLmZvbnRTdHlsZTtcbiAgICB9XG4gICAgaWYgKGZvcmVncm91bmQgPT09IDApIHtcbiAgICAgIGZvcmVncm91bmQgPSB0aGlzLl9tYWluUnVsZS5mb3JlZ3JvdW5kO1xuICAgIH1cbiAgICBpZiAoYmFja2dyb3VuZCA9PT0gMCkge1xuICAgICAgYmFja2dyb3VuZCA9IHRoaXMuX21haW5SdWxlLmJhY2tncm91bmQ7XG4gICAgfVxuICAgIHRoaXMuX3J1bGVzV2l0aFBhcmVudFNjb3Blcy5wdXNoKG5ldyBUaGVtZVRyaWVFbGVtZW50UnVsZShzY29wZURlcHRoLCBwYXJlbnRTY29wZXMsIGZvbnRTdHlsZSwgZm9yZWdyb3VuZCwgYmFja2dyb3VuZCkpO1xuICB9XG59O1xuXG4vLyBzcmMvZW5jb2RlZFRva2VuQXR0cmlidXRlcy50c1xudmFyIEVuY29kZWRUb2tlbk1ldGFkYXRhID0gY2xhc3MgX0VuY29kZWRUb2tlbk1ldGFkYXRhIHtcbiAgc3RhdGljIHRvQmluYXJ5U3RyKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMpIHtcbiAgICByZXR1cm4gZW5jb2RlZFRva2VuQXR0cmlidXRlcy50b1N0cmluZygyKS5wYWRTdGFydCgzMiwgXCIwXCIpO1xuICB9XG4gIHN0YXRpYyBwcmludChlbmNvZGVkVG9rZW5BdHRyaWJ1dGVzKSB7XG4gICAgY29uc3QgbGFuZ3VhZ2VJZCA9IF9FbmNvZGVkVG9rZW5NZXRhZGF0YS5nZXRMYW5ndWFnZUlkKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMpO1xuICAgIGNvbnN0IHRva2VuVHlwZSA9IF9FbmNvZGVkVG9rZW5NZXRhZGF0YS5nZXRUb2tlblR5cGUoZW5jb2RlZFRva2VuQXR0cmlidXRlcyk7XG4gICAgY29uc3QgZm9udFN0eWxlID0gX0VuY29kZWRUb2tlbk1ldGFkYXRhLmdldEZvbnRTdHlsZShlbmNvZGVkVG9rZW5BdHRyaWJ1dGVzKTtcbiAgICBjb25zdCBmb3JlZ3JvdW5kID0gX0VuY29kZWRUb2tlbk1ldGFkYXRhLmdldEZvcmVncm91bmQoZW5jb2RlZFRva2VuQXR0cmlidXRlcyk7XG4gICAgY29uc3QgYmFja2dyb3VuZCA9IF9FbmNvZGVkVG9rZW5NZXRhZGF0YS5nZXRCYWNrZ3JvdW5kKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMpO1xuICAgIGNvbnNvbGUubG9nKHtcbiAgICAgIGxhbmd1YWdlSWQsXG4gICAgICB0b2tlblR5cGUsXG4gICAgICBmb250U3R5bGUsXG4gICAgICBmb3JlZ3JvdW5kLFxuICAgICAgYmFja2dyb3VuZFxuICAgIH0pO1xuICB9XG4gIHN0YXRpYyBnZXRMYW5ndWFnZUlkKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMpIHtcbiAgICByZXR1cm4gKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMgJiAyNTUgLyogTEFOR1VBR0VJRF9NQVNLICovKSA+Pj4gMCAvKiBMQU5HVUFHRUlEX09GRlNFVCAqLztcbiAgfVxuICBzdGF0aWMgZ2V0VG9rZW5UeXBlKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMpIHtcbiAgICByZXR1cm4gKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMgJiA3NjggLyogVE9LRU5fVFlQRV9NQVNLICovKSA+Pj4gOCAvKiBUT0tFTl9UWVBFX09GRlNFVCAqLztcbiAgfVxuICBzdGF0aWMgY29udGFpbnNCYWxhbmNlZEJyYWNrZXRzKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMpIHtcbiAgICByZXR1cm4gKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMgJiAxMDI0IC8qIEJBTEFOQ0VEX0JSQUNLRVRTX01BU0sgKi8pICE9PSAwO1xuICB9XG4gIHN0YXRpYyBnZXRGb250U3R5bGUoZW5jb2RlZFRva2VuQXR0cmlidXRlcykge1xuICAgIHJldHVybiAoZW5jb2RlZFRva2VuQXR0cmlidXRlcyAmIDMwNzIwIC8qIEZPTlRfU1RZTEVfTUFTSyAqLykgPj4+IDExIC8qIEZPTlRfU1RZTEVfT0ZGU0VUICovO1xuICB9XG4gIHN0YXRpYyBnZXRGb3JlZ3JvdW5kKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMpIHtcbiAgICByZXR1cm4gKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMgJiAxNjc0NDQ0OCAvKiBGT1JFR1JPVU5EX01BU0sgKi8pID4+PiAxNSAvKiBGT1JFR1JPVU5EX09GRlNFVCAqLztcbiAgfVxuICBzdGF0aWMgZ2V0QmFja2dyb3VuZChlbmNvZGVkVG9rZW5BdHRyaWJ1dGVzKSB7XG4gICAgcmV0dXJuIChlbmNvZGVkVG9rZW5BdHRyaWJ1dGVzICYgNDI3ODE5MDA4MCAvKiBCQUNLR1JPVU5EX01BU0sgKi8pID4+PiAyNCAvKiBCQUNLR1JPVU5EX09GRlNFVCAqLztcbiAgfVxuICAvKipcbiAgICogVXBkYXRlcyB0aGUgZmllbGRzIGluIGBtZXRhZGF0YWAuXG4gICAqIEEgdmFsdWUgb2YgYDBgLCBgTm90U2V0YCBvciBgbnVsbGAgaW5kaWNhdGVzIHRoYXQgdGhlIGNvcnJlc3BvbmRpbmcgZmllbGQgc2hvdWxkIGJlIGxlZnQgYXMgaXMuXG4gICAqL1xuICBzdGF0aWMgc2V0KGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMsIGxhbmd1YWdlSWQsIHRva2VuVHlwZSwgY29udGFpbnNCYWxhbmNlZEJyYWNrZXRzLCBmb250U3R5bGUsIGZvcmVncm91bmQsIGJhY2tncm91bmQpIHtcbiAgICBsZXQgX2xhbmd1YWdlSWQgPSBfRW5jb2RlZFRva2VuTWV0YWRhdGEuZ2V0TGFuZ3VhZ2VJZChlbmNvZGVkVG9rZW5BdHRyaWJ1dGVzKTtcbiAgICBsZXQgX3Rva2VuVHlwZSA9IF9FbmNvZGVkVG9rZW5NZXRhZGF0YS5nZXRUb2tlblR5cGUoZW5jb2RlZFRva2VuQXR0cmlidXRlcyk7XG4gICAgbGV0IF9jb250YWluc0JhbGFuY2VkQnJhY2tldHNCaXQgPSBfRW5jb2RlZFRva2VuTWV0YWRhdGEuY29udGFpbnNCYWxhbmNlZEJyYWNrZXRzKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMpID8gMSA6IDA7XG4gICAgbGV0IF9mb250U3R5bGUgPSBfRW5jb2RlZFRva2VuTWV0YWRhdGEuZ2V0Rm9udFN0eWxlKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMpO1xuICAgIGxldCBfZm9yZWdyb3VuZCA9IF9FbmNvZGVkVG9rZW5NZXRhZGF0YS5nZXRGb3JlZ3JvdW5kKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMpO1xuICAgIGxldCBfYmFja2dyb3VuZCA9IF9FbmNvZGVkVG9rZW5NZXRhZGF0YS5nZXRCYWNrZ3JvdW5kKGVuY29kZWRUb2tlbkF0dHJpYnV0ZXMpO1xuICAgIGlmIChsYW5ndWFnZUlkICE9PSAwKSB7XG4gICAgICBfbGFuZ3VhZ2VJZCA9IGxhbmd1YWdlSWQ7XG4gICAgfVxuICAgIGlmICh0b2tlblR5cGUgIT09IDggLyogTm90U2V0ICovKSB7XG4gICAgICBfdG9rZW5UeXBlID0gZnJvbU9wdGlvbmFsVG9rZW5UeXBlKHRva2VuVHlwZSk7XG4gICAgfVxuICAgIGlmIChjb250YWluc0JhbGFuY2VkQnJhY2tldHMgIT09IG51bGwpIHtcbiAgICAgIF9jb250YWluc0JhbGFuY2VkQnJhY2tldHNCaXQgPSBjb250YWluc0JhbGFuY2VkQnJhY2tldHMgPyAxIDogMDtcbiAgICB9XG4gICAgaWYgKGZvbnRTdHlsZSAhPT0gLTEgLyogTm90U2V0ICovKSB7XG4gICAgICBfZm9udFN0eWxlID0gZm9udFN0eWxlO1xuICAgIH1cbiAgICBpZiAoZm9yZWdyb3VuZCAhPT0gMCkge1xuICAgICAgX2ZvcmVncm91bmQgPSBmb3JlZ3JvdW5kO1xuICAgIH1cbiAgICBpZiAoYmFja2dyb3VuZCAhPT0gMCkge1xuICAgICAgX2JhY2tncm91bmQgPSBiYWNrZ3JvdW5kO1xuICAgIH1cbiAgICByZXR1cm4gKF9sYW5ndWFnZUlkIDw8IDAgLyogTEFOR1VBR0VJRF9PRkZTRVQgKi8gfCBfdG9rZW5UeXBlIDw8IDggLyogVE9LRU5fVFlQRV9PRkZTRVQgKi8gfCBfY29udGFpbnNCYWxhbmNlZEJyYWNrZXRzQml0IDw8IDEwIC8qIEJBTEFOQ0VEX0JSQUNLRVRTX09GRlNFVCAqLyB8IF9mb250U3R5bGUgPDwgMTEgLyogRk9OVF9TVFlMRV9PRkZTRVQgKi8gfCBfZm9yZWdyb3VuZCA8PCAxNSAvKiBGT1JFR1JPVU5EX09GRlNFVCAqLyB8IF9iYWNrZ3JvdW5kIDw8IDI0IC8qIEJBQ0tHUk9VTkRfT0ZGU0VUICovKSA+Pj4gMDtcbiAgfVxufTtcbmZ1bmN0aW9uIHRvT3B0aW9uYWxUb2tlblR5cGUoc3RhbmRhcmRUeXBlKSB7XG4gIHJldHVybiBzdGFuZGFyZFR5cGU7XG59XG5mdW5jdGlvbiBmcm9tT3B0aW9uYWxUb2tlblR5cGUoc3RhbmRhcmRUeXBlKSB7XG4gIHJldHVybiBzdGFuZGFyZFR5cGU7XG59XG5cbi8vIHNyYy9tYXRjaGVyLnRzXG5mdW5jdGlvbiBjcmVhdGVNYXRjaGVycyhzZWxlY3RvciwgbWF0Y2hlc05hbWUpIHtcbiAgY29uc3QgcmVzdWx0cyA9IFtdO1xuICBjb25zdCB0b2tlbml6ZXIgPSBuZXdUb2tlbml6ZXIoc2VsZWN0b3IpO1xuICBsZXQgdG9rZW4gPSB0b2tlbml6ZXIubmV4dCgpO1xuICB3aGlsZSAodG9rZW4gIT09IG51bGwpIHtcbiAgICBsZXQgcHJpb3JpdHkgPSAwO1xuICAgIGlmICh0b2tlbi5sZW5ndGggPT09IDIgJiYgdG9rZW4uY2hhckF0KDEpID09PSBcIjpcIikge1xuICAgICAgc3dpdGNoICh0b2tlbi5jaGFyQXQoMCkpIHtcbiAgICAgICAgY2FzZSBcIlJcIjpcbiAgICAgICAgICBwcmlvcml0eSA9IDE7XG4gICAgICAgICAgYnJlYWs7XG4gICAgICAgIGNhc2UgXCJMXCI6XG4gICAgICAgICAgcHJpb3JpdHkgPSAtMTtcbiAgICAgICAgICBicmVhaztcbiAgICAgICAgZGVmYXVsdDpcbiAgICAgICAgICBjb25zb2xlLmxvZyhgVW5rbm93biBwcmlvcml0eSAke3Rva2VufSBpbiBzY29wZSBzZWxlY3RvcmApO1xuICAgICAgfVxuICAgICAgdG9rZW4gPSB0b2tlbml6ZXIubmV4dCgpO1xuICAgIH1cbiAgICBsZXQgbWF0Y2hlciA9IHBhcnNlQ29uanVuY3Rpb24oKTtcbiAgICByZXN1bHRzLnB1c2goeyBtYXRjaGVyLCBwcmlvcml0eSB9KTtcbiAgICBpZiAodG9rZW4gIT09IFwiLFwiKSB7XG4gICAgICBicmVhaztcbiAgICB9XG4gICAgdG9rZW4gPSB0b2tlbml6ZXIubmV4dCgpO1xuICB9XG4gIHJldHVybiByZXN1bHRzO1xuICBmdW5jdGlvbiBwYXJzZU9wZXJhbmQoKSB7XG4gICAgaWYgKHRva2VuID09PSBcIi1cIikge1xuICAgICAgdG9rZW4gPSB0b2tlbml6ZXIubmV4dCgpO1xuICAgICAgY29uc3QgZXhwcmVzc2lvblRvTmVnYXRlID0gcGFyc2VPcGVyYW5kKCk7XG4gICAgICByZXR1cm4gKG1hdGNoZXJJbnB1dCkgPT4gISFleHByZXNzaW9uVG9OZWdhdGUgJiYgIWV4cHJlc3Npb25Ub05lZ2F0ZShtYXRjaGVySW5wdXQpO1xuICAgIH1cbiAgICBpZiAodG9rZW4gPT09IFwiKFwiKSB7XG4gICAgICB0b2tlbiA9IHRva2VuaXplci5uZXh0KCk7XG4gICAgICBjb25zdCBleHByZXNzaW9uSW5QYXJlbnRzID0gcGFyc2VJbm5lckV4cHJlc3Npb24oKTtcbiAgICAgIGlmICh0b2tlbiA9PT0gXCIpXCIpIHtcbiAgICAgICAgdG9rZW4gPSB0b2tlbml6ZXIubmV4dCgpO1xuICAgICAgfVxuICAgICAgcmV0dXJuIGV4cHJlc3Npb25JblBhcmVudHM7XG4gICAgfVxuICAgIGlmIChpc0lkZW50aWZpZXIodG9rZW4pKSB7XG4gICAgICBjb25zdCBpZGVudGlmaWVycyA9IFtdO1xuICAgICAgZG8ge1xuICAgICAgICBpZGVudGlmaWVycy5wdXNoKHRva2VuKTtcbiAgICAgICAgdG9rZW4gPSB0b2tlbml6ZXIubmV4dCgpO1xuICAgICAgfSB3aGlsZSAoaXNJZGVudGlmaWVyKHRva2VuKSk7XG4gICAgICByZXR1cm4gKG1hdGNoZXJJbnB1dCkgPT4gbWF0Y2hlc05hbWUoaWRlbnRpZmllcnMsIG1hdGNoZXJJbnB1dCk7XG4gICAgfVxuICAgIHJldHVybiBudWxsO1xuICB9XG4gIGZ1bmN0aW9uIHBhcnNlQ29uanVuY3Rpb24oKSB7XG4gICAgY29uc3QgbWF0Y2hlcnMgPSBbXTtcbiAgICBsZXQgbWF0Y2hlciA9IHBhcnNlT3BlcmFuZCgpO1xuICAgIHdoaWxlIChtYXRjaGVyKSB7XG4gICAgICBtYXRjaGVycy5wdXNoKG1hdGNoZXIpO1xuICAgICAgbWF0Y2hlciA9IHBhcnNlT3BlcmFuZCgpO1xuICAgIH1cbiAgICByZXR1cm4gKG1hdGNoZXJJbnB1dCkgPT4gbWF0Y2hlcnMuZXZlcnkoKG1hdGNoZXIyKSA9PiBtYXRjaGVyMihtYXRjaGVySW5wdXQpKTtcbiAgfVxuICBmdW5jdGlvbiBwYXJzZUlubmVyRXhwcmVzc2lvbigpIHtcbiAgICBjb25zdCBtYXRjaGVycyA9IFtdO1xuICAgIGxldCBtYXRjaGVyID0gcGFyc2VDb25qdW5jdGlvbigpO1xuICAgIHdoaWxlIChtYXRjaGVyKSB7XG4gICAgICBtYXRjaGVycy5wdXNoKG1hdGNoZXIpO1xuICAgICAgaWYgKHRva2VuID09PSBcInxcIiB8fCB0b2tlbiA9PT0gXCIsXCIpIHtcbiAgICAgICAgZG8ge1xuICAgICAgICAgIHRva2VuID0gdG9rZW5pemVyLm5leHQoKTtcbiAgICAgICAgfSB3aGlsZSAodG9rZW4gPT09IFwifFwiIHx8IHRva2VuID09PSBcIixcIik7XG4gICAgICB9IGVsc2Uge1xuICAgICAgICBicmVhaztcbiAgICAgIH1cbiAgICAgIG1hdGNoZXIgPSBwYXJzZUNvbmp1bmN0aW9uKCk7XG4gICAgfVxuICAgIHJldHVybiAobWF0Y2hlcklucHV0KSA9PiBtYXRjaGVycy5zb21lKChtYXRjaGVyMikgPT4gbWF0Y2hlcjIobWF0Y2hlcklucHV0KSk7XG4gIH1cbn1cbmZ1bmN0aW9uIGlzSWRlbnRpZmllcih0b2tlbikge1xuICByZXR1cm4gISF0b2tlbiAmJiAhIXRva2VuLm1hdGNoKC9bXFx3XFwuOl0rLyk7XG59XG5mdW5jdGlvbiBuZXdUb2tlbml6ZXIoaW5wdXQpIHtcbiAgbGV0IHJlZ2V4ID0gLyhbTFJdOnxbXFx3XFwuOl1bXFx3XFwuOlxcLV0qfFtcXCxcXHxcXC1cXChcXCldKS9nO1xuICBsZXQgbWF0Y2ggPSByZWdleC5leGVjKGlucHV0KTtcbiAgcmV0dXJuIHtcbiAgICBuZXh0OiAoKSA9PiB7XG4gICAgICBpZiAoIW1hdGNoKSB7XG4gICAgICAgIHJldHVybiBudWxsO1xuICAgICAgfVxuICAgICAgY29uc3QgcmVzID0gbWF0Y2hbMF07XG4gICAgICBtYXRjaCA9IHJlZ2V4LmV4ZWMoaW5wdXQpO1xuICAgICAgcmV0dXJuIHJlcztcbiAgICB9XG4gIH07XG59XG5cbi8vIHNyYy9vbmlnTGliLnRzXG52YXIgRmluZE9wdGlvbiA9IC8qIEBfX1BVUkVfXyAqLyAoKEZpbmRPcHRpb24yKSA9PiB7XG4gIEZpbmRPcHRpb24yW0ZpbmRPcHRpb24yW1wiTm9uZVwiXSA9IDBdID0gXCJOb25lXCI7XG4gIEZpbmRPcHRpb24yW0ZpbmRPcHRpb24yW1wiTm90QmVnaW5TdHJpbmdcIl0gPSAxXSA9IFwiTm90QmVnaW5TdHJpbmdcIjtcbiAgRmluZE9wdGlvbjJbRmluZE9wdGlvbjJbXCJOb3RFbmRTdHJpbmdcIl0gPSAyXSA9IFwiTm90RW5kU3RyaW5nXCI7XG4gIEZpbmRPcHRpb24yW0ZpbmRPcHRpb24yW1wiTm90QmVnaW5Qb3NpdGlvblwiXSA9IDRdID0gXCJOb3RCZWdpblBvc2l0aW9uXCI7XG4gIEZpbmRPcHRpb24yW0ZpbmRPcHRpb24yW1wiRGVidWdDYWxsXCJdID0gOF0gPSBcIkRlYnVnQ2FsbFwiO1xuICByZXR1cm4gRmluZE9wdGlvbjI7XG59KShGaW5kT3B0aW9uIHx8IHt9KTtcbmZ1bmN0aW9uIGRpc3Bvc2VPbmlnU3RyaW5nKHN0cikge1xuICBpZiAodHlwZW9mIHN0ci5kaXNwb3NlID09PSBcImZ1bmN0aW9uXCIpIHtcbiAgICBzdHIuZGlzcG9zZSgpO1xuICB9XG59XG5cbi8vIHNyYy9ncmFtbWFyL2dyYW1tYXJEZXBlbmRlbmNpZXMudHNcbnZhciBUb3BMZXZlbFJ1bGVSZWZlcmVuY2UgPSBjbGFzcyB7XG4gIGNvbnN0cnVjdG9yKHNjb3BlTmFtZSkge1xuICAgIHRoaXMuc2NvcGVOYW1lID0gc2NvcGVOYW1lO1xuICB9XG4gIHRvS2V5KCkge1xuICAgIHJldHVybiB0aGlzLnNjb3BlTmFtZTtcbiAgfVxufTtcbnZhciBUb3BMZXZlbFJlcG9zaXRvcnlSdWxlUmVmZXJlbmNlID0gY2xhc3Mge1xuICBjb25zdHJ1Y3RvcihzY29wZU5hbWUsIHJ1bGVOYW1lKSB7XG4gICAgdGhpcy5zY29wZU5hbWUgPSBzY29wZU5hbWU7XG4gICAgdGhpcy5ydWxlTmFtZSA9IHJ1bGVOYW1lO1xuICB9XG4gIHRvS2V5KCkge1xuICAgIHJldHVybiBgJHt0aGlzLnNjb3BlTmFtZX0jJHt0aGlzLnJ1bGVOYW1lfWA7XG4gIH1cbn07XG52YXIgRXh0ZXJuYWxSZWZlcmVuY2VDb2xsZWN0b3IgPSBjbGFzcyB7XG4gIF9yZWZlcmVuY2VzID0gW107XG4gIF9zZWVuUmVmZXJlbmNlS2V5cyA9IC8qIEBfX1BVUkVfXyAqLyBuZXcgU2V0KCk7XG4gIGdldCByZWZlcmVuY2VzKCkge1xuICAgIHJldHVybiB0aGlzLl9yZWZlcmVuY2VzO1xuICB9XG4gIHZpc2l0ZWRSdWxlID0gLyogQF9fUFVSRV9fICovIG5ldyBTZXQoKTtcbiAgYWRkKHJlZmVyZW5jZSkge1xuICAgIGNvbnN0IGtleSA9IHJlZmVyZW5jZS50b0tleSgpO1xuICAgIGlmICh0aGlzLl9zZWVuUmVmZXJlbmNlS2V5cy5oYXMoa2V5KSkge1xuICAgICAgcmV0dXJuO1xuICAgIH1cbiAgICB0aGlzLl9zZWVuUmVmZXJlbmNlS2V5cy5hZGQoa2V5KTtcbiAgICB0aGlzLl9yZWZlcmVuY2VzLnB1c2gocmVmZXJlbmNlKTtcbiAgfVxufTtcbnZhciBTY29wZURlcGVuZGVuY3lQcm9jZXNzb3IgPSBjbGFzcyB7XG4gIGNvbnN0cnVjdG9yKHJlcG8sIGluaXRpYWxTY29wZU5hbWUpIHtcbiAgICB0aGlzLnJlcG8gPSByZXBvO1xuICAgIHRoaXMuaW5pdGlhbFNjb3BlTmFtZSA9IGluaXRpYWxTY29wZU5hbWU7XG4gICAgdGhpcy5zZWVuRnVsbFNjb3BlUmVxdWVzdHMuYWRkKHRoaXMuaW5pdGlhbFNjb3BlTmFtZSk7XG4gICAgdGhpcy5RID0gW25ldyBUb3BMZXZlbFJ1bGVSZWZlcmVuY2UodGhpcy5pbml0aWFsU2NvcGVOYW1lKV07XG4gIH1cbiAgc2VlbkZ1bGxTY29wZVJlcXVlc3RzID0gLyogQF9fUFVSRV9fICovIG5ldyBTZXQoKTtcbiAgc2VlblBhcnRpYWxTY29wZVJlcXVlc3RzID0gLyogQF9fUFVSRV9fICovIG5ldyBTZXQoKTtcbiAgUTtcbiAgcHJvY2Vzc1F1ZXVlKCkge1xuICAgIGNvbnN0IHEgPSB0aGlzLlE7XG4gICAgdGhpcy5RID0gW107XG4gICAgY29uc3QgZGVwcyA9IG5ldyBFeHRlcm5hbFJlZmVyZW5jZUNvbGxlY3RvcigpO1xuICAgIGZvciAoY29uc3QgZGVwIG9mIHEpIHtcbiAgICAgIGNvbGxlY3RSZWZlcmVuY2VzT2ZSZWZlcmVuY2UoZGVwLCB0aGlzLmluaXRpYWxTY29wZU5hbWUsIHRoaXMucmVwbywgZGVwcyk7XG4gICAgfVxuICAgIGZvciAoY29uc3QgZGVwIG9mIGRlcHMucmVmZXJlbmNlcykge1xuICAgICAgaWYgKGRlcCBpbnN0YW5jZW9mIFRvcExldmVsUnVsZVJlZmVyZW5jZSkge1xuICAgICAgICBpZiAodGhpcy5zZWVuRnVsbFNjb3BlUmVxdWVzdHMuaGFzKGRlcC5zY29wZU5hbWUpKSB7XG4gICAgICAgICAgY29udGludWU7XG4gICAgICAgIH1cbiAgICAgICAgdGhpcy5zZWVuRnVsbFNjb3BlUmVxdWVzdHMuYWRkKGRlcC5zY29wZU5hbWUpO1xuICAgICAgICB0aGlzLlEucHVzaChkZXApO1xuICAgICAgfSBlbHNlIHtcbiAgICAgICAgaWYgKHRoaXMuc2VlbkZ1bGxTY29wZVJlcXVlc3RzLmhhcyhkZXAuc2NvcGVOYW1lKSkge1xuICAgICAgICAgIGNvbnRpbnVlO1xuICAgICAgICB9XG4gICAgICAgIGlmICh0aGlzLnNlZW5QYXJ0aWFsU2NvcGVSZXF1ZXN0cy5oYXMoZGVwLnRvS2V5KCkpKSB7XG4gICAgICAgICAgY29udGludWU7XG4gICAgICAgIH1cbiAgICAgICAgdGhpcy5zZWVuUGFydGlhbFNjb3BlUmVxdWVzdHMuYWRkKGRlcC50b0tleSgpKTtcbiAgICAgICAgdGhpcy5RLnB1c2goZGVwKTtcbiAgICAgIH1cbiAgICB9XG4gIH1cbn07XG5mdW5jdGlvbiBjb2xsZWN0UmVmZXJlbmNlc09mUmVmZXJlbmNlKHJlZmVyZW5jZSwgYmFzZUdyYW1tYXJTY29wZU5hbWUsIHJlcG8sIHJlc3VsdCkge1xuICBjb25zdCBzZWxmR3JhbW1hciA9IHJlcG8ubG9va3VwKHJlZmVyZW5jZS5zY29wZU5hbWUpO1xuICBpZiAoIXNlbGZHcmFtbWFyKSB7XG4gICAgaWYgKHJlZmVyZW5jZS5zY29wZU5hbWUgPT09IGJhc2VHcmFtbWFyU2NvcGVOYW1lKSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoYE5vIGdyYW1tYXIgcHJvdmlkZWQgZm9yIDwke2Jhc2VHcmFtbWFyU2NvcGVOYW1lfT5gKTtcbiAgICB9XG4gICAgcmV0dXJuO1xuICB9XG4gIGNvbnN0IGJhc2VHcmFtbWFyID0gcmVwby5sb29rdXAoYmFzZUdyYW1tYXJTY29wZU5hbWUpO1xuICBpZiAocmVmZXJlbmNlIGluc3RhbmNlb2YgVG9wTGV2ZWxSdWxlUmVmZXJlbmNlKSB7XG4gICAgY29sbGVjdEV4dGVybmFsUmVmZXJlbmNlc0luVG9wTGV2ZWxSdWxlKHsgYmFzZUdyYW1tYXIsIHNlbGZHcmFtbWFyIH0sIHJlc3VsdCk7XG4gIH0gZWxzZSB7XG4gICAgY29sbGVjdEV4dGVybmFsUmVmZXJlbmNlc0luVG9wTGV2ZWxSZXBvc2l0b3J5UnVsZShcbiAgICAgIHJlZmVyZW5jZS5ydWxlTmFtZSxcbiAgICAgIHsgYmFzZUdyYW1tYXIsIHNlbGZHcmFtbWFyLCByZXBvc2l0b3J5OiBzZWxmR3JhbW1hci5yZXBvc2l0b3J5IH0sXG4gICAgICByZXN1bHRcbiAgICApO1xuICB9XG4gIGNvbnN0IGluamVjdGlvbnMgPSByZXBvLmluamVjdGlvbnMocmVmZXJlbmNlLnNjb3BlTmFtZSk7XG4gIGlmIChpbmplY3Rpb25zKSB7XG4gICAgZm9yIChjb25zdCBpbmplY3Rpb24gb2YgaW5qZWN0aW9ucykge1xuICAgICAgcmVzdWx0LmFkZChuZXcgVG9wTGV2ZWxSdWxlUmVmZXJlbmNlKGluamVjdGlvbikpO1xuICAgIH1cbiAgfVxufVxuZnVuY3Rpb24gY29sbGVjdEV4dGVybmFsUmVmZXJlbmNlc0luVG9wTGV2ZWxSZXBvc2l0b3J5UnVsZShydWxlTmFtZSwgY29udGV4dCwgcmVzdWx0KSB7XG4gIGlmIChjb250ZXh0LnJlcG9zaXRvcnkgJiYgY29udGV4dC5yZXBvc2l0b3J5W3J1bGVOYW1lXSkge1xuICAgIGNvbnN0IHJ1bGUgPSBjb250ZXh0LnJlcG9zaXRvcnlbcnVsZU5hbWVdO1xuICAgIGNvbGxlY3RFeHRlcm5hbFJlZmVyZW5jZXNJblJ1bGVzKFtydWxlXSwgY29udGV4dCwgcmVzdWx0KTtcbiAgfVxufVxuZnVuY3Rpb24gY29sbGVjdEV4dGVybmFsUmVmZXJlbmNlc0luVG9wTGV2ZWxSdWxlKGNvbnRleHQsIHJlc3VsdCkge1xuICBpZiAoY29udGV4dC5zZWxmR3JhbW1hci5wYXR0ZXJucyAmJiBBcnJheS5pc0FycmF5KGNvbnRleHQuc2VsZkdyYW1tYXIucGF0dGVybnMpKSB7XG4gICAgY29sbGVjdEV4dGVybmFsUmVmZXJlbmNlc0luUnVsZXMoXG4gICAgICBjb250ZXh0LnNlbGZHcmFtbWFyLnBhdHRlcm5zLFxuICAgICAgeyAuLi5jb250ZXh0LCByZXBvc2l0b3J5OiBjb250ZXh0LnNlbGZHcmFtbWFyLnJlcG9zaXRvcnkgfSxcbiAgICAgIHJlc3VsdFxuICAgICk7XG4gIH1cbiAgaWYgKGNvbnRleHQuc2VsZkdyYW1tYXIuaW5qZWN0aW9ucykge1xuICAgIGNvbGxlY3RFeHRlcm5hbFJlZmVyZW5jZXNJblJ1bGVzKFxuICAgICAgT2JqZWN0LnZhbHVlcyhjb250ZXh0LnNlbGZHcmFtbWFyLmluamVjdGlvbnMpLFxuICAgICAgeyAuLi5jb250ZXh0LCByZXBvc2l0b3J5OiBjb250ZXh0LnNlbGZHcmFtbWFyLnJlcG9zaXRvcnkgfSxcbiAgICAgIHJlc3VsdFxuICAgICk7XG4gIH1cbn1cbmZ1bmN0aW9uIGNvbGxlY3RFeHRlcm5hbFJlZmVyZW5jZXNJblJ1bGVzKHJ1bGVzLCBjb250ZXh0LCByZXN1bHQpIHtcbiAgZm9yIChjb25zdCBydWxlIG9mIHJ1bGVzKSB7XG4gICAgaWYgKHJlc3VsdC52aXNpdGVkUnVsZS5oYXMocnVsZSkpIHtcbiAgICAgIGNvbnRpbnVlO1xuICAgIH1cbiAgICByZXN1bHQudmlzaXRlZFJ1bGUuYWRkKHJ1bGUpO1xuICAgIGNvbnN0IHBhdHRlcm5SZXBvc2l0b3J5ID0gcnVsZS5yZXBvc2l0b3J5ID8gbWVyZ2VPYmplY3RzKHt9LCBjb250ZXh0LnJlcG9zaXRvcnksIHJ1bGUucmVwb3NpdG9yeSkgOiBjb250ZXh0LnJlcG9zaXRvcnk7XG4gICAgaWYgKEFycmF5LmlzQXJyYXkocnVsZS5wYXR0ZXJucykpIHtcbiAgICAgIGNvbGxlY3RFeHRlcm5hbFJlZmVyZW5jZXNJblJ1bGVzKHJ1bGUucGF0dGVybnMsIHsgLi4uY29udGV4dCwgcmVwb3NpdG9yeTogcGF0dGVyblJlcG9zaXRvcnkgfSwgcmVzdWx0KTtcbiAgICB9XG4gICAgY29uc3QgaW5jbHVkZSA9IHJ1bGUuaW5jbHVkZTtcbiAgICBpZiAoIWluY2x1ZGUpIHtcbiAgICAgIGNvbnRpbnVlO1xuICAgIH1cbiAgICBjb25zdCByZWZlcmVuY2UgPSBwYXJzZUluY2x1ZGUoaW5jbHVkZSk7XG4gICAgc3dpdGNoIChyZWZlcmVuY2Uua2luZCkge1xuICAgICAgY2FzZSAwIC8qIEJhc2UgKi86XG4gICAgICAgIGNvbGxlY3RFeHRlcm5hbFJlZmVyZW5jZXNJblRvcExldmVsUnVsZSh7IC4uLmNvbnRleHQsIHNlbGZHcmFtbWFyOiBjb250ZXh0LmJhc2VHcmFtbWFyIH0sIHJlc3VsdCk7XG4gICAgICAgIGJyZWFrO1xuICAgICAgY2FzZSAxIC8qIFNlbGYgKi86XG4gICAgICAgIGNvbGxlY3RFeHRlcm5hbFJlZmVyZW5jZXNJblRvcExldmVsUnVsZShjb250ZXh0LCByZXN1bHQpO1xuICAgICAgICBicmVhaztcbiAgICAgIGNhc2UgMiAvKiBSZWxhdGl2ZVJlZmVyZW5jZSAqLzpcbiAgICAgICAgY29sbGVjdEV4dGVybmFsUmVmZXJlbmNlc0luVG9wTGV2ZWxSZXBvc2l0b3J5UnVsZShyZWZlcmVuY2UucnVsZU5hbWUsIHsgLi4uY29udGV4dCwgcmVwb3NpdG9yeTogcGF0dGVyblJlcG9zaXRvcnkgfSwgcmVzdWx0KTtcbiAgICAgICAgYnJlYWs7XG4gICAgICBjYXNlIDMgLyogVG9wTGV2ZWxSZWZlcmVuY2UgKi86XG4gICAgICBjYXNlIDQgLyogVG9wTGV2ZWxSZXBvc2l0b3J5UmVmZXJlbmNlICovOlxuICAgICAgICBjb25zdCBzZWxmR3JhbW1hciA9IHJlZmVyZW5jZS5zY29wZU5hbWUgPT09IGNvbnRleHQuc2VsZkdyYW1tYXIuc2NvcGVOYW1lID8gY29udGV4dC5zZWxmR3JhbW1hciA6IHJlZmVyZW5jZS5zY29wZU5hbWUgPT09IGNvbnRleHQuYmFzZUdyYW1tYXIuc2NvcGVOYW1lID8gY29udGV4dC5iYXNlR3JhbW1hciA6IHZvaWQgMDtcbiAgICAgICAgaWYgKHNlbGZHcmFtbWFyKSB7XG4gICAgICAgICAgY29uc3QgbmV3Q29udGV4dCA9IHsgYmFzZUdyYW1tYXI6IGNvbnRleHQuYmFzZUdyYW1tYXIsIHNlbGZHcmFtbWFyLCByZXBvc2l0b3J5OiBwYXR0ZXJuUmVwb3NpdG9yeSB9O1xuICAgICAgICAgIGlmIChyZWZlcmVuY2Uua2luZCA9PT0gNCAvKiBUb3BMZXZlbFJlcG9zaXRvcnlSZWZlcmVuY2UgKi8pIHtcbiAgICAgICAgICAgIGNvbGxlY3RFeHRlcm5hbFJlZmVyZW5jZXNJblRvcExldmVsUmVwb3NpdG9yeVJ1bGUocmVmZXJlbmNlLnJ1bGVOYW1lLCBuZXdDb250ZXh0LCByZXN1bHQpO1xuICAgICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgICBjb2xsZWN0RXh0ZXJuYWxSZWZlcmVuY2VzSW5Ub3BMZXZlbFJ1bGUobmV3Q29udGV4dCwgcmVzdWx0KTtcbiAgICAgICAgICB9XG4gICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgaWYgKHJlZmVyZW5jZS5raW5kID09PSA0IC8qIFRvcExldmVsUmVwb3NpdG9yeVJlZmVyZW5jZSAqLykge1xuICAgICAgICAgICAgcmVzdWx0LmFkZChuZXcgVG9wTGV2ZWxSZXBvc2l0b3J5UnVsZVJlZmVyZW5jZShyZWZlcmVuY2Uuc2NvcGVOYW1lLCByZWZlcmVuY2UucnVsZU5hbWUpKTtcbiAgICAgICAgICB9IGVsc2Uge1xuICAgICAgICAgICAgcmVzdWx0LmFkZChuZXcgVG9wTGV2ZWxSdWxlUmVmZXJlbmNlKHJlZmVyZW5jZS5zY29wZU5hbWUpKTtcbiAgICAgICAgICB9XG4gICAgICAgIH1cbiAgICAgICAgYnJlYWs7XG4gICAgfVxuICB9XG59XG52YXIgQmFzZVJlZmVyZW5jZSA9IGNsYXNzIHtcbiAga2luZCA9IDAgLyogQmFzZSAqLztcbn07XG52YXIgU2VsZlJlZmVyZW5jZSA9IGNsYXNzIHtcbiAga2luZCA9IDEgLyogU2VsZiAqLztcbn07XG52YXIgUmVsYXRpdmVSZWZlcmVuY2UgPSBjbGFzcyB7XG4gIGNvbnN0cnVjdG9yKHJ1bGVOYW1lKSB7XG4gICAgdGhpcy5ydWxlTmFtZSA9IHJ1bGVOYW1lO1xuICB9XG4gIGtpbmQgPSAyIC8qIFJlbGF0aXZlUmVmZXJlbmNlICovO1xufTtcbnZhciBUb3BMZXZlbFJlZmVyZW5jZSA9IGNsYXNzIHtcbiAgY29uc3RydWN0b3Ioc2NvcGVOYW1lKSB7XG4gICAgdGhpcy5zY29wZU5hbWUgPSBzY29wZU5hbWU7XG4gIH1cbiAga2luZCA9IDMgLyogVG9wTGV2ZWxSZWZlcmVuY2UgKi87XG59O1xudmFyIFRvcExldmVsUmVwb3NpdG9yeVJlZmVyZW5jZSA9IGNsYXNzIHtcbiAgY29uc3RydWN0b3Ioc2NvcGVOYW1lLCBydWxlTmFtZSkge1xuICAgIHRoaXMuc2NvcGVOYW1lID0gc2NvcGVOYW1lO1xuICAgIHRoaXMucnVsZU5hbWUgPSBydWxlTmFtZTtcbiAgfVxuICBraW5kID0gNCAvKiBUb3BMZXZlbFJlcG9zaXRvcnlSZWZlcmVuY2UgKi87XG59O1xuZnVuY3Rpb24gcGFyc2VJbmNsdWRlKGluY2x1ZGUpIHtcbiAgaWYgKGluY2x1ZGUgPT09IFwiJGJhc2VcIikge1xuICAgIHJldHVybiBuZXcgQmFzZVJlZmVyZW5jZSgpO1xuICB9IGVsc2UgaWYgKGluY2x1ZGUgPT09IFwiJHNlbGZcIikge1xuICAgIHJldHVybiBuZXcgU2VsZlJlZmVyZW5jZSgpO1xuICB9XG4gIGNvbnN0IGluZGV4T2ZTaGFycCA9IGluY2x1ZGUuaW5kZXhPZihcIiNcIik7XG4gIGlmIChpbmRleE9mU2hhcnAgPT09IC0xKSB7XG4gICAgcmV0dXJuIG5ldyBUb3BMZXZlbFJlZmVyZW5jZShpbmNsdWRlKTtcbiAgfSBlbHNlIGlmIChpbmRleE9mU2hhcnAgPT09IDApIHtcbiAgICByZXR1cm4gbmV3IFJlbGF0aXZlUmVmZXJlbmNlKGluY2x1ZGUuc3Vic3RyaW5nKDEpKTtcbiAgfSBlbHNlIHtcbiAgICBjb25zdCBzY29wZU5hbWUgPSBpbmNsdWRlLnN1YnN0cmluZygwLCBpbmRleE9mU2hhcnApO1xuICAgIGNvbnN0IHJ1bGVOYW1lID0gaW5jbHVkZS5zdWJzdHJpbmcoaW5kZXhPZlNoYXJwICsgMSk7XG4gICAgcmV0dXJuIG5ldyBUb3BMZXZlbFJlcG9zaXRvcnlSZWZlcmVuY2Uoc2NvcGVOYW1lLCBydWxlTmFtZSk7XG4gIH1cbn1cblxuLy8gc3JjL3J1bGUudHNcbnZhciBIQVNfQkFDS19SRUZFUkVOQ0VTID0gL1xcXFwoXFxkKykvO1xudmFyIEJBQ0tfUkVGRVJFTkNJTkdfRU5EID0gL1xcXFwoXFxkKykvZztcbnZhciBydWxlSWRTeW1ib2wgPSBTeW1ib2woXCJSdWxlSWRcIik7XG52YXIgZW5kUnVsZUlkID0gLTE7XG52YXIgd2hpbGVSdWxlSWQgPSAtMjtcbmZ1bmN0aW9uIHJ1bGVJZEZyb21OdW1iZXIoaWQpIHtcbiAgcmV0dXJuIGlkO1xufVxuZnVuY3Rpb24gcnVsZUlkVG9OdW1iZXIoaWQpIHtcbiAgcmV0dXJuIGlkO1xufVxudmFyIFJ1bGUgPSBjbGFzcyB7XG4gICRsb2NhdGlvbjtcbiAgaWQ7XG4gIF9uYW1lSXNDYXB0dXJpbmc7XG4gIF9uYW1lO1xuICBfY29udGVudE5hbWVJc0NhcHR1cmluZztcbiAgX2NvbnRlbnROYW1lO1xuICBjb25zdHJ1Y3RvcigkbG9jYXRpb24sIGlkLCBuYW1lLCBjb250ZW50TmFtZSkge1xuICAgIHRoaXMuJGxvY2F0aW9uID0gJGxvY2F0aW9uO1xuICAgIHRoaXMuaWQgPSBpZDtcbiAgICB0aGlzLl9uYW1lID0gbmFtZSB8fCBudWxsO1xuICAgIHRoaXMuX25hbWVJc0NhcHR1cmluZyA9IFJlZ2V4U291cmNlLmhhc0NhcHR1cmVzKHRoaXMuX25hbWUpO1xuICAgIHRoaXMuX2NvbnRlbnROYW1lID0gY29udGVudE5hbWUgfHwgbnVsbDtcbiAgICB0aGlzLl9jb250ZW50TmFtZUlzQ2FwdHVyaW5nID0gUmVnZXhTb3VyY2UuaGFzQ2FwdHVyZXModGhpcy5fY29udGVudE5hbWUpO1xuICB9XG4gIGdldCBkZWJ1Z05hbWUoKSB7XG4gICAgY29uc3QgbG9jYXRpb24gPSB0aGlzLiRsb2NhdGlvbiA/IGAke2Jhc2VuYW1lKHRoaXMuJGxvY2F0aW9uLmZpbGVuYW1lKX06JHt0aGlzLiRsb2NhdGlvbi5saW5lfWAgOiBcInVua25vd25cIjtcbiAgICByZXR1cm4gYCR7dGhpcy5jb25zdHJ1Y3Rvci5uYW1lfSMke3RoaXMuaWR9IEAgJHtsb2NhdGlvbn1gO1xuICB9XG4gIGdldE5hbWUobGluZVRleHQsIGNhcHR1cmVJbmRpY2VzKSB7XG4gICAgaWYgKCF0aGlzLl9uYW1lSXNDYXB0dXJpbmcgfHwgdGhpcy5fbmFtZSA9PT0gbnVsbCB8fCBsaW5lVGV4dCA9PT0gbnVsbCB8fCBjYXB0dXJlSW5kaWNlcyA9PT0gbnVsbCkge1xuICAgICAgcmV0dXJuIHRoaXMuX25hbWU7XG4gICAgfVxuICAgIHJldHVybiBSZWdleFNvdXJjZS5yZXBsYWNlQ2FwdHVyZXModGhpcy5fbmFtZSwgbGluZVRleHQsIGNhcHR1cmVJbmRpY2VzKTtcbiAgfVxuICBnZXRDb250ZW50TmFtZShsaW5lVGV4dCwgY2FwdHVyZUluZGljZXMpIHtcbiAgICBpZiAoIXRoaXMuX2NvbnRlbnROYW1lSXNDYXB0dXJpbmcgfHwgdGhpcy5fY29udGVudE5hbWUgPT09IG51bGwpIHtcbiAgICAgIHJldHVybiB0aGlzLl9jb250ZW50TmFtZTtcbiAgICB9XG4gICAgcmV0dXJuIFJlZ2V4U291cmNlLnJlcGxhY2VDYXB0dXJlcyh0aGlzLl9jb250ZW50TmFtZSwgbGluZVRleHQsIGNhcHR1cmVJbmRpY2VzKTtcbiAgfVxufTtcbnZhciBDYXB0dXJlUnVsZSA9IGNsYXNzIGV4dGVuZHMgUnVsZSB7XG4gIHJldG9rZW5pemVDYXB0dXJlZFdpdGhSdWxlSWQ7XG4gIGNvbnN0cnVjdG9yKCRsb2NhdGlvbiwgaWQsIG5hbWUsIGNvbnRlbnROYW1lLCByZXRva2VuaXplQ2FwdHVyZWRXaXRoUnVsZUlkKSB7XG4gICAgc3VwZXIoJGxvY2F0aW9uLCBpZCwgbmFtZSwgY29udGVudE5hbWUpO1xuICAgIHRoaXMucmV0b2tlbml6ZUNhcHR1cmVkV2l0aFJ1bGVJZCA9IHJldG9rZW5pemVDYXB0dXJlZFdpdGhSdWxlSWQ7XG4gIH1cbiAgZGlzcG9zZSgpIHtcbiAgfVxuICBjb2xsZWN0UGF0dGVybnMoZ3JhbW1hciwgb3V0KSB7XG4gICAgdGhyb3cgbmV3IEVycm9yKFwiTm90IHN1cHBvcnRlZCFcIik7XG4gIH1cbiAgY29tcGlsZShncmFtbWFyLCBlbmRSZWdleFNvdXJjZSkge1xuICAgIHRocm93IG5ldyBFcnJvcihcIk5vdCBzdXBwb3J0ZWQhXCIpO1xuICB9XG4gIGNvbXBpbGVBRyhncmFtbWFyLCBlbmRSZWdleFNvdXJjZSwgYWxsb3dBLCBhbGxvd0cpIHtcbiAgICB0aHJvdyBuZXcgRXJyb3IoXCJOb3Qgc3VwcG9ydGVkIVwiKTtcbiAgfVxufTtcbnZhciBNYXRjaFJ1bGUgPSBjbGFzcyBleHRlbmRzIFJ1bGUge1xuICBfbWF0Y2g7XG4gIGNhcHR1cmVzO1xuICBfY2FjaGVkQ29tcGlsZWRQYXR0ZXJucztcbiAgY29uc3RydWN0b3IoJGxvY2F0aW9uLCBpZCwgbmFtZSwgbWF0Y2gsIGNhcHR1cmVzKSB7XG4gICAgc3VwZXIoJGxvY2F0aW9uLCBpZCwgbmFtZSwgbnVsbCk7XG4gICAgdGhpcy5fbWF0Y2ggPSBuZXcgUmVnRXhwU291cmNlKG1hdGNoLCB0aGlzLmlkKTtcbiAgICB0aGlzLmNhcHR1cmVzID0gY2FwdHVyZXM7XG4gICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucyA9IG51bGw7XG4gIH1cbiAgZGlzcG9zZSgpIHtcbiAgICBpZiAodGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucykge1xuICAgICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucy5kaXNwb3NlKCk7XG4gICAgICB0aGlzLl9jYWNoZWRDb21waWxlZFBhdHRlcm5zID0gbnVsbDtcbiAgICB9XG4gIH1cbiAgZ2V0IGRlYnVnTWF0Y2hSZWdFeHAoKSB7XG4gICAgcmV0dXJuIGAke3RoaXMuX21hdGNoLnNvdXJjZX1gO1xuICB9XG4gIGNvbGxlY3RQYXR0ZXJucyhncmFtbWFyLCBvdXQpIHtcbiAgICBvdXQucHVzaCh0aGlzLl9tYXRjaCk7XG4gIH1cbiAgY29tcGlsZShncmFtbWFyLCBlbmRSZWdleFNvdXJjZSkge1xuICAgIHJldHVybiB0aGlzLl9nZXRDYWNoZWRDb21waWxlZFBhdHRlcm5zKGdyYW1tYXIpLmNvbXBpbGUoZ3JhbW1hcik7XG4gIH1cbiAgY29tcGlsZUFHKGdyYW1tYXIsIGVuZFJlZ2V4U291cmNlLCBhbGxvd0EsIGFsbG93Rykge1xuICAgIHJldHVybiB0aGlzLl9nZXRDYWNoZWRDb21waWxlZFBhdHRlcm5zKGdyYW1tYXIpLmNvbXBpbGVBRyhncmFtbWFyLCBhbGxvd0EsIGFsbG93Ryk7XG4gIH1cbiAgX2dldENhY2hlZENvbXBpbGVkUGF0dGVybnMoZ3JhbW1hcikge1xuICAgIGlmICghdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucykge1xuICAgICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucyA9IG5ldyBSZWdFeHBTb3VyY2VMaXN0KCk7XG4gICAgICB0aGlzLmNvbGxlY3RQYXR0ZXJucyhncmFtbWFyLCB0aGlzLl9jYWNoZWRDb21waWxlZFBhdHRlcm5zKTtcbiAgICB9XG4gICAgcmV0dXJuIHRoaXMuX2NhY2hlZENvbXBpbGVkUGF0dGVybnM7XG4gIH1cbn07XG52YXIgSW5jbHVkZU9ubHlSdWxlID0gY2xhc3MgZXh0ZW5kcyBSdWxlIHtcbiAgaGFzTWlzc2luZ1BhdHRlcm5zO1xuICBwYXR0ZXJucztcbiAgX2NhY2hlZENvbXBpbGVkUGF0dGVybnM7XG4gIGNvbnN0cnVjdG9yKCRsb2NhdGlvbiwgaWQsIG5hbWUsIGNvbnRlbnROYW1lLCBwYXR0ZXJucykge1xuICAgIHN1cGVyKCRsb2NhdGlvbiwgaWQsIG5hbWUsIGNvbnRlbnROYW1lKTtcbiAgICB0aGlzLnBhdHRlcm5zID0gcGF0dGVybnMucGF0dGVybnM7XG4gICAgdGhpcy5oYXNNaXNzaW5nUGF0dGVybnMgPSBwYXR0ZXJucy5oYXNNaXNzaW5nUGF0dGVybnM7XG4gICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucyA9IG51bGw7XG4gIH1cbiAgZGlzcG9zZSgpIHtcbiAgICBpZiAodGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucykge1xuICAgICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucy5kaXNwb3NlKCk7XG4gICAgICB0aGlzLl9jYWNoZWRDb21waWxlZFBhdHRlcm5zID0gbnVsbDtcbiAgICB9XG4gIH1cbiAgY29sbGVjdFBhdHRlcm5zKGdyYW1tYXIsIG91dCkge1xuICAgIGZvciAoY29uc3QgcGF0dGVybiBvZiB0aGlzLnBhdHRlcm5zKSB7XG4gICAgICBjb25zdCBydWxlID0gZ3JhbW1hci5nZXRSdWxlKHBhdHRlcm4pO1xuICAgICAgcnVsZS5jb2xsZWN0UGF0dGVybnMoZ3JhbW1hciwgb3V0KTtcbiAgICB9XG4gIH1cbiAgY29tcGlsZShncmFtbWFyLCBlbmRSZWdleFNvdXJjZSkge1xuICAgIHJldHVybiB0aGlzLl9nZXRDYWNoZWRDb21waWxlZFBhdHRlcm5zKGdyYW1tYXIpLmNvbXBpbGUoZ3JhbW1hcik7XG4gIH1cbiAgY29tcGlsZUFHKGdyYW1tYXIsIGVuZFJlZ2V4U291cmNlLCBhbGxvd0EsIGFsbG93Rykge1xuICAgIHJldHVybiB0aGlzLl9nZXRDYWNoZWRDb21waWxlZFBhdHRlcm5zKGdyYW1tYXIpLmNvbXBpbGVBRyhncmFtbWFyLCBhbGxvd0EsIGFsbG93Ryk7XG4gIH1cbiAgX2dldENhY2hlZENvbXBpbGVkUGF0dGVybnMoZ3JhbW1hcikge1xuICAgIGlmICghdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucykge1xuICAgICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucyA9IG5ldyBSZWdFeHBTb3VyY2VMaXN0KCk7XG4gICAgICB0aGlzLmNvbGxlY3RQYXR0ZXJucyhncmFtbWFyLCB0aGlzLl9jYWNoZWRDb21waWxlZFBhdHRlcm5zKTtcbiAgICB9XG4gICAgcmV0dXJuIHRoaXMuX2NhY2hlZENvbXBpbGVkUGF0dGVybnM7XG4gIH1cbn07XG52YXIgQmVnaW5FbmRSdWxlID0gY2xhc3MgZXh0ZW5kcyBSdWxlIHtcbiAgX2JlZ2luO1xuICBiZWdpbkNhcHR1cmVzO1xuICBfZW5kO1xuICBlbmRIYXNCYWNrUmVmZXJlbmNlcztcbiAgZW5kQ2FwdHVyZXM7XG4gIGFwcGx5RW5kUGF0dGVybkxhc3Q7XG4gIGhhc01pc3NpbmdQYXR0ZXJucztcbiAgcGF0dGVybnM7XG4gIF9jYWNoZWRDb21waWxlZFBhdHRlcm5zO1xuICBjb25zdHJ1Y3RvcigkbG9jYXRpb24sIGlkLCBuYW1lLCBjb250ZW50TmFtZSwgYmVnaW4sIGJlZ2luQ2FwdHVyZXMsIGVuZCwgZW5kQ2FwdHVyZXMsIGFwcGx5RW5kUGF0dGVybkxhc3QsIHBhdHRlcm5zKSB7XG4gICAgc3VwZXIoJGxvY2F0aW9uLCBpZCwgbmFtZSwgY29udGVudE5hbWUpO1xuICAgIHRoaXMuX2JlZ2luID0gbmV3IFJlZ0V4cFNvdXJjZShiZWdpbiwgdGhpcy5pZCk7XG4gICAgdGhpcy5iZWdpbkNhcHR1cmVzID0gYmVnaW5DYXB0dXJlcztcbiAgICB0aGlzLl9lbmQgPSBuZXcgUmVnRXhwU291cmNlKGVuZCA/IGVuZCA6IFwiXFx1RkZGRlwiLCAtMSk7XG4gICAgdGhpcy5lbmRIYXNCYWNrUmVmZXJlbmNlcyA9IHRoaXMuX2VuZC5oYXNCYWNrUmVmZXJlbmNlcztcbiAgICB0aGlzLmVuZENhcHR1cmVzID0gZW5kQ2FwdHVyZXM7XG4gICAgdGhpcy5hcHBseUVuZFBhdHRlcm5MYXN0ID0gYXBwbHlFbmRQYXR0ZXJuTGFzdCB8fCBmYWxzZTtcbiAgICB0aGlzLnBhdHRlcm5zID0gcGF0dGVybnMucGF0dGVybnM7XG4gICAgdGhpcy5oYXNNaXNzaW5nUGF0dGVybnMgPSBwYXR0ZXJucy5oYXNNaXNzaW5nUGF0dGVybnM7XG4gICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucyA9IG51bGw7XG4gIH1cbiAgZGlzcG9zZSgpIHtcbiAgICBpZiAodGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucykge1xuICAgICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucy5kaXNwb3NlKCk7XG4gICAgICB0aGlzLl9jYWNoZWRDb21waWxlZFBhdHRlcm5zID0gbnVsbDtcbiAgICB9XG4gIH1cbiAgZ2V0IGRlYnVnQmVnaW5SZWdFeHAoKSB7XG4gICAgcmV0dXJuIGAke3RoaXMuX2JlZ2luLnNvdXJjZX1gO1xuICB9XG4gIGdldCBkZWJ1Z0VuZFJlZ0V4cCgpIHtcbiAgICByZXR1cm4gYCR7dGhpcy5fZW5kLnNvdXJjZX1gO1xuICB9XG4gIGdldEVuZFdpdGhSZXNvbHZlZEJhY2tSZWZlcmVuY2VzKGxpbmVUZXh0LCBjYXB0dXJlSW5kaWNlcykge1xuICAgIHJldHVybiB0aGlzLl9lbmQucmVzb2x2ZUJhY2tSZWZlcmVuY2VzKGxpbmVUZXh0LCBjYXB0dXJlSW5kaWNlcyk7XG4gIH1cbiAgY29sbGVjdFBhdHRlcm5zKGdyYW1tYXIsIG91dCkge1xuICAgIG91dC5wdXNoKHRoaXMuX2JlZ2luKTtcbiAgfVxuICBjb21waWxlKGdyYW1tYXIsIGVuZFJlZ2V4U291cmNlKSB7XG4gICAgcmV0dXJuIHRoaXMuX2dldENhY2hlZENvbXBpbGVkUGF0dGVybnMoZ3JhbW1hciwgZW5kUmVnZXhTb3VyY2UpLmNvbXBpbGUoZ3JhbW1hcik7XG4gIH1cbiAgY29tcGlsZUFHKGdyYW1tYXIsIGVuZFJlZ2V4U291cmNlLCBhbGxvd0EsIGFsbG93Rykge1xuICAgIHJldHVybiB0aGlzLl9nZXRDYWNoZWRDb21waWxlZFBhdHRlcm5zKGdyYW1tYXIsIGVuZFJlZ2V4U291cmNlKS5jb21waWxlQUcoZ3JhbW1hciwgYWxsb3dBLCBhbGxvd0cpO1xuICB9XG4gIF9nZXRDYWNoZWRDb21waWxlZFBhdHRlcm5zKGdyYW1tYXIsIGVuZFJlZ2V4U291cmNlKSB7XG4gICAgaWYgKCF0aGlzLl9jYWNoZWRDb21waWxlZFBhdHRlcm5zKSB7XG4gICAgICB0aGlzLl9jYWNoZWRDb21waWxlZFBhdHRlcm5zID0gbmV3IFJlZ0V4cFNvdXJjZUxpc3QoKTtcbiAgICAgIGZvciAoY29uc3QgcGF0dGVybiBvZiB0aGlzLnBhdHRlcm5zKSB7XG4gICAgICAgIGNvbnN0IHJ1bGUgPSBncmFtbWFyLmdldFJ1bGUocGF0dGVybik7XG4gICAgICAgIHJ1bGUuY29sbGVjdFBhdHRlcm5zKGdyYW1tYXIsIHRoaXMuX2NhY2hlZENvbXBpbGVkUGF0dGVybnMpO1xuICAgICAgfVxuICAgICAgaWYgKHRoaXMuYXBwbHlFbmRQYXR0ZXJuTGFzdCkge1xuICAgICAgICB0aGlzLl9jYWNoZWRDb21waWxlZFBhdHRlcm5zLnB1c2godGhpcy5fZW5kLmhhc0JhY2tSZWZlcmVuY2VzID8gdGhpcy5fZW5kLmNsb25lKCkgOiB0aGlzLl9lbmQpO1xuICAgICAgfSBlbHNlIHtcbiAgICAgICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucy51bnNoaWZ0KHRoaXMuX2VuZC5oYXNCYWNrUmVmZXJlbmNlcyA/IHRoaXMuX2VuZC5jbG9uZSgpIDogdGhpcy5fZW5kKTtcbiAgICAgIH1cbiAgICB9XG4gICAgaWYgKHRoaXMuX2VuZC5oYXNCYWNrUmVmZXJlbmNlcykge1xuICAgICAgaWYgKHRoaXMuYXBwbHlFbmRQYXR0ZXJuTGFzdCkge1xuICAgICAgICB0aGlzLl9jYWNoZWRDb21waWxlZFBhdHRlcm5zLnNldFNvdXJjZSh0aGlzLl9jYWNoZWRDb21waWxlZFBhdHRlcm5zLmxlbmd0aCgpIC0gMSwgZW5kUmVnZXhTb3VyY2UpO1xuICAgICAgfSBlbHNlIHtcbiAgICAgICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucy5zZXRTb3VyY2UoMCwgZW5kUmVnZXhTb3VyY2UpO1xuICAgICAgfVxuICAgIH1cbiAgICByZXR1cm4gdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucztcbiAgfVxufTtcbnZhciBCZWdpbldoaWxlUnVsZSA9IGNsYXNzIGV4dGVuZHMgUnVsZSB7XG4gIF9iZWdpbjtcbiAgYmVnaW5DYXB0dXJlcztcbiAgd2hpbGVDYXB0dXJlcztcbiAgX3doaWxlO1xuICB3aGlsZUhhc0JhY2tSZWZlcmVuY2VzO1xuICBoYXNNaXNzaW5nUGF0dGVybnM7XG4gIHBhdHRlcm5zO1xuICBfY2FjaGVkQ29tcGlsZWRQYXR0ZXJucztcbiAgX2NhY2hlZENvbXBpbGVkV2hpbGVQYXR0ZXJucztcbiAgY29uc3RydWN0b3IoJGxvY2F0aW9uLCBpZCwgbmFtZSwgY29udGVudE5hbWUsIGJlZ2luLCBiZWdpbkNhcHR1cmVzLCBfd2hpbGUsIHdoaWxlQ2FwdHVyZXMsIHBhdHRlcm5zKSB7XG4gICAgc3VwZXIoJGxvY2F0aW9uLCBpZCwgbmFtZSwgY29udGVudE5hbWUpO1xuICAgIHRoaXMuX2JlZ2luID0gbmV3IFJlZ0V4cFNvdXJjZShiZWdpbiwgdGhpcy5pZCk7XG4gICAgdGhpcy5iZWdpbkNhcHR1cmVzID0gYmVnaW5DYXB0dXJlcztcbiAgICB0aGlzLndoaWxlQ2FwdHVyZXMgPSB3aGlsZUNhcHR1cmVzO1xuICAgIHRoaXMuX3doaWxlID0gbmV3IFJlZ0V4cFNvdXJjZShfd2hpbGUsIHdoaWxlUnVsZUlkKTtcbiAgICB0aGlzLndoaWxlSGFzQmFja1JlZmVyZW5jZXMgPSB0aGlzLl93aGlsZS5oYXNCYWNrUmVmZXJlbmNlcztcbiAgICB0aGlzLnBhdHRlcm5zID0gcGF0dGVybnMucGF0dGVybnM7XG4gICAgdGhpcy5oYXNNaXNzaW5nUGF0dGVybnMgPSBwYXR0ZXJucy5oYXNNaXNzaW5nUGF0dGVybnM7XG4gICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucyA9IG51bGw7XG4gICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRXaGlsZVBhdHRlcm5zID0gbnVsbDtcbiAgfVxuICBkaXNwb3NlKCkge1xuICAgIGlmICh0aGlzLl9jYWNoZWRDb21waWxlZFBhdHRlcm5zKSB7XG4gICAgICB0aGlzLl9jYWNoZWRDb21waWxlZFBhdHRlcm5zLmRpc3Bvc2UoKTtcbiAgICAgIHRoaXMuX2NhY2hlZENvbXBpbGVkUGF0dGVybnMgPSBudWxsO1xuICAgIH1cbiAgICBpZiAodGhpcy5fY2FjaGVkQ29tcGlsZWRXaGlsZVBhdHRlcm5zKSB7XG4gICAgICB0aGlzLl9jYWNoZWRDb21waWxlZFdoaWxlUGF0dGVybnMuZGlzcG9zZSgpO1xuICAgICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRXaGlsZVBhdHRlcm5zID0gbnVsbDtcbiAgICB9XG4gIH1cbiAgZ2V0IGRlYnVnQmVnaW5SZWdFeHAoKSB7XG4gICAgcmV0dXJuIGAke3RoaXMuX2JlZ2luLnNvdXJjZX1gO1xuICB9XG4gIGdldCBkZWJ1Z1doaWxlUmVnRXhwKCkge1xuICAgIHJldHVybiBgJHt0aGlzLl93aGlsZS5zb3VyY2V9YDtcbiAgfVxuICBnZXRXaGlsZVdpdGhSZXNvbHZlZEJhY2tSZWZlcmVuY2VzKGxpbmVUZXh0LCBjYXB0dXJlSW5kaWNlcykge1xuICAgIHJldHVybiB0aGlzLl93aGlsZS5yZXNvbHZlQmFja1JlZmVyZW5jZXMobGluZVRleHQsIGNhcHR1cmVJbmRpY2VzKTtcbiAgfVxuICBjb2xsZWN0UGF0dGVybnMoZ3JhbW1hciwgb3V0KSB7XG4gICAgb3V0LnB1c2godGhpcy5fYmVnaW4pO1xuICB9XG4gIGNvbXBpbGUoZ3JhbW1hciwgZW5kUmVnZXhTb3VyY2UpIHtcbiAgICByZXR1cm4gdGhpcy5fZ2V0Q2FjaGVkQ29tcGlsZWRQYXR0ZXJucyhncmFtbWFyKS5jb21waWxlKGdyYW1tYXIpO1xuICB9XG4gIGNvbXBpbGVBRyhncmFtbWFyLCBlbmRSZWdleFNvdXJjZSwgYWxsb3dBLCBhbGxvd0cpIHtcbiAgICByZXR1cm4gdGhpcy5fZ2V0Q2FjaGVkQ29tcGlsZWRQYXR0ZXJucyhncmFtbWFyKS5jb21waWxlQUcoZ3JhbW1hciwgYWxsb3dBLCBhbGxvd0cpO1xuICB9XG4gIF9nZXRDYWNoZWRDb21waWxlZFBhdHRlcm5zKGdyYW1tYXIpIHtcbiAgICBpZiAoIXRoaXMuX2NhY2hlZENvbXBpbGVkUGF0dGVybnMpIHtcbiAgICAgIHRoaXMuX2NhY2hlZENvbXBpbGVkUGF0dGVybnMgPSBuZXcgUmVnRXhwU291cmNlTGlzdCgpO1xuICAgICAgZm9yIChjb25zdCBwYXR0ZXJuIG9mIHRoaXMucGF0dGVybnMpIHtcbiAgICAgICAgY29uc3QgcnVsZSA9IGdyYW1tYXIuZ2V0UnVsZShwYXR0ZXJuKTtcbiAgICAgICAgcnVsZS5jb2xsZWN0UGF0dGVybnMoZ3JhbW1hciwgdGhpcy5fY2FjaGVkQ29tcGlsZWRQYXR0ZXJucyk7XG4gICAgICB9XG4gICAgfVxuICAgIHJldHVybiB0aGlzLl9jYWNoZWRDb21waWxlZFBhdHRlcm5zO1xuICB9XG4gIGNvbXBpbGVXaGlsZShncmFtbWFyLCBlbmRSZWdleFNvdXJjZSkge1xuICAgIHJldHVybiB0aGlzLl9nZXRDYWNoZWRDb21waWxlZFdoaWxlUGF0dGVybnMoZ3JhbW1hciwgZW5kUmVnZXhTb3VyY2UpLmNvbXBpbGUoZ3JhbW1hcik7XG4gIH1cbiAgY29tcGlsZVdoaWxlQUcoZ3JhbW1hciwgZW5kUmVnZXhTb3VyY2UsIGFsbG93QSwgYWxsb3dHKSB7XG4gICAgcmV0dXJuIHRoaXMuX2dldENhY2hlZENvbXBpbGVkV2hpbGVQYXR0ZXJucyhncmFtbWFyLCBlbmRSZWdleFNvdXJjZSkuY29tcGlsZUFHKGdyYW1tYXIsIGFsbG93QSwgYWxsb3dHKTtcbiAgfVxuICBfZ2V0Q2FjaGVkQ29tcGlsZWRXaGlsZVBhdHRlcm5zKGdyYW1tYXIsIGVuZFJlZ2V4U291cmNlKSB7XG4gICAgaWYgKCF0aGlzLl9jYWNoZWRDb21waWxlZFdoaWxlUGF0dGVybnMpIHtcbiAgICAgIHRoaXMuX2NhY2hlZENvbXBpbGVkV2hpbGVQYXR0ZXJucyA9IG5ldyBSZWdFeHBTb3VyY2VMaXN0KCk7XG4gICAgICB0aGlzLl9jYWNoZWRDb21waWxlZFdoaWxlUGF0dGVybnMucHVzaCh0aGlzLl93aGlsZS5oYXNCYWNrUmVmZXJlbmNlcyA/IHRoaXMuX3doaWxlLmNsb25lKCkgOiB0aGlzLl93aGlsZSk7XG4gICAgfVxuICAgIGlmICh0aGlzLl93aGlsZS5oYXNCYWNrUmVmZXJlbmNlcykge1xuICAgICAgdGhpcy5fY2FjaGVkQ29tcGlsZWRXaGlsZVBhdHRlcm5zLnNldFNvdXJjZSgwLCBlbmRSZWdleFNvdXJjZSA/IGVuZFJlZ2V4U291cmNlIDogXCJcXHVGRkZGXCIpO1xuICAgIH1cbiAgICByZXR1cm4gdGhpcy5fY2FjaGVkQ29tcGlsZWRXaGlsZVBhdHRlcm5zO1xuICB9XG59O1xudmFyIFJ1bGVGYWN0b3J5ID0gY2xhc3MgX1J1bGVGYWN0b3J5IHtcbiAgc3RhdGljIGNyZWF0ZUNhcHR1cmVSdWxlKGhlbHBlciwgJGxvY2F0aW9uLCBuYW1lLCBjb250ZW50TmFtZSwgcmV0b2tlbml6ZUNhcHR1cmVkV2l0aFJ1bGVJZCkge1xuICAgIHJldHVybiBoZWxwZXIucmVnaXN0ZXJSdWxlKChpZCkgPT4ge1xuICAgICAgcmV0dXJuIG5ldyBDYXB0dXJlUnVsZSgkbG9jYXRpb24sIGlkLCBuYW1lLCBjb250ZW50TmFtZSwgcmV0b2tlbml6ZUNhcHR1cmVkV2l0aFJ1bGVJZCk7XG4gICAgfSk7XG4gIH1cbiAgc3RhdGljIGdldENvbXBpbGVkUnVsZUlkKGRlc2MsIGhlbHBlciwgcmVwb3NpdG9yeSkge1xuICAgIGlmICghZGVzYy5pZCkge1xuICAgICAgaGVscGVyLnJlZ2lzdGVyUnVsZSgoaWQpID0+IHtcbiAgICAgICAgZGVzYy5pZCA9IGlkO1xuICAgICAgICBpZiAoZGVzYy5tYXRjaCkge1xuICAgICAgICAgIHJldHVybiBuZXcgTWF0Y2hSdWxlKFxuICAgICAgICAgICAgZGVzYy4kdnNjb2RlVGV4dG1hdGVMb2NhdGlvbixcbiAgICAgICAgICAgIGRlc2MuaWQsXG4gICAgICAgICAgICBkZXNjLm5hbWUsXG4gICAgICAgICAgICBkZXNjLm1hdGNoLFxuICAgICAgICAgICAgX1J1bGVGYWN0b3J5Ll9jb21waWxlQ2FwdHVyZXMoZGVzYy5jYXB0dXJlcywgaGVscGVyLCByZXBvc2l0b3J5KVxuICAgICAgICAgICk7XG4gICAgICAgIH1cbiAgICAgICAgaWYgKHR5cGVvZiBkZXNjLmJlZ2luID09PSBcInVuZGVmaW5lZFwiKSB7XG4gICAgICAgICAgaWYgKGRlc2MucmVwb3NpdG9yeSkge1xuICAgICAgICAgICAgcmVwb3NpdG9yeSA9IG1lcmdlT2JqZWN0cyh7fSwgcmVwb3NpdG9yeSwgZGVzYy5yZXBvc2l0b3J5KTtcbiAgICAgICAgICB9XG4gICAgICAgICAgbGV0IHBhdHRlcm5zID0gZGVzYy5wYXR0ZXJucztcbiAgICAgICAgICBpZiAodHlwZW9mIHBhdHRlcm5zID09PSBcInVuZGVmaW5lZFwiICYmIGRlc2MuaW5jbHVkZSkge1xuICAgICAgICAgICAgcGF0dGVybnMgPSBbeyBpbmNsdWRlOiBkZXNjLmluY2x1ZGUgfV07XG4gICAgICAgICAgfVxuICAgICAgICAgIHJldHVybiBuZXcgSW5jbHVkZU9ubHlSdWxlKFxuICAgICAgICAgICAgZGVzYy4kdnNjb2RlVGV4dG1hdGVMb2NhdGlvbixcbiAgICAgICAgICAgIGRlc2MuaWQsXG4gICAgICAgICAgICBkZXNjLm5hbWUsXG4gICAgICAgICAgICBkZXNjLmNvbnRlbnROYW1lLFxuICAgICAgICAgICAgX1J1bGVGYWN0b3J5Ll9jb21waWxlUGF0dGVybnMocGF0dGVybnMsIGhlbHBlciwgcmVwb3NpdG9yeSlcbiAgICAgICAgICApO1xuICAgICAgICB9XG4gICAgICAgIGlmIChkZXNjLndoaWxlKSB7XG4gICAgICAgICAgcmV0dXJuIG5ldyBCZWdpbldoaWxlUnVsZShcbiAgICAgICAgICAgIGRlc2MuJHZzY29kZVRleHRtYXRlTG9jYXRpb24sXG4gICAgICAgICAgICBkZXNjLmlkLFxuICAgICAgICAgICAgZGVzYy5uYW1lLFxuICAgICAgICAgICAgZGVzYy5jb250ZW50TmFtZSxcbiAgICAgICAgICAgIGRlc2MuYmVnaW4sXG4gICAgICAgICAgICBfUnVsZUZhY3RvcnkuX2NvbXBpbGVDYXB0dXJlcyhkZXNjLmJlZ2luQ2FwdHVyZXMgfHwgZGVzYy5jYXB0dXJlcywgaGVscGVyLCByZXBvc2l0b3J5KSxcbiAgICAgICAgICAgIGRlc2Mud2hpbGUsXG4gICAgICAgICAgICBfUnVsZUZhY3RvcnkuX2NvbXBpbGVDYXB0dXJlcyhkZXNjLndoaWxlQ2FwdHVyZXMgfHwgZGVzYy5jYXB0dXJlcywgaGVscGVyLCByZXBvc2l0b3J5KSxcbiAgICAgICAgICAgIF9SdWxlRmFjdG9yeS5fY29tcGlsZVBhdHRlcm5zKGRlc2MucGF0dGVybnMsIGhlbHBlciwgcmVwb3NpdG9yeSlcbiAgICAgICAgICApO1xuICAgICAgICB9XG4gICAgICAgIHJldHVybiBuZXcgQmVnaW5FbmRSdWxlKFxuICAgICAgICAgIGRlc2MuJHZzY29kZVRleHRtYXRlTG9jYXRpb24sXG4gICAgICAgICAgZGVzYy5pZCxcbiAgICAgICAgICBkZXNjLm5hbWUsXG4gICAgICAgICAgZGVzYy5jb250ZW50TmFtZSxcbiAgICAgICAgICBkZXNjLmJlZ2luLFxuICAgICAgICAgIF9SdWxlRmFjdG9yeS5fY29tcGlsZUNhcHR1cmVzKGRlc2MuYmVnaW5DYXB0dXJlcyB8fCBkZXNjLmNhcHR1cmVzLCBoZWxwZXIsIHJlcG9zaXRvcnkpLFxuICAgICAgICAgIGRlc2MuZW5kLFxuICAgICAgICAgIF9SdWxlRmFjdG9yeS5fY29tcGlsZUNhcHR1cmVzKGRlc2MuZW5kQ2FwdHVyZXMgfHwgZGVzYy5jYXB0dXJlcywgaGVscGVyLCByZXBvc2l0b3J5KSxcbiAgICAgICAgICBkZXNjLmFwcGx5RW5kUGF0dGVybkxhc3QsXG4gICAgICAgICAgX1J1bGVGYWN0b3J5Ll9jb21waWxlUGF0dGVybnMoZGVzYy5wYXR0ZXJucywgaGVscGVyLCByZXBvc2l0b3J5KVxuICAgICAgICApO1xuICAgICAgfSk7XG4gICAgfVxuICAgIHJldHVybiBkZXNjLmlkO1xuICB9XG4gIHN0YXRpYyBfY29tcGlsZUNhcHR1cmVzKGNhcHR1cmVzLCBoZWxwZXIsIHJlcG9zaXRvcnkpIHtcbiAgICBsZXQgciA9IFtdO1xuICAgIGlmIChjYXB0dXJlcykge1xuICAgICAgbGV0IG1heGltdW1DYXB0dXJlSWQgPSAwO1xuICAgICAgZm9yIChjb25zdCBjYXB0dXJlSWQgaW4gY2FwdHVyZXMpIHtcbiAgICAgICAgaWYgKGNhcHR1cmVJZCA9PT0gXCIkdnNjb2RlVGV4dG1hdGVMb2NhdGlvblwiKSB7XG4gICAgICAgICAgY29udGludWU7XG4gICAgICAgIH1cbiAgICAgICAgY29uc3QgbnVtZXJpY0NhcHR1cmVJZCA9IHBhcnNlSW50KGNhcHR1cmVJZCwgMTApO1xuICAgICAgICBpZiAobnVtZXJpY0NhcHR1cmVJZCA+IG1heGltdW1DYXB0dXJlSWQpIHtcbiAgICAgICAgICBtYXhpbXVtQ2FwdHVyZUlkID0gbnVtZXJpY0NhcHR1cmVJZDtcbiAgICAgICAgfVxuICAgICAgfVxuICAgICAgZm9yIChsZXQgaSA9IDA7IGkgPD0gbWF4aW11bUNhcHR1cmVJZDsgaSsrKSB7XG4gICAgICAgIHJbaV0gPSBudWxsO1xuICAgICAgfVxuICAgICAgZm9yIChjb25zdCBjYXB0dXJlSWQgaW4gY2FwdHVyZXMpIHtcbiAgICAgICAgaWYgKGNhcHR1cmVJZCA9PT0gXCIkdnNjb2RlVGV4dG1hdGVMb2NhdGlvblwiKSB7XG4gICAgICAgICAgY29udGludWU7XG4gICAgICAgIH1cbiAgICAgICAgY29uc3QgbnVtZXJpY0NhcHR1cmVJZCA9IHBhcnNlSW50KGNhcHR1cmVJZCwgMTApO1xuICAgICAgICBsZXQgcmV0b2tlbml6ZUNhcHR1cmVkV2l0aFJ1bGVJZCA9IDA7XG4gICAgICAgIGlmIChjYXB0dXJlc1tjYXB0dXJlSWRdLnBhdHRlcm5zKSB7XG4gICAgICAgICAgcmV0b2tlbml6ZUNhcHR1cmVkV2l0aFJ1bGVJZCA9IF9SdWxlRmFjdG9yeS5nZXRDb21waWxlZFJ1bGVJZChjYXB0dXJlc1tjYXB0dXJlSWRdLCBoZWxwZXIsIHJlcG9zaXRvcnkpO1xuICAgICAgICB9XG4gICAgICAgIHJbbnVtZXJpY0NhcHR1cmVJZF0gPSBfUnVsZUZhY3RvcnkuY3JlYXRlQ2FwdHVyZVJ1bGUoaGVscGVyLCBjYXB0dXJlc1tjYXB0dXJlSWRdLiR2c2NvZGVUZXh0bWF0ZUxvY2F0aW9uLCBjYXB0dXJlc1tjYXB0dXJlSWRdLm5hbWUsIGNhcHR1cmVzW2NhcHR1cmVJZF0uY29udGVudE5hbWUsIHJldG9rZW5pemVDYXB0dXJlZFdpdGhSdWxlSWQpO1xuICAgICAgfVxuICAgIH1cbiAgICByZXR1cm4gcjtcbiAgfVxuICBzdGF0aWMgX2NvbXBpbGVQYXR0ZXJucyhwYXR0ZXJucywgaGVscGVyLCByZXBvc2l0b3J5KSB7XG4gICAgbGV0IHIgPSBbXTtcbiAgICBpZiAocGF0dGVybnMpIHtcbiAgICAgIGZvciAobGV0IGkgPSAwLCBsZW4gPSBwYXR0ZXJucy5sZW5ndGg7IGkgPCBsZW47IGkrKykge1xuICAgICAgICBjb25zdCBwYXR0ZXJuID0gcGF0dGVybnNbaV07XG4gICAgICAgIGxldCBydWxlSWQgPSAtMTtcbiAgICAgICAgaWYgKHBhdHRlcm4uaW5jbHVkZSkge1xuICAgICAgICAgIGNvbnN0IHJlZmVyZW5jZSA9IHBhcnNlSW5jbHVkZShwYXR0ZXJuLmluY2x1ZGUpO1xuICAgICAgICAgIHN3aXRjaCAocmVmZXJlbmNlLmtpbmQpIHtcbiAgICAgICAgICAgIGNhc2UgMCAvKiBCYXNlICovOlxuICAgICAgICAgICAgY2FzZSAxIC8qIFNlbGYgKi86XG4gICAgICAgICAgICAgIHJ1bGVJZCA9IF9SdWxlRmFjdG9yeS5nZXRDb21waWxlZFJ1bGVJZChyZXBvc2l0b3J5W3BhdHRlcm4uaW5jbHVkZV0sIGhlbHBlciwgcmVwb3NpdG9yeSk7XG4gICAgICAgICAgICAgIGJyZWFrO1xuICAgICAgICAgICAgY2FzZSAyIC8qIFJlbGF0aXZlUmVmZXJlbmNlICovOlxuICAgICAgICAgICAgICBsZXQgbG9jYWxJbmNsdWRlZFJ1bGUgPSByZXBvc2l0b3J5W3JlZmVyZW5jZS5ydWxlTmFtZV07XG4gICAgICAgICAgICAgIGlmIChsb2NhbEluY2x1ZGVkUnVsZSkge1xuICAgICAgICAgICAgICAgIHJ1bGVJZCA9IF9SdWxlRmFjdG9yeS5nZXRDb21waWxlZFJ1bGVJZChsb2NhbEluY2x1ZGVkUnVsZSwgaGVscGVyLCByZXBvc2l0b3J5KTtcbiAgICAgICAgICAgICAgfSBlbHNlIHtcbiAgICAgICAgICAgICAgfVxuICAgICAgICAgICAgICBicmVhaztcbiAgICAgICAgICAgIGNhc2UgMyAvKiBUb3BMZXZlbFJlZmVyZW5jZSAqLzpcbiAgICAgICAgICAgIGNhc2UgNCAvKiBUb3BMZXZlbFJlcG9zaXRvcnlSZWZlcmVuY2UgKi86XG4gICAgICAgICAgICAgIGNvbnN0IGV4dGVybmFsR3JhbW1hck5hbWUgPSByZWZlcmVuY2Uuc2NvcGVOYW1lO1xuICAgICAgICAgICAgICBjb25zdCBleHRlcm5hbEdyYW1tYXJJbmNsdWRlID0gcmVmZXJlbmNlLmtpbmQgPT09IDQgLyogVG9wTGV2ZWxSZXBvc2l0b3J5UmVmZXJlbmNlICovID8gcmVmZXJlbmNlLnJ1bGVOYW1lIDogbnVsbDtcbiAgICAgICAgICAgICAgY29uc3QgZXh0ZXJuYWxHcmFtbWFyID0gaGVscGVyLmdldEV4dGVybmFsR3JhbW1hcihleHRlcm5hbEdyYW1tYXJOYW1lLCByZXBvc2l0b3J5KTtcbiAgICAgICAgICAgICAgaWYgKGV4dGVybmFsR3JhbW1hcikge1xuICAgICAgICAgICAgICAgIGlmIChleHRlcm5hbEdyYW1tYXJJbmNsdWRlKSB7XG4gICAgICAgICAgICAgICAgICBsZXQgZXh0ZXJuYWxJbmNsdWRlZFJ1bGUgPSBleHRlcm5hbEdyYW1tYXIucmVwb3NpdG9yeVtleHRlcm5hbEdyYW1tYXJJbmNsdWRlXTtcbiAgICAgICAgICAgICAgICAgIGlmIChleHRlcm5hbEluY2x1ZGVkUnVsZSkge1xuICAgICAgICAgICAgICAgICAgICBydWxlSWQgPSBfUnVsZUZhY3RvcnkuZ2V0Q29tcGlsZWRSdWxlSWQoZXh0ZXJuYWxJbmNsdWRlZFJ1bGUsIGhlbHBlciwgZXh0ZXJuYWxHcmFtbWFyLnJlcG9zaXRvcnkpO1xuICAgICAgICAgICAgICAgICAgfSBlbHNlIHtcbiAgICAgICAgICAgICAgICAgIH1cbiAgICAgICAgICAgICAgICB9IGVsc2Uge1xuICAgICAgICAgICAgICAgICAgcnVsZUlkID0gX1J1bGVGYWN0b3J5LmdldENvbXBpbGVkUnVsZUlkKGV4dGVybmFsR3JhbW1hci5yZXBvc2l0b3J5LiRzZWxmLCBoZWxwZXIsIGV4dGVybmFsR3JhbW1hci5yZXBvc2l0b3J5KTtcbiAgICAgICAgICAgICAgICB9XG4gICAgICAgICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgICAgIH1cbiAgICAgICAgICAgICAgYnJlYWs7XG4gICAgICAgICAgfVxuICAgICAgICB9IGVsc2Uge1xuICAgICAgICAgIHJ1bGVJZCA9IF9SdWxlRmFjdG9yeS5nZXRDb21waWxlZFJ1bGVJZChwYXR0ZXJuLCBoZWxwZXIsIHJlcG9zaXRvcnkpO1xuICAgICAgICB9XG4gICAgICAgIGlmIChydWxlSWQgIT09IC0xKSB7XG4gICAgICAgICAgY29uc3QgcnVsZSA9IGhlbHBlci5nZXRSdWxlKHJ1bGVJZCk7XG4gICAgICAgICAgbGV0IHNraXBSdWxlID0gZmFsc2U7XG4gICAgICAgICAgaWYgKHJ1bGUgaW5zdGFuY2VvZiBJbmNsdWRlT25seVJ1bGUgfHwgcnVsZSBpbnN0YW5jZW9mIEJlZ2luRW5kUnVsZSB8fCBydWxlIGluc3RhbmNlb2YgQmVnaW5XaGlsZVJ1bGUpIHtcbiAgICAgICAgICAgIGlmIChydWxlLmhhc01pc3NpbmdQYXR0ZXJucyAmJiBydWxlLnBhdHRlcm5zLmxlbmd0aCA9PT0gMCkge1xuICAgICAgICAgICAgICBza2lwUnVsZSA9IHRydWU7XG4gICAgICAgICAgICB9XG4gICAgICAgICAgfVxuICAgICAgICAgIGlmIChza2lwUnVsZSkge1xuICAgICAgICAgICAgY29udGludWU7XG4gICAgICAgICAgfVxuICAgICAgICAgIHIucHVzaChydWxlSWQpO1xuICAgICAgICB9XG4gICAgICB9XG4gICAgfVxuICAgIHJldHVybiB7XG4gICAgICBwYXR0ZXJuczogcixcbiAgICAgIGhhc01pc3NpbmdQYXR0ZXJuczogKHBhdHRlcm5zID8gcGF0dGVybnMubGVuZ3RoIDogMCkgIT09IHIubGVuZ3RoXG4gICAgfTtcbiAgfVxufTtcbnZhciBSZWdFeHBTb3VyY2UgPSBjbGFzcyBfUmVnRXhwU291cmNlIHtcbiAgc291cmNlO1xuICBydWxlSWQ7XG4gIGhhc0FuY2hvcjtcbiAgaGFzQmFja1JlZmVyZW5jZXM7XG4gIF9hbmNob3JDYWNoZTtcbiAgY29uc3RydWN0b3IocmVnRXhwU291cmNlLCBydWxlSWQpIHtcbiAgICBpZiAocmVnRXhwU291cmNlICYmIHR5cGVvZiByZWdFeHBTb3VyY2UgPT09IFwic3RyaW5nXCIpIHtcbiAgICAgIGNvbnN0IGxlbiA9IHJlZ0V4cFNvdXJjZS5sZW5ndGg7XG4gICAgICBsZXQgbGFzdFB1c2hlZFBvcyA9IDA7XG4gICAgICBsZXQgb3V0cHV0ID0gW107XG4gICAgICBsZXQgaGFzQW5jaG9yID0gZmFsc2U7XG4gICAgICBmb3IgKGxldCBwb3MgPSAwOyBwb3MgPCBsZW47IHBvcysrKSB7XG4gICAgICAgIGNvbnN0IGNoID0gcmVnRXhwU291cmNlLmNoYXJBdChwb3MpO1xuICAgICAgICBpZiAoY2ggPT09IFwiXFxcXFwiKSB7XG4gICAgICAgICAgaWYgKHBvcyArIDEgPCBsZW4pIHtcbiAgICAgICAgICAgIGNvbnN0IG5leHRDaCA9IHJlZ0V4cFNvdXJjZS5jaGFyQXQocG9zICsgMSk7XG4gICAgICAgICAgICBpZiAobmV4dENoID09PSBcInpcIikge1xuICAgICAgICAgICAgICBvdXRwdXQucHVzaChyZWdFeHBTb3VyY2Uuc3Vic3RyaW5nKGxhc3RQdXNoZWRQb3MsIHBvcykpO1xuICAgICAgICAgICAgICBvdXRwdXQucHVzaChcIiQoPyFcXFxcbikoPzwhXFxcXG4pXCIpO1xuICAgICAgICAgICAgICBsYXN0UHVzaGVkUG9zID0gcG9zICsgMjtcbiAgICAgICAgICAgIH0gZWxzZSBpZiAobmV4dENoID09PSBcIkFcIiB8fCBuZXh0Q2ggPT09IFwiR1wiKSB7XG4gICAgICAgICAgICAgIGhhc0FuY2hvciA9IHRydWU7XG4gICAgICAgICAgICB9XG4gICAgICAgICAgICBwb3MrKztcbiAgICAgICAgICB9XG4gICAgICAgIH1cbiAgICAgIH1cbiAgICAgIHRoaXMuaGFzQW5jaG9yID0gaGFzQW5jaG9yO1xuICAgICAgaWYgKGxhc3RQdXNoZWRQb3MgPT09IDApIHtcbiAgICAgICAgdGhpcy5zb3VyY2UgPSByZWdFeHBTb3VyY2U7XG4gICAgICB9IGVsc2Uge1xuICAgICAgICBvdXRwdXQucHVzaChyZWdFeHBTb3VyY2Uuc3Vic3RyaW5nKGxhc3RQdXNoZWRQb3MsIGxlbikpO1xuICAgICAgICB0aGlzLnNvdXJjZSA9IG91dHB1dC5qb2luKFwiXCIpO1xuICAgICAgfVxuICAgIH0gZWxzZSB7XG4gICAgICB0aGlzLmhhc0FuY2hvciA9IGZhbHNlO1xuICAgICAgdGhpcy5zb3VyY2UgPSByZWdFeHBTb3VyY2U7XG4gICAgfVxuICAgIGlmICh0aGlzLmhhc0FuY2hvcikge1xuICAgICAgdGhpcy5fYW5jaG9yQ2FjaGUgPSB0aGlzLl9idWlsZEFuY2hvckNhY2hlKCk7XG4gICAgfSBlbHNlIHtcbiAgICAgIHRoaXMuX2FuY2hvckNhY2hlID0gbnVsbDtcbiAgICB9XG4gICAgdGhpcy5ydWxlSWQgPSBydWxlSWQ7XG4gICAgaWYgKHR5cGVvZiB0aGlzLnNvdXJjZSA9PT0gXCJzdHJpbmdcIikge1xuICAgICAgdGhpcy5oYXNCYWNrUmVmZXJlbmNlcyA9IEhBU19CQUNLX1JFRkVSRU5DRVMudGVzdCh0aGlzLnNvdXJjZSk7XG4gICAgfSBlbHNlIHtcbiAgICAgIHRoaXMuaGFzQmFja1JlZmVyZW5jZXMgPSBmYWxzZTtcbiAgICB9XG4gIH1cbiAgY2xvbmUoKSB7XG4gICAgcmV0dXJuIG5ldyBfUmVnRXhwU291cmNlKHRoaXMuc291cmNlLCB0aGlzLnJ1bGVJZCk7XG4gIH1cbiAgc2V0U291cmNlKG5ld1NvdXJjZSkge1xuICAgIGlmICh0aGlzLnNvdXJjZSA9PT0gbmV3U291cmNlKSB7XG4gICAgICByZXR1cm47XG4gICAgfVxuICAgIHRoaXMuc291cmNlID0gbmV3U291cmNlO1xuICAgIGlmICh0aGlzLmhhc0FuY2hvcikge1xuICAgICAgdGhpcy5fYW5jaG9yQ2FjaGUgPSB0aGlzLl9idWlsZEFuY2hvckNhY2hlKCk7XG4gICAgfVxuICB9XG4gIHJlc29sdmVCYWNrUmVmZXJlbmNlcyhsaW5lVGV4dCwgY2FwdHVyZUluZGljZXMpIHtcbiAgICBpZiAodHlwZW9mIHRoaXMuc291cmNlICE9PSBcInN0cmluZ1wiKSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoXCJUaGlzIG1ldGhvZCBzaG91bGQgb25seSBiZSBjYWxsZWQgaWYgdGhlIHNvdXJjZSBpcyBhIHN0cmluZ1wiKTtcbiAgICB9XG4gICAgbGV0IGNhcHR1cmVkVmFsdWVzID0gY2FwdHVyZUluZGljZXMubWFwKChjYXB0dXJlKSA9PiB7XG4gICAgICByZXR1cm4gbGluZVRleHQuc3Vic3RyaW5nKGNhcHR1cmUuc3RhcnQsIGNhcHR1cmUuZW5kKTtcbiAgICB9KTtcbiAgICBCQUNLX1JFRkVSRU5DSU5HX0VORC5sYXN0SW5kZXggPSAwO1xuICAgIHJldHVybiB0aGlzLnNvdXJjZS5yZXBsYWNlKEJBQ0tfUkVGRVJFTkNJTkdfRU5ELCAobWF0Y2gsIGcxKSA9PiB7XG4gICAgICByZXR1cm4gZXNjYXBlUmVnRXhwQ2hhcmFjdGVycyhjYXB0dXJlZFZhbHVlc1twYXJzZUludChnMSwgMTApXSB8fCBcIlwiKTtcbiAgICB9KTtcbiAgfVxuICBfYnVpbGRBbmNob3JDYWNoZSgpIHtcbiAgICBpZiAodHlwZW9mIHRoaXMuc291cmNlICE9PSBcInN0cmluZ1wiKSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoXCJUaGlzIG1ldGhvZCBzaG91bGQgb25seSBiZSBjYWxsZWQgaWYgdGhlIHNvdXJjZSBpcyBhIHN0cmluZ1wiKTtcbiAgICB9XG4gICAgbGV0IEEwX0cwX3Jlc3VsdCA9IFtdO1xuICAgIGxldCBBMF9HMV9yZXN1bHQgPSBbXTtcbiAgICBsZXQgQTFfRzBfcmVzdWx0ID0gW107XG4gICAgbGV0IEExX0cxX3Jlc3VsdCA9IFtdO1xuICAgIGxldCBwb3MsIGxlbiwgY2gsIG5leHRDaDtcbiAgICBmb3IgKHBvcyA9IDAsIGxlbiA9IHRoaXMuc291cmNlLmxlbmd0aDsgcG9zIDwgbGVuOyBwb3MrKykge1xuICAgICAgY2ggPSB0aGlzLnNvdXJjZS5jaGFyQXQocG9zKTtcbiAgICAgIEEwX0cwX3Jlc3VsdFtwb3NdID0gY2g7XG4gICAgICBBMF9HMV9yZXN1bHRbcG9zXSA9IGNoO1xuICAgICAgQTFfRzBfcmVzdWx0W3Bvc10gPSBjaDtcbiAgICAgIEExX0cxX3Jlc3VsdFtwb3NdID0gY2g7XG4gICAgICBpZiAoY2ggPT09IFwiXFxcXFwiKSB7XG4gICAgICAgIGlmIChwb3MgKyAxIDwgbGVuKSB7XG4gICAgICAgICAgbmV4dENoID0gdGhpcy5zb3VyY2UuY2hhckF0KHBvcyArIDEpO1xuICAgICAgICAgIGlmIChuZXh0Q2ggPT09IFwiQVwiKSB7XG4gICAgICAgICAgICBBMF9HMF9yZXN1bHRbcG9zICsgMV0gPSBcIlxcdUZGRkZcIjtcbiAgICAgICAgICAgIEEwX0cxX3Jlc3VsdFtwb3MgKyAxXSA9IFwiXFx1RkZGRlwiO1xuICAgICAgICAgICAgQTFfRzBfcmVzdWx0W3BvcyArIDFdID0gXCJBXCI7XG4gICAgICAgICAgICBBMV9HMV9yZXN1bHRbcG9zICsgMV0gPSBcIkFcIjtcbiAgICAgICAgICB9IGVsc2UgaWYgKG5leHRDaCA9PT0gXCJHXCIpIHtcbiAgICAgICAgICAgIEEwX0cwX3Jlc3VsdFtwb3MgKyAxXSA9IFwiXFx1RkZGRlwiO1xuICAgICAgICAgICAgQTBfRzFfcmVzdWx0W3BvcyArIDFdID0gXCJHXCI7XG4gICAgICAgICAgICBBMV9HMF9yZXN1bHRbcG9zICsgMV0gPSBcIlxcdUZGRkZcIjtcbiAgICAgICAgICAgIEExX0cxX3Jlc3VsdFtwb3MgKyAxXSA9IFwiR1wiO1xuICAgICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgICBBMF9HMF9yZXN1bHRbcG9zICsgMV0gPSBuZXh0Q2g7XG4gICAgICAgICAgICBBMF9HMV9yZXN1bHRbcG9zICsgMV0gPSBuZXh0Q2g7XG4gICAgICAgICAgICBBMV9HMF9yZXN1bHRbcG9zICsgMV0gPSBuZXh0Q2g7XG4gICAgICAgICAgICBBMV9HMV9yZXN1bHRbcG9zICsgMV0gPSBuZXh0Q2g7XG4gICAgICAgICAgfVxuICAgICAgICAgIHBvcysrO1xuICAgICAgICB9XG4gICAgICB9XG4gICAgfVxuICAgIHJldHVybiB7XG4gICAgICBBMF9HMDogQTBfRzBfcmVzdWx0LmpvaW4oXCJcIiksXG4gICAgICBBMF9HMTogQTBfRzFfcmVzdWx0LmpvaW4oXCJcIiksXG4gICAgICBBMV9HMDogQTFfRzBfcmVzdWx0LmpvaW4oXCJcIiksXG4gICAgICBBMV9HMTogQTFfRzFfcmVzdWx0LmpvaW4oXCJcIilcbiAgICB9O1xuICB9XG4gIHJlc29sdmVBbmNob3JzKGFsbG93QSwgYWxsb3dHKSB7XG4gICAgaWYgKCF0aGlzLmhhc0FuY2hvciB8fCAhdGhpcy5fYW5jaG9yQ2FjaGUgfHwgdHlwZW9mIHRoaXMuc291cmNlICE9PSBcInN0cmluZ1wiKSB7XG4gICAgICByZXR1cm4gdGhpcy5zb3VyY2U7XG4gICAgfVxuICAgIGlmIChhbGxvd0EpIHtcbiAgICAgIGlmIChhbGxvd0cpIHtcbiAgICAgICAgcmV0dXJuIHRoaXMuX2FuY2hvckNhY2hlLkExX0cxO1xuICAgICAgfSBlbHNlIHtcbiAgICAgICAgcmV0dXJuIHRoaXMuX2FuY2hvckNhY2hlLkExX0cwO1xuICAgICAgfVxuICAgIH0gZWxzZSB7XG4gICAgICBpZiAoYWxsb3dHKSB7XG4gICAgICAgIHJldHVybiB0aGlzLl9hbmNob3JDYWNoZS5BMF9HMTtcbiAgICAgIH0gZWxzZSB7XG4gICAgICAgIHJldHVybiB0aGlzLl9hbmNob3JDYWNoZS5BMF9HMDtcbiAgICAgIH1cbiAgICB9XG4gIH1cbn07XG52YXIgUmVnRXhwU291cmNlTGlzdCA9IGNsYXNzIHtcbiAgX2l0ZW1zO1xuICBfaGFzQW5jaG9ycztcbiAgX2NhY2hlZDtcbiAgX2FuY2hvckNhY2hlO1xuICBjb25zdHJ1Y3RvcigpIHtcbiAgICB0aGlzLl9pdGVtcyA9IFtdO1xuICAgIHRoaXMuX2hhc0FuY2hvcnMgPSBmYWxzZTtcbiAgICB0aGlzLl9jYWNoZWQgPSBudWxsO1xuICAgIHRoaXMuX2FuY2hvckNhY2hlID0ge1xuICAgICAgQTBfRzA6IG51bGwsXG4gICAgICBBMF9HMTogbnVsbCxcbiAgICAgIEExX0cwOiBudWxsLFxuICAgICAgQTFfRzE6IG51bGxcbiAgICB9O1xuICB9XG4gIGRpc3Bvc2UoKSB7XG4gICAgdGhpcy5fZGlzcG9zZUNhY2hlcygpO1xuICB9XG4gIF9kaXNwb3NlQ2FjaGVzKCkge1xuICAgIGlmICh0aGlzLl9jYWNoZWQpIHtcbiAgICAgIHRoaXMuX2NhY2hlZC5kaXNwb3NlKCk7XG4gICAgICB0aGlzLl9jYWNoZWQgPSBudWxsO1xuICAgIH1cbiAgICBpZiAodGhpcy5fYW5jaG9yQ2FjaGUuQTBfRzApIHtcbiAgICAgIHRoaXMuX2FuY2hvckNhY2hlLkEwX0cwLmRpc3Bvc2UoKTtcbiAgICAgIHRoaXMuX2FuY2hvckNhY2hlLkEwX0cwID0gbnVsbDtcbiAgICB9XG4gICAgaWYgKHRoaXMuX2FuY2hvckNhY2hlLkEwX0cxKSB7XG4gICAgICB0aGlzLl9hbmNob3JDYWNoZS5BMF9HMS5kaXNwb3NlKCk7XG4gICAgICB0aGlzLl9hbmNob3JDYWNoZS5BMF9HMSA9IG51bGw7XG4gICAgfVxuICAgIGlmICh0aGlzLl9hbmNob3JDYWNoZS5BMV9HMCkge1xuICAgICAgdGhpcy5fYW5jaG9yQ2FjaGUuQTFfRzAuZGlzcG9zZSgpO1xuICAgICAgdGhpcy5fYW5jaG9yQ2FjaGUuQTFfRzAgPSBudWxsO1xuICAgIH1cbiAgICBpZiAodGhpcy5fYW5jaG9yQ2FjaGUuQTFfRzEpIHtcbiAgICAgIHRoaXMuX2FuY2hvckNhY2hlLkExX0cxLmRpc3Bvc2UoKTtcbiAgICAgIHRoaXMuX2FuY2hvckNhY2hlLkExX0cxID0gbnVsbDtcbiAgICB9XG4gIH1cbiAgcHVzaChpdGVtKSB7XG4gICAgdGhpcy5faXRlbXMucHVzaChpdGVtKTtcbiAgICB0aGlzLl9oYXNBbmNob3JzID0gdGhpcy5faGFzQW5jaG9ycyB8fCBpdGVtLmhhc0FuY2hvcjtcbiAgfVxuICB1bnNoaWZ0KGl0ZW0pIHtcbiAgICB0aGlzLl9pdGVtcy51bnNoaWZ0KGl0ZW0pO1xuICAgIHRoaXMuX2hhc0FuY2hvcnMgPSB0aGlzLl9oYXNBbmNob3JzIHx8IGl0ZW0uaGFzQW5jaG9yO1xuICB9XG4gIGxlbmd0aCgpIHtcbiAgICByZXR1cm4gdGhpcy5faXRlbXMubGVuZ3RoO1xuICB9XG4gIHNldFNvdXJjZShpbmRleCwgbmV3U291cmNlKSB7XG4gICAgaWYgKHRoaXMuX2l0ZW1zW2luZGV4XS5zb3VyY2UgIT09IG5ld1NvdXJjZSkge1xuICAgICAgdGhpcy5fZGlzcG9zZUNhY2hlcygpO1xuICAgICAgdGhpcy5faXRlbXNbaW5kZXhdLnNldFNvdXJjZShuZXdTb3VyY2UpO1xuICAgIH1cbiAgfVxuICBjb21waWxlKG9uaWdMaWIpIHtcbiAgICBpZiAoIXRoaXMuX2NhY2hlZCkge1xuICAgICAgbGV0IHJlZ0V4cHMgPSB0aGlzLl9pdGVtcy5tYXAoKGUpID0+IGUuc291cmNlKTtcbiAgICAgIHRoaXMuX2NhY2hlZCA9IG5ldyBDb21waWxlZFJ1bGUob25pZ0xpYiwgcmVnRXhwcywgdGhpcy5faXRlbXMubWFwKChlKSA9PiBlLnJ1bGVJZCkpO1xuICAgIH1cbiAgICByZXR1cm4gdGhpcy5fY2FjaGVkO1xuICB9XG4gIGNvbXBpbGVBRyhvbmlnTGliLCBhbGxvd0EsIGFsbG93Rykge1xuICAgIGlmICghdGhpcy5faGFzQW5jaG9ycykge1xuICAgICAgcmV0dXJuIHRoaXMuY29tcGlsZShvbmlnTGliKTtcbiAgICB9IGVsc2Uge1xuICAgICAgaWYgKGFsbG93QSkge1xuICAgICAgICBpZiAoYWxsb3dHKSB7XG4gICAgICAgICAgaWYgKCF0aGlzLl9hbmNob3JDYWNoZS5BMV9HMSkge1xuICAgICAgICAgICAgdGhpcy5fYW5jaG9yQ2FjaGUuQTFfRzEgPSB0aGlzLl9yZXNvbHZlQW5jaG9ycyhvbmlnTGliLCBhbGxvd0EsIGFsbG93Ryk7XG4gICAgICAgICAgfVxuICAgICAgICAgIHJldHVybiB0aGlzLl9hbmNob3JDYWNoZS5BMV9HMTtcbiAgICAgICAgfSBlbHNlIHtcbiAgICAgICAgICBpZiAoIXRoaXMuX2FuY2hvckNhY2hlLkExX0cwKSB7XG4gICAgICAgICAgICB0aGlzLl9hbmNob3JDYWNoZS5BMV9HMCA9IHRoaXMuX3Jlc29sdmVBbmNob3JzKG9uaWdMaWIsIGFsbG93QSwgYWxsb3dHKTtcbiAgICAgICAgICB9XG4gICAgICAgICAgcmV0dXJuIHRoaXMuX2FuY2hvckNhY2hlLkExX0cwO1xuICAgICAgICB9XG4gICAgICB9IGVsc2Uge1xuICAgICAgICBpZiAoYWxsb3dHKSB7XG4gICAgICAgICAgaWYgKCF0aGlzLl9hbmNob3JDYWNoZS5BMF9HMSkge1xuICAgICAgICAgICAgdGhpcy5fYW5jaG9yQ2FjaGUuQTBfRzEgPSB0aGlzLl9yZXNvbHZlQW5jaG9ycyhvbmlnTGliLCBhbGxvd0EsIGFsbG93Ryk7XG4gICAgICAgICAgfVxuICAgICAgICAgIHJldHVybiB0aGlzLl9hbmNob3JDYWNoZS5BMF9HMTtcbiAgICAgICAgfSBlbHNlIHtcbiAgICAgICAgICBpZiAoIXRoaXMuX2FuY2hvckNhY2hlLkEwX0cwKSB7XG4gICAgICAgICAgICB0aGlzLl9hbmNob3JDYWNoZS5BMF9HMCA9IHRoaXMuX3Jlc29sdmVBbmNob3JzKG9uaWdMaWIsIGFsbG93QSwgYWxsb3dHKTtcbiAgICAgICAgICB9XG4gICAgICAgICAgcmV0dXJuIHRoaXMuX2FuY2hvckNhY2hlLkEwX0cwO1xuICAgICAgICB9XG4gICAgICB9XG4gICAgfVxuICB9XG4gIF9yZXNvbHZlQW5jaG9ycyhvbmlnTGliLCBhbGxvd0EsIGFsbG93Rykge1xuICAgIGxldCByZWdFeHBzID0gdGhpcy5faXRlbXMubWFwKChlKSA9PiBlLnJlc29sdmVBbmNob3JzKGFsbG93QSwgYWxsb3dHKSk7XG4gICAgcmV0dXJuIG5ldyBDb21waWxlZFJ1bGUob25pZ0xpYiwgcmVnRXhwcywgdGhpcy5faXRlbXMubWFwKChlKSA9PiBlLnJ1bGVJZCkpO1xuICB9XG59O1xudmFyIENvbXBpbGVkUnVsZSA9IGNsYXNzIHtcbiAgY29uc3RydWN0b3Iob25pZ0xpYiwgcmVnRXhwcywgcnVsZXMpIHtcbiAgICB0aGlzLnJlZ0V4cHMgPSByZWdFeHBzO1xuICAgIHRoaXMucnVsZXMgPSBydWxlcztcbiAgICB0aGlzLnNjYW5uZXIgPSBvbmlnTGliLmNyZWF0ZU9uaWdTY2FubmVyKHJlZ0V4cHMpO1xuICB9XG4gIHNjYW5uZXI7XG4gIGRpc3Bvc2UoKSB7XG4gICAgaWYgKHR5cGVvZiB0aGlzLnNjYW5uZXIuZGlzcG9zZSA9PT0gXCJmdW5jdGlvblwiKSB7XG4gICAgICB0aGlzLnNjYW5uZXIuZGlzcG9zZSgpO1xuICAgIH1cbiAgfVxuICB0b1N0cmluZygpIHtcbiAgICBjb25zdCByID0gW107XG4gICAgZm9yIChsZXQgaSA9IDAsIGxlbiA9IHRoaXMucnVsZXMubGVuZ3RoOyBpIDwgbGVuOyBpKyspIHtcbiAgICAgIHIucHVzaChcIiAgIC0gXCIgKyB0aGlzLnJ1bGVzW2ldICsgXCI6IFwiICsgdGhpcy5yZWdFeHBzW2ldKTtcbiAgICB9XG4gICAgcmV0dXJuIHIuam9pbihcIlxcblwiKTtcbiAgfVxuICBmaW5kTmV4dE1hdGNoU3luYyhzdHJpbmcsIHN0YXJ0UG9zaXRpb24sIG9wdGlvbnMpIHtcbiAgICBjb25zdCByZXN1bHQgPSB0aGlzLnNjYW5uZXIuZmluZE5leHRNYXRjaFN5bmMoc3RyaW5nLCBzdGFydFBvc2l0aW9uLCBvcHRpb25zKTtcbiAgICBpZiAoIXJlc3VsdCkge1xuICAgICAgcmV0dXJuIG51bGw7XG4gICAgfVxuICAgIHJldHVybiB7XG4gICAgICBydWxlSWQ6IHRoaXMucnVsZXNbcmVzdWx0LmluZGV4XSxcbiAgICAgIGNhcHR1cmVJbmRpY2VzOiByZXN1bHQuY2FwdHVyZUluZGljZXNcbiAgICB9O1xuICB9XG59O1xuXG4vLyBzcmMvZ3JhbW1hci9iYXNpY1Njb3Blc0F0dHJpYnV0ZVByb3ZpZGVyLnRzXG52YXIgQmFzaWNTY29wZUF0dHJpYnV0ZXMgPSBjbGFzcyB7XG4gIGNvbnN0cnVjdG9yKGxhbmd1YWdlSWQsIHRva2VuVHlwZSkge1xuICAgIHRoaXMubGFuZ3VhZ2VJZCA9IGxhbmd1YWdlSWQ7XG4gICAgdGhpcy50b2tlblR5cGUgPSB0b2tlblR5cGU7XG4gIH1cbn07XG52YXIgQmFzaWNTY29wZUF0dHJpYnV0ZXNQcm92aWRlciA9IGNsYXNzIF9CYXNpY1Njb3BlQXR0cmlidXRlc1Byb3ZpZGVyIHtcbiAgX2RlZmF1bHRBdHRyaWJ1dGVzO1xuICBfZW1iZWRkZWRMYW5ndWFnZXNNYXRjaGVyO1xuICBjb25zdHJ1Y3Rvcihpbml0aWFsTGFuZ3VhZ2VJZCwgZW1iZWRkZWRMYW5ndWFnZXMpIHtcbiAgICB0aGlzLl9kZWZhdWx0QXR0cmlidXRlcyA9IG5ldyBCYXNpY1Njb3BlQXR0cmlidXRlcyhpbml0aWFsTGFuZ3VhZ2VJZCwgOCAvKiBOb3RTZXQgKi8pO1xuICAgIHRoaXMuX2VtYmVkZGVkTGFuZ3VhZ2VzTWF0Y2hlciA9IG5ldyBTY29wZU1hdGNoZXIoT2JqZWN0LmVudHJpZXMoZW1iZWRkZWRMYW5ndWFnZXMgfHwge30pKTtcbiAgfVxuICBnZXREZWZhdWx0QXR0cmlidXRlcygpIHtcbiAgICByZXR1cm4gdGhpcy5fZGVmYXVsdEF0dHJpYnV0ZXM7XG4gIH1cbiAgZ2V0QmFzaWNTY29wZUF0dHJpYnV0ZXMoc2NvcGVOYW1lKSB7XG4gICAgaWYgKHNjb3BlTmFtZSA9PT0gbnVsbCkge1xuICAgICAgcmV0dXJuIF9CYXNpY1Njb3BlQXR0cmlidXRlc1Byb3ZpZGVyLl9OVUxMX1NDT1BFX01FVEFEQVRBO1xuICAgIH1cbiAgICByZXR1cm4gdGhpcy5fZ2V0QmFzaWNTY29wZUF0dHJpYnV0ZXMuZ2V0KHNjb3BlTmFtZSk7XG4gIH1cbiAgc3RhdGljIF9OVUxMX1NDT1BFX01FVEFEQVRBID0gbmV3IEJhc2ljU2NvcGVBdHRyaWJ1dGVzKDAsIDApO1xuICBfZ2V0QmFzaWNTY29wZUF0dHJpYnV0ZXMgPSBuZXcgQ2FjaGVkRm4oKHNjb3BlTmFtZSkgPT4ge1xuICAgIGNvbnN0IGxhbmd1YWdlSWQgPSB0aGlzLl9zY29wZVRvTGFuZ3VhZ2Uoc2NvcGVOYW1lKTtcbiAgICBjb25zdCBzdGFuZGFyZFRva2VuVHlwZSA9IHRoaXMuX3RvU3RhbmRhcmRUb2tlblR5cGUoc2NvcGVOYW1lKTtcbiAgICByZXR1cm4gbmV3IEJhc2ljU2NvcGVBdHRyaWJ1dGVzKGxhbmd1YWdlSWQsIHN0YW5kYXJkVG9rZW5UeXBlKTtcbiAgfSk7XG4gIC8qKlxuICAgKiBHaXZlbiBhIHByb2R1Y2VkIFRNIHNjb3BlLCByZXR1cm4gdGhlIGxhbmd1YWdlIHRoYXQgdG9rZW4gZGVzY3JpYmVzIG9yIG51bGwgaWYgdW5rbm93bi5cbiAgICogZS5nLiBzb3VyY2UuaHRtbCA9PiBodG1sLCBzb3VyY2UuY3NzLmVtYmVkZGVkLmh0bWwgPT4gY3NzLCBwdW5jdHVhdGlvbi5kZWZpbml0aW9uLnRhZy5odG1sID0+IG51bGxcbiAgICovXG4gIF9zY29wZVRvTGFuZ3VhZ2Uoc2NvcGUpIHtcbiAgICByZXR1cm4gdGhpcy5fZW1iZWRkZWRMYW5ndWFnZXNNYXRjaGVyLm1hdGNoKHNjb3BlKSB8fCAwO1xuICB9XG4gIF90b1N0YW5kYXJkVG9rZW5UeXBlKHNjb3BlTmFtZSkge1xuICAgIGNvbnN0IG0gPSBzY29wZU5hbWUubWF0Y2goX0Jhc2ljU2NvcGVBdHRyaWJ1dGVzUHJvdmlkZXIuU1RBTkRBUkRfVE9LRU5fVFlQRV9SRUdFWFApO1xuICAgIGlmICghbSkge1xuICAgICAgcmV0dXJuIDggLyogTm90U2V0ICovO1xuICAgIH1cbiAgICBzd2l0Y2ggKG1bMV0pIHtcbiAgICAgIGNhc2UgXCJjb21tZW50XCI6XG4gICAgICAgIHJldHVybiAxIC8qIENvbW1lbnQgKi87XG4gICAgICBjYXNlIFwic3RyaW5nXCI6XG4gICAgICAgIHJldHVybiAyIC8qIFN0cmluZyAqLztcbiAgICAgIGNhc2UgXCJyZWdleFwiOlxuICAgICAgICByZXR1cm4gMyAvKiBSZWdFeCAqLztcbiAgICAgIGNhc2UgXCJtZXRhLmVtYmVkZGVkXCI6XG4gICAgICAgIHJldHVybiAwIC8qIE90aGVyICovO1xuICAgIH1cbiAgICB0aHJvdyBuZXcgRXJyb3IoXCJVbmV4cGVjdGVkIG1hdGNoIGZvciBzdGFuZGFyZCB0b2tlbiB0eXBlIVwiKTtcbiAgfVxuICBzdGF0aWMgU1RBTkRBUkRfVE9LRU5fVFlQRV9SRUdFWFAgPSAvXFxiKGNvbW1lbnR8c3RyaW5nfHJlZ2V4fG1ldGFcXC5lbWJlZGRlZClcXGIvO1xufTtcbnZhciBTY29wZU1hdGNoZXIgPSBjbGFzcyB7XG4gIHZhbHVlcztcbiAgc2NvcGVzUmVnRXhwO1xuICBjb25zdHJ1Y3Rvcih2YWx1ZXMpIHtcbiAgICBpZiAodmFsdWVzLmxlbmd0aCA9PT0gMCkge1xuICAgICAgdGhpcy52YWx1ZXMgPSBudWxsO1xuICAgICAgdGhpcy5zY29wZXNSZWdFeHAgPSBudWxsO1xuICAgIH0gZWxzZSB7XG4gICAgICB0aGlzLnZhbHVlcyA9IG5ldyBNYXAodmFsdWVzKTtcbiAgICAgIGNvbnN0IGVzY2FwZWRTY29wZXMgPSB2YWx1ZXMubWFwKFxuICAgICAgICAoW3Njb3BlTmFtZSwgdmFsdWVdKSA9PiBlc2NhcGVSZWdFeHBDaGFyYWN0ZXJzKHNjb3BlTmFtZSlcbiAgICAgICk7XG4gICAgICBlc2NhcGVkU2NvcGVzLnNvcnQoKTtcbiAgICAgIGVzY2FwZWRTY29wZXMucmV2ZXJzZSgpO1xuICAgICAgdGhpcy5zY29wZXNSZWdFeHAgPSBuZXcgUmVnRXhwKFxuICAgICAgICBgXigoJHtlc2NhcGVkU2NvcGVzLmpvaW4oXCIpfChcIil9KSkoJHxcXFxcLilgLFxuICAgICAgICBcIlwiXG4gICAgICApO1xuICAgIH1cbiAgfVxuICBtYXRjaChzY29wZSkge1xuICAgIGlmICghdGhpcy5zY29wZXNSZWdFeHApIHtcbiAgICAgIHJldHVybiB2b2lkIDA7XG4gICAgfVxuICAgIGNvbnN0IG0gPSBzY29wZS5tYXRjaCh0aGlzLnNjb3Blc1JlZ0V4cCk7XG4gICAgaWYgKCFtKSB7XG4gICAgICByZXR1cm4gdm9pZCAwO1xuICAgIH1cbiAgICByZXR1cm4gdGhpcy52YWx1ZXMuZ2V0KG1bMV0pO1xuICB9XG59O1xuXG4vLyBzcmMvZGVidWcudHNcbnZhciBEZWJ1Z0ZsYWdzID0ge1xuICBJbkRlYnVnTW9kZTogdHlwZW9mIHByb2Nlc3MgIT09IFwidW5kZWZpbmVkXCIgJiYgISFwcm9jZXNzLmVudltcIlZTQ09ERV9URVhUTUFURV9ERUJVR1wiXVxufTtcbnZhciBVc2VPbmlndXJ1bWFGaW5kT3B0aW9ucyA9IGZhbHNlO1xuXG4vLyBzcmMvZ3JhbW1hci90b2tlbml6ZVN0cmluZy50c1xudmFyIFRva2VuaXplU3RyaW5nUmVzdWx0ID0gY2xhc3Mge1xuICBjb25zdHJ1Y3RvcihzdGFjaywgc3RvcHBlZEVhcmx5KSB7XG4gICAgdGhpcy5zdGFjayA9IHN0YWNrO1xuICAgIHRoaXMuc3RvcHBlZEVhcmx5ID0gc3RvcHBlZEVhcmx5O1xuICB9XG59O1xuZnVuY3Rpb24gX3Rva2VuaXplU3RyaW5nKGdyYW1tYXIsIGxpbmVUZXh0LCBpc0ZpcnN0TGluZSwgbGluZVBvcywgc3RhY2ssIGxpbmVUb2tlbnMsIGNoZWNrV2hpbGVDb25kaXRpb25zLCB0aW1lTGltaXQpIHtcbiAgY29uc3QgbGluZUxlbmd0aCA9IGxpbmVUZXh0LmNvbnRlbnQubGVuZ3RoO1xuICBsZXQgU1RPUCA9IGZhbHNlO1xuICBsZXQgYW5jaG9yUG9zaXRpb24gPSAtMTtcbiAgaWYgKGNoZWNrV2hpbGVDb25kaXRpb25zKSB7XG4gICAgY29uc3Qgd2hpbGVDaGVja1Jlc3VsdCA9IF9jaGVja1doaWxlQ29uZGl0aW9ucyhcbiAgICAgIGdyYW1tYXIsXG4gICAgICBsaW5lVGV4dCxcbiAgICAgIGlzRmlyc3RMaW5lLFxuICAgICAgbGluZVBvcyxcbiAgICAgIHN0YWNrLFxuICAgICAgbGluZVRva2Vuc1xuICAgICk7XG4gICAgc3RhY2sgPSB3aGlsZUNoZWNrUmVzdWx0LnN0YWNrO1xuICAgIGxpbmVQb3MgPSB3aGlsZUNoZWNrUmVzdWx0LmxpbmVQb3M7XG4gICAgaXNGaXJzdExpbmUgPSB3aGlsZUNoZWNrUmVzdWx0LmlzRmlyc3RMaW5lO1xuICAgIGFuY2hvclBvc2l0aW9uID0gd2hpbGVDaGVja1Jlc3VsdC5hbmNob3JQb3NpdGlvbjtcbiAgfVxuICBjb25zdCBzdGFydFRpbWUgPSBEYXRlLm5vdygpO1xuICB3aGlsZSAoIVNUT1ApIHtcbiAgICBpZiAodGltZUxpbWl0ICE9PSAwKSB7XG4gICAgICBjb25zdCBlbGFwc2VkVGltZSA9IERhdGUubm93KCkgLSBzdGFydFRpbWU7XG4gICAgICBpZiAoZWxhcHNlZFRpbWUgPiB0aW1lTGltaXQpIHtcbiAgICAgICAgcmV0dXJuIG5ldyBUb2tlbml6ZVN0cmluZ1Jlc3VsdChzdGFjaywgdHJ1ZSk7XG4gICAgICB9XG4gICAgfVxuICAgIHNjYW5OZXh0KCk7XG4gIH1cbiAgcmV0dXJuIG5ldyBUb2tlbml6ZVN0cmluZ1Jlc3VsdChzdGFjaywgZmFsc2UpO1xuICBmdW5jdGlvbiBzY2FuTmV4dCgpIHtcbiAgICBpZiAoZmFsc2UpIHtcbiAgICAgIGNvbnNvbGUubG9nKFwiXCIpO1xuICAgICAgY29uc29sZS5sb2coXG4gICAgICAgIGBAQHNjYW5OZXh0ICR7bGluZVBvc306IHwke2xpbmVUZXh0LmNvbnRlbnQuc3Vic3RyKGxpbmVQb3MpLnJlcGxhY2UoL1xcbiQvLCBcIlxcXFxuXCIpfXxgXG4gICAgICApO1xuICAgIH1cbiAgICBjb25zdCByID0gbWF0Y2hSdWxlT3JJbmplY3Rpb25zKFxuICAgICAgZ3JhbW1hcixcbiAgICAgIGxpbmVUZXh0LFxuICAgICAgaXNGaXJzdExpbmUsXG4gICAgICBsaW5lUG9zLFxuICAgICAgc3RhY2ssXG4gICAgICBhbmNob3JQb3NpdGlvblxuICAgICk7XG4gICAgaWYgKCFyKSB7XG4gICAgICBsaW5lVG9rZW5zLnByb2R1Y2Uoc3RhY2ssIGxpbmVMZW5ndGgpO1xuICAgICAgU1RPUCA9IHRydWU7XG4gICAgICByZXR1cm47XG4gICAgfVxuICAgIGNvbnN0IGNhcHR1cmVJbmRpY2VzID0gci5jYXB0dXJlSW5kaWNlcztcbiAgICBjb25zdCBtYXRjaGVkUnVsZUlkID0gci5tYXRjaGVkUnVsZUlkO1xuICAgIGNvbnN0IGhhc0FkdmFuY2VkID0gY2FwdHVyZUluZGljZXMgJiYgY2FwdHVyZUluZGljZXMubGVuZ3RoID4gMCA/IGNhcHR1cmVJbmRpY2VzWzBdLmVuZCA+IGxpbmVQb3MgOiBmYWxzZTtcbiAgICBpZiAobWF0Y2hlZFJ1bGVJZCA9PT0gZW5kUnVsZUlkKSB7XG4gICAgICBjb25zdCBwb3BwZWRSdWxlID0gc3RhY2suZ2V0UnVsZShncmFtbWFyKTtcbiAgICAgIGlmIChmYWxzZSkge1xuICAgICAgICBjb25zb2xlLmxvZyhcbiAgICAgICAgICBcIiAgcG9wcGluZyBcIiArIHBvcHBlZFJ1bGUuZGVidWdOYW1lICsgXCIgLSBcIiArIHBvcHBlZFJ1bGUuZGVidWdFbmRSZWdFeHBcbiAgICAgICAgKTtcbiAgICAgIH1cbiAgICAgIGxpbmVUb2tlbnMucHJvZHVjZShzdGFjaywgY2FwdHVyZUluZGljZXNbMF0uc3RhcnQpO1xuICAgICAgc3RhY2sgPSBzdGFjay53aXRoQ29udGVudE5hbWVTY29wZXNMaXN0KHN0YWNrLm5hbWVTY29wZXNMaXN0KTtcbiAgICAgIGhhbmRsZUNhcHR1cmVzKFxuICAgICAgICBncmFtbWFyLFxuICAgICAgICBsaW5lVGV4dCxcbiAgICAgICAgaXNGaXJzdExpbmUsXG4gICAgICAgIHN0YWNrLFxuICAgICAgICBsaW5lVG9rZW5zLFxuICAgICAgICBwb3BwZWRSdWxlLmVuZENhcHR1cmVzLFxuICAgICAgICBjYXB0dXJlSW5kaWNlc1xuICAgICAgKTtcbiAgICAgIGxpbmVUb2tlbnMucHJvZHVjZShzdGFjaywgY2FwdHVyZUluZGljZXNbMF0uZW5kKTtcbiAgICAgIGNvbnN0IHBvcHBlZCA9IHN0YWNrO1xuICAgICAgc3RhY2sgPSBzdGFjay5wYXJlbnQ7XG4gICAgICBhbmNob3JQb3NpdGlvbiA9IHBvcHBlZC5nZXRBbmNob3JQb3MoKTtcbiAgICAgIGlmICghaGFzQWR2YW5jZWQgJiYgcG9wcGVkLmdldEVudGVyUG9zKCkgPT09IGxpbmVQb3MpIHtcbiAgICAgICAgaWYgKGZhbHNlKSB7XG4gICAgICAgICAgY29uc29sZS5lcnJvcihcbiAgICAgICAgICAgIFwiWzFdIC0gR3JhbW1hciBpcyBpbiBhbiBlbmRsZXNzIGxvb3AgLSBHcmFtbWFyIHB1c2hlZCAmIHBvcHBlZCBhIHJ1bGUgd2l0aG91dCBhZHZhbmNpbmdcIlxuICAgICAgICAgICk7XG4gICAgICAgIH1cbiAgICAgICAgc3RhY2sgPSBwb3BwZWQ7XG4gICAgICAgIGxpbmVUb2tlbnMucHJvZHVjZShzdGFjaywgbGluZUxlbmd0aCk7XG4gICAgICAgIFNUT1AgPSB0cnVlO1xuICAgICAgICByZXR1cm47XG4gICAgICB9XG4gICAgfSBlbHNlIHtcbiAgICAgIGNvbnN0IF9ydWxlID0gZ3JhbW1hci5nZXRSdWxlKG1hdGNoZWRSdWxlSWQpO1xuICAgICAgbGluZVRva2Vucy5wcm9kdWNlKHN0YWNrLCBjYXB0dXJlSW5kaWNlc1swXS5zdGFydCk7XG4gICAgICBjb25zdCBiZWZvcmVQdXNoID0gc3RhY2s7XG4gICAgICBjb25zdCBzY29wZU5hbWUgPSBfcnVsZS5nZXROYW1lKGxpbmVUZXh0LmNvbnRlbnQsIGNhcHR1cmVJbmRpY2VzKTtcbiAgICAgIGNvbnN0IG5hbWVTY29wZXNMaXN0ID0gc3RhY2suY29udGVudE5hbWVTY29wZXNMaXN0LnB1c2hBdHRyaWJ1dGVkKFxuICAgICAgICBzY29wZU5hbWUsXG4gICAgICAgIGdyYW1tYXJcbiAgICAgICk7XG4gICAgICBzdGFjayA9IHN0YWNrLnB1c2goXG4gICAgICAgIG1hdGNoZWRSdWxlSWQsXG4gICAgICAgIGxpbmVQb3MsXG4gICAgICAgIGFuY2hvclBvc2l0aW9uLFxuICAgICAgICBjYXB0dXJlSW5kaWNlc1swXS5lbmQgPT09IGxpbmVMZW5ndGgsXG4gICAgICAgIG51bGwsXG4gICAgICAgIG5hbWVTY29wZXNMaXN0LFxuICAgICAgICBuYW1lU2NvcGVzTGlzdFxuICAgICAgKTtcbiAgICAgIGlmIChfcnVsZSBpbnN0YW5jZW9mIEJlZ2luRW5kUnVsZSkge1xuICAgICAgICBjb25zdCBwdXNoZWRSdWxlID0gX3J1bGU7XG4gICAgICAgIGlmIChmYWxzZSkge1xuICAgICAgICAgIGNvbnNvbGUubG9nKFxuICAgICAgICAgICAgXCIgIHB1c2hpbmcgXCIgKyBwdXNoZWRSdWxlLmRlYnVnTmFtZSArIFwiIC0gXCIgKyBwdXNoZWRSdWxlLmRlYnVnQmVnaW5SZWdFeHBcbiAgICAgICAgICApO1xuICAgICAgICB9XG4gICAgICAgIGhhbmRsZUNhcHR1cmVzKFxuICAgICAgICAgIGdyYW1tYXIsXG4gICAgICAgICAgbGluZVRleHQsXG4gICAgICAgICAgaXNGaXJzdExpbmUsXG4gICAgICAgICAgc3RhY2ssXG4gICAgICAgICAgbGluZVRva2VucyxcbiAgICAgICAgICBwdXNoZWRSdWxlLmJlZ2luQ2FwdHVyZXMsXG4gICAgICAgICAgY2FwdHVyZUluZGljZXNcbiAgICAgICAgKTtcbiAgICAgICAgbGluZVRva2Vucy5wcm9kdWNlKHN0YWNrLCBjYXB0dXJlSW5kaWNlc1swXS5lbmQpO1xuICAgICAgICBhbmNob3JQb3NpdGlvbiA9IGNhcHR1cmVJbmRpY2VzWzBdLmVuZDtcbiAgICAgICAgY29uc3QgY29udGVudE5hbWUgPSBwdXNoZWRSdWxlLmdldENvbnRlbnROYW1lKFxuICAgICAgICAgIGxpbmVUZXh0LmNvbnRlbnQsXG4gICAgICAgICAgY2FwdHVyZUluZGljZXNcbiAgICAgICAgKTtcbiAgICAgICAgY29uc3QgY29udGVudE5hbWVTY29wZXNMaXN0ID0gbmFtZVNjb3Blc0xpc3QucHVzaEF0dHJpYnV0ZWQoXG4gICAgICAgICAgY29udGVudE5hbWUsXG4gICAgICAgICAgZ3JhbW1hclxuICAgICAgICApO1xuICAgICAgICBzdGFjayA9IHN0YWNrLndpdGhDb250ZW50TmFtZVNjb3Blc0xpc3QoY29udGVudE5hbWVTY29wZXNMaXN0KTtcbiAgICAgICAgaWYgKHB1c2hlZFJ1bGUuZW5kSGFzQmFja1JlZmVyZW5jZXMpIHtcbiAgICAgICAgICBzdGFjayA9IHN0YWNrLndpdGhFbmRSdWxlKFxuICAgICAgICAgICAgcHVzaGVkUnVsZS5nZXRFbmRXaXRoUmVzb2x2ZWRCYWNrUmVmZXJlbmNlcyhcbiAgICAgICAgICAgICAgbGluZVRleHQuY29udGVudCxcbiAgICAgICAgICAgICAgY2FwdHVyZUluZGljZXNcbiAgICAgICAgICAgIClcbiAgICAgICAgICApO1xuICAgICAgICB9XG4gICAgICAgIGlmICghaGFzQWR2YW5jZWQgJiYgYmVmb3JlUHVzaC5oYXNTYW1lUnVsZUFzKHN0YWNrKSkge1xuICAgICAgICAgIGlmIChmYWxzZSkge1xuICAgICAgICAgICAgY29uc29sZS5lcnJvcihcbiAgICAgICAgICAgICAgXCJbMl0gLSBHcmFtbWFyIGlzIGluIGFuIGVuZGxlc3MgbG9vcCAtIEdyYW1tYXIgcHVzaGVkIHRoZSBzYW1lIHJ1bGUgd2l0aG91dCBhZHZhbmNpbmdcIlxuICAgICAgICAgICAgKTtcbiAgICAgICAgICB9XG4gICAgICAgICAgc3RhY2sgPSBzdGFjay5wb3AoKTtcbiAgICAgICAgICBsaW5lVG9rZW5zLnByb2R1Y2Uoc3RhY2ssIGxpbmVMZW5ndGgpO1xuICAgICAgICAgIFNUT1AgPSB0cnVlO1xuICAgICAgICAgIHJldHVybjtcbiAgICAgICAgfVxuICAgICAgfSBlbHNlIGlmIChfcnVsZSBpbnN0YW5jZW9mIEJlZ2luV2hpbGVSdWxlKSB7XG4gICAgICAgIGNvbnN0IHB1c2hlZFJ1bGUgPSBfcnVsZTtcbiAgICAgICAgaWYgKGZhbHNlKSB7XG4gICAgICAgICAgY29uc29sZS5sb2coXCIgIHB1c2hpbmcgXCIgKyBwdXNoZWRSdWxlLmRlYnVnTmFtZSk7XG4gICAgICAgIH1cbiAgICAgICAgaGFuZGxlQ2FwdHVyZXMoXG4gICAgICAgICAgZ3JhbW1hcixcbiAgICAgICAgICBsaW5lVGV4dCxcbiAgICAgICAgICBpc0ZpcnN0TGluZSxcbiAgICAgICAgICBzdGFjayxcbiAgICAgICAgICBsaW5lVG9rZW5zLFxuICAgICAgICAgIHB1c2hlZFJ1bGUuYmVnaW5DYXB0dXJlcyxcbiAgICAgICAgICBjYXB0dXJlSW5kaWNlc1xuICAgICAgICApO1xuICAgICAgICBsaW5lVG9rZW5zLnByb2R1Y2Uoc3RhY2ssIGNhcHR1cmVJbmRpY2VzWzBdLmVuZCk7XG4gICAgICAgIGFuY2hvclBvc2l0aW9uID0gY2FwdHVyZUluZGljZXNbMF0uZW5kO1xuICAgICAgICBjb25zdCBjb250ZW50TmFtZSA9IHB1c2hlZFJ1bGUuZ2V0Q29udGVudE5hbWUoXG4gICAgICAgICAgbGluZVRleHQuY29udGVudCxcbiAgICAgICAgICBjYXB0dXJlSW5kaWNlc1xuICAgICAgICApO1xuICAgICAgICBjb25zdCBjb250ZW50TmFtZVNjb3Blc0xpc3QgPSBuYW1lU2NvcGVzTGlzdC5wdXNoQXR0cmlidXRlZChcbiAgICAgICAgICBjb250ZW50TmFtZSxcbiAgICAgICAgICBncmFtbWFyXG4gICAgICAgICk7XG4gICAgICAgIHN0YWNrID0gc3RhY2sud2l0aENvbnRlbnROYW1lU2NvcGVzTGlzdChjb250ZW50TmFtZVNjb3Blc0xpc3QpO1xuICAgICAgICBpZiAocHVzaGVkUnVsZS53aGlsZUhhc0JhY2tSZWZlcmVuY2VzKSB7XG4gICAgICAgICAgc3RhY2sgPSBzdGFjay53aXRoRW5kUnVsZShcbiAgICAgICAgICAgIHB1c2hlZFJ1bGUuZ2V0V2hpbGVXaXRoUmVzb2x2ZWRCYWNrUmVmZXJlbmNlcyhcbiAgICAgICAgICAgICAgbGluZVRleHQuY29udGVudCxcbiAgICAgICAgICAgICAgY2FwdHVyZUluZGljZXNcbiAgICAgICAgICAgIClcbiAgICAgICAgICApO1xuICAgICAgICB9XG4gICAgICAgIGlmICghaGFzQWR2YW5jZWQgJiYgYmVmb3JlUHVzaC5oYXNTYW1lUnVsZUFzKHN0YWNrKSkge1xuICAgICAgICAgIGlmIChmYWxzZSkge1xuICAgICAgICAgICAgY29uc29sZS5lcnJvcihcbiAgICAgICAgICAgICAgXCJbM10gLSBHcmFtbWFyIGlzIGluIGFuIGVuZGxlc3MgbG9vcCAtIEdyYW1tYXIgcHVzaGVkIHRoZSBzYW1lIHJ1bGUgd2l0aG91dCBhZHZhbmNpbmdcIlxuICAgICAgICAgICAgKTtcbiAgICAgICAgICB9XG4gICAgICAgICAgc3RhY2sgPSBzdGFjay5wb3AoKTtcbiAgICAgICAgICBsaW5lVG9rZW5zLnByb2R1Y2Uoc3RhY2ssIGxpbmVMZW5ndGgpO1xuICAgICAgICAgIFNUT1AgPSB0cnVlO1xuICAgICAgICAgIHJldHVybjtcbiAgICAgICAgfVxuICAgICAgfSBlbHNlIHtcbiAgICAgICAgY29uc3QgbWF0Y2hpbmdSdWxlID0gX3J1bGU7XG4gICAgICAgIGlmIChmYWxzZSkge1xuICAgICAgICAgIGNvbnNvbGUubG9nKFxuICAgICAgICAgICAgXCIgIG1hdGNoZWQgXCIgKyBtYXRjaGluZ1J1bGUuZGVidWdOYW1lICsgXCIgLSBcIiArIG1hdGNoaW5nUnVsZS5kZWJ1Z01hdGNoUmVnRXhwXG4gICAgICAgICAgKTtcbiAgICAgICAgfVxuICAgICAgICBoYW5kbGVDYXB0dXJlcyhcbiAgICAgICAgICBncmFtbWFyLFxuICAgICAgICAgIGxpbmVUZXh0LFxuICAgICAgICAgIGlzRmlyc3RMaW5lLFxuICAgICAgICAgIHN0YWNrLFxuICAgICAgICAgIGxpbmVUb2tlbnMsXG4gICAgICAgICAgbWF0Y2hpbmdSdWxlLmNhcHR1cmVzLFxuICAgICAgICAgIGNhcHR1cmVJbmRpY2VzXG4gICAgICAgICk7XG4gICAgICAgIGxpbmVUb2tlbnMucHJvZHVjZShzdGFjaywgY2FwdHVyZUluZGljZXNbMF0uZW5kKTtcbiAgICAgICAgc3RhY2sgPSBzdGFjay5wb3AoKTtcbiAgICAgICAgaWYgKCFoYXNBZHZhbmNlZCkge1xuICAgICAgICAgIGlmIChmYWxzZSkge1xuICAgICAgICAgICAgY29uc29sZS5lcnJvcihcbiAgICAgICAgICAgICAgXCJbNF0gLSBHcmFtbWFyIGlzIGluIGFuIGVuZGxlc3MgbG9vcCAtIEdyYW1tYXIgaXMgbm90IGFkdmFuY2luZywgbm9yIGlzIGl0IHB1c2hpbmcvcG9wcGluZ1wiXG4gICAgICAgICAgICApO1xuICAgICAgICAgIH1cbiAgICAgICAgICBzdGFjayA9IHN0YWNrLnNhZmVQb3AoKTtcbiAgICAgICAgICBsaW5lVG9rZW5zLnByb2R1Y2Uoc3RhY2ssIGxpbmVMZW5ndGgpO1xuICAgICAgICAgIFNUT1AgPSB0cnVlO1xuICAgICAgICAgIHJldHVybjtcbiAgICAgICAgfVxuICAgICAgfVxuICAgIH1cbiAgICBpZiAoY2FwdHVyZUluZGljZXNbMF0uZW5kID4gbGluZVBvcykge1xuICAgICAgbGluZVBvcyA9IGNhcHR1cmVJbmRpY2VzWzBdLmVuZDtcbiAgICAgIGlzRmlyc3RMaW5lID0gZmFsc2U7XG4gICAgfVxuICB9XG59XG5mdW5jdGlvbiBfY2hlY2tXaGlsZUNvbmRpdGlvbnMoZ3JhbW1hciwgbGluZVRleHQsIGlzRmlyc3RMaW5lLCBsaW5lUG9zLCBzdGFjaywgbGluZVRva2Vucykge1xuICBsZXQgYW5jaG9yUG9zaXRpb24gPSBzdGFjay5iZWdpblJ1bGVDYXB0dXJlZEVPTCA/IDAgOiAtMTtcbiAgY29uc3Qgd2hpbGVSdWxlcyA9IFtdO1xuICBmb3IgKGxldCBub2RlID0gc3RhY2s7IG5vZGU7IG5vZGUgPSBub2RlLnBvcCgpKSB7XG4gICAgY29uc3Qgbm9kZVJ1bGUgPSBub2RlLmdldFJ1bGUoZ3JhbW1hcik7XG4gICAgaWYgKG5vZGVSdWxlIGluc3RhbmNlb2YgQmVnaW5XaGlsZVJ1bGUpIHtcbiAgICAgIHdoaWxlUnVsZXMucHVzaCh7XG4gICAgICAgIHJ1bGU6IG5vZGVSdWxlLFxuICAgICAgICBzdGFjazogbm9kZVxuICAgICAgfSk7XG4gICAgfVxuICB9XG4gIGZvciAobGV0IHdoaWxlUnVsZSA9IHdoaWxlUnVsZXMucG9wKCk7IHdoaWxlUnVsZTsgd2hpbGVSdWxlID0gd2hpbGVSdWxlcy5wb3AoKSkge1xuICAgIGNvbnN0IHsgcnVsZVNjYW5uZXIsIGZpbmRPcHRpb25zIH0gPSBwcmVwYXJlUnVsZVdoaWxlU2VhcmNoKHdoaWxlUnVsZS5ydWxlLCBncmFtbWFyLCB3aGlsZVJ1bGUuc3RhY2suZW5kUnVsZSwgaXNGaXJzdExpbmUsIGxpbmVQb3MgPT09IGFuY2hvclBvc2l0aW9uKTtcbiAgICBjb25zdCByID0gcnVsZVNjYW5uZXIuZmluZE5leHRNYXRjaFN5bmMobGluZVRleHQsIGxpbmVQb3MsIGZpbmRPcHRpb25zKTtcbiAgICBpZiAoZmFsc2UpIHtcbiAgICAgIGNvbnNvbGUubG9nKFwiICBzY2FubmluZyBmb3Igd2hpbGUgcnVsZVwiKTtcbiAgICAgIGNvbnNvbGUubG9nKHJ1bGVTY2FubmVyLnRvU3RyaW5nKCkpO1xuICAgIH1cbiAgICBpZiAocikge1xuICAgICAgY29uc3QgbWF0Y2hlZFJ1bGVJZCA9IHIucnVsZUlkO1xuICAgICAgaWYgKG1hdGNoZWRSdWxlSWQgIT09IHdoaWxlUnVsZUlkKSB7XG4gICAgICAgIHN0YWNrID0gd2hpbGVSdWxlLnN0YWNrLnBvcCgpO1xuICAgICAgICBicmVhaztcbiAgICAgIH1cbiAgICAgIGlmIChyLmNhcHR1cmVJbmRpY2VzICYmIHIuY2FwdHVyZUluZGljZXMubGVuZ3RoKSB7XG4gICAgICAgIGxpbmVUb2tlbnMucHJvZHVjZSh3aGlsZVJ1bGUuc3RhY2ssIHIuY2FwdHVyZUluZGljZXNbMF0uc3RhcnQpO1xuICAgICAgICBoYW5kbGVDYXB0dXJlcyhncmFtbWFyLCBsaW5lVGV4dCwgaXNGaXJzdExpbmUsIHdoaWxlUnVsZS5zdGFjaywgbGluZVRva2Vucywgd2hpbGVSdWxlLnJ1bGUud2hpbGVDYXB0dXJlcywgci5jYXB0dXJlSW5kaWNlcyk7XG4gICAgICAgIGxpbmVUb2tlbnMucHJvZHVjZSh3aGlsZVJ1bGUuc3RhY2ssIHIuY2FwdHVyZUluZGljZXNbMF0uZW5kKTtcbiAgICAgICAgYW5jaG9yUG9zaXRpb24gPSByLmNhcHR1cmVJbmRpY2VzWzBdLmVuZDtcbiAgICAgICAgaWYgKHIuY2FwdHVyZUluZGljZXNbMF0uZW5kID4gbGluZVBvcykge1xuICAgICAgICAgIGxpbmVQb3MgPSByLmNhcHR1cmVJbmRpY2VzWzBdLmVuZDtcbiAgICAgICAgICBpc0ZpcnN0TGluZSA9IGZhbHNlO1xuICAgICAgICB9XG4gICAgICB9XG4gICAgfSBlbHNlIHtcbiAgICAgIGlmIChmYWxzZSkge1xuICAgICAgICBjb25zb2xlLmxvZyhcIiAgcG9wcGluZyBcIiArIHdoaWxlUnVsZS5ydWxlLmRlYnVnTmFtZSArIFwiIC0gXCIgKyB3aGlsZVJ1bGUucnVsZS5kZWJ1Z1doaWxlUmVnRXhwKTtcbiAgICAgIH1cbiAgICAgIHN0YWNrID0gd2hpbGVSdWxlLnN0YWNrLnBvcCgpO1xuICAgICAgYnJlYWs7XG4gICAgfVxuICB9XG4gIHJldHVybiB7IHN0YWNrLCBsaW5lUG9zLCBhbmNob3JQb3NpdGlvbiwgaXNGaXJzdExpbmUgfTtcbn1cbmZ1bmN0aW9uIG1hdGNoUnVsZU9ySW5qZWN0aW9ucyhncmFtbWFyLCBsaW5lVGV4dCwgaXNGaXJzdExpbmUsIGxpbmVQb3MsIHN0YWNrLCBhbmNob3JQb3NpdGlvbikge1xuICBjb25zdCBtYXRjaFJlc3VsdCA9IG1hdGNoUnVsZShncmFtbWFyLCBsaW5lVGV4dCwgaXNGaXJzdExpbmUsIGxpbmVQb3MsIHN0YWNrLCBhbmNob3JQb3NpdGlvbik7XG4gIGNvbnN0IGluamVjdGlvbnMgPSBncmFtbWFyLmdldEluamVjdGlvbnMoKTtcbiAgaWYgKGluamVjdGlvbnMubGVuZ3RoID09PSAwKSB7XG4gICAgcmV0dXJuIG1hdGNoUmVzdWx0O1xuICB9XG4gIGNvbnN0IGluamVjdGlvblJlc3VsdCA9IG1hdGNoSW5qZWN0aW9ucyhpbmplY3Rpb25zLCBncmFtbWFyLCBsaW5lVGV4dCwgaXNGaXJzdExpbmUsIGxpbmVQb3MsIHN0YWNrLCBhbmNob3JQb3NpdGlvbik7XG4gIGlmICghaW5qZWN0aW9uUmVzdWx0KSB7XG4gICAgcmV0dXJuIG1hdGNoUmVzdWx0O1xuICB9XG4gIGlmICghbWF0Y2hSZXN1bHQpIHtcbiAgICByZXR1cm4gaW5qZWN0aW9uUmVzdWx0O1xuICB9XG4gIGNvbnN0IG1hdGNoUmVzdWx0U2NvcmUgPSBtYXRjaFJlc3VsdC5jYXB0dXJlSW5kaWNlc1swXS5zdGFydDtcbiAgY29uc3QgaW5qZWN0aW9uUmVzdWx0U2NvcmUgPSBpbmplY3Rpb25SZXN1bHQuY2FwdHVyZUluZGljZXNbMF0uc3RhcnQ7XG4gIGlmIChpbmplY3Rpb25SZXN1bHRTY29yZSA8IG1hdGNoUmVzdWx0U2NvcmUgfHwgaW5qZWN0aW9uUmVzdWx0LnByaW9yaXR5TWF0Y2ggJiYgaW5qZWN0aW9uUmVzdWx0U2NvcmUgPT09IG1hdGNoUmVzdWx0U2NvcmUpIHtcbiAgICByZXR1cm4gaW5qZWN0aW9uUmVzdWx0O1xuICB9XG4gIHJldHVybiBtYXRjaFJlc3VsdDtcbn1cbmZ1bmN0aW9uIG1hdGNoUnVsZShncmFtbWFyLCBsaW5lVGV4dCwgaXNGaXJzdExpbmUsIGxpbmVQb3MsIHN0YWNrLCBhbmNob3JQb3NpdGlvbikge1xuICBjb25zdCBydWxlID0gc3RhY2suZ2V0UnVsZShncmFtbWFyKTtcbiAgY29uc3QgeyBydWxlU2Nhbm5lciwgZmluZE9wdGlvbnMgfSA9IHByZXBhcmVSdWxlU2VhcmNoKHJ1bGUsIGdyYW1tYXIsIHN0YWNrLmVuZFJ1bGUsIGlzRmlyc3RMaW5lLCBsaW5lUG9zID09PSBhbmNob3JQb3NpdGlvbik7XG4gIGNvbnN0IHIgPSBydWxlU2Nhbm5lci5maW5kTmV4dE1hdGNoU3luYyhsaW5lVGV4dCwgbGluZVBvcywgZmluZE9wdGlvbnMpO1xuICBpZiAocikge1xuICAgIHJldHVybiB7XG4gICAgICBjYXB0dXJlSW5kaWNlczogci5jYXB0dXJlSW5kaWNlcyxcbiAgICAgIG1hdGNoZWRSdWxlSWQ6IHIucnVsZUlkXG4gICAgfTtcbiAgfVxuICByZXR1cm4gbnVsbDtcbn1cbmZ1bmN0aW9uIG1hdGNoSW5qZWN0aW9ucyhpbmplY3Rpb25zLCBncmFtbWFyLCBsaW5lVGV4dCwgaXNGaXJzdExpbmUsIGxpbmVQb3MsIHN0YWNrLCBhbmNob3JQb3NpdGlvbikge1xuICBsZXQgYmVzdE1hdGNoUmF0aW5nID0gTnVtYmVyLk1BWF9WQUxVRTtcbiAgbGV0IGJlc3RNYXRjaENhcHR1cmVJbmRpY2VzID0gbnVsbDtcbiAgbGV0IGJlc3RNYXRjaFJ1bGVJZDtcbiAgbGV0IGJlc3RNYXRjaFJlc3VsdFByaW9yaXR5ID0gMDtcbiAgY29uc3Qgc2NvcGVzID0gc3RhY2suY29udGVudE5hbWVTY29wZXNMaXN0LmdldFNjb3BlTmFtZXMoKTtcbiAgZm9yIChsZXQgaSA9IDAsIGxlbiA9IGluamVjdGlvbnMubGVuZ3RoOyBpIDwgbGVuOyBpKyspIHtcbiAgICBjb25zdCBpbmplY3Rpb24gPSBpbmplY3Rpb25zW2ldO1xuICAgIGlmICghaW5qZWN0aW9uLm1hdGNoZXIoc2NvcGVzKSkge1xuICAgICAgY29udGludWU7XG4gICAgfVxuICAgIGNvbnN0IHJ1bGUgPSBncmFtbWFyLmdldFJ1bGUoaW5qZWN0aW9uLnJ1bGVJZCk7XG4gICAgY29uc3QgeyBydWxlU2Nhbm5lciwgZmluZE9wdGlvbnMgfSA9IHByZXBhcmVSdWxlU2VhcmNoKHJ1bGUsIGdyYW1tYXIsIG51bGwsIGlzRmlyc3RMaW5lLCBsaW5lUG9zID09PSBhbmNob3JQb3NpdGlvbik7XG4gICAgY29uc3QgbWF0Y2hSZXN1bHQgPSBydWxlU2Nhbm5lci5maW5kTmV4dE1hdGNoU3luYyhsaW5lVGV4dCwgbGluZVBvcywgZmluZE9wdGlvbnMpO1xuICAgIGlmICghbWF0Y2hSZXN1bHQpIHtcbiAgICAgIGNvbnRpbnVlO1xuICAgIH1cbiAgICBpZiAoZmFsc2UpIHtcbiAgICAgIGNvbnNvbGUubG9nKGAgIG1hdGNoZWQgaW5qZWN0aW9uOiAke2luamVjdGlvbi5kZWJ1Z1NlbGVjdG9yfWApO1xuICAgICAgY29uc29sZS5sb2cocnVsZVNjYW5uZXIudG9TdHJpbmcoKSk7XG4gICAgfVxuICAgIGNvbnN0IG1hdGNoUmF0aW5nID0gbWF0Y2hSZXN1bHQuY2FwdHVyZUluZGljZXNbMF0uc3RhcnQ7XG4gICAgaWYgKG1hdGNoUmF0aW5nID49IGJlc3RNYXRjaFJhdGluZykge1xuICAgICAgY29udGludWU7XG4gICAgfVxuICAgIGJlc3RNYXRjaFJhdGluZyA9IG1hdGNoUmF0aW5nO1xuICAgIGJlc3RNYXRjaENhcHR1cmVJbmRpY2VzID0gbWF0Y2hSZXN1bHQuY2FwdHVyZUluZGljZXM7XG4gICAgYmVzdE1hdGNoUnVsZUlkID0gbWF0Y2hSZXN1bHQucnVsZUlkO1xuICAgIGJlc3RNYXRjaFJlc3VsdFByaW9yaXR5ID0gaW5qZWN0aW9uLnByaW9yaXR5O1xuICAgIGlmIChiZXN0TWF0Y2hSYXRpbmcgPT09IGxpbmVQb3MpIHtcbiAgICAgIGJyZWFrO1xuICAgIH1cbiAgfVxuICBpZiAoYmVzdE1hdGNoQ2FwdHVyZUluZGljZXMpIHtcbiAgICByZXR1cm4ge1xuICAgICAgcHJpb3JpdHlNYXRjaDogYmVzdE1hdGNoUmVzdWx0UHJpb3JpdHkgPT09IC0xLFxuICAgICAgY2FwdHVyZUluZGljZXM6IGJlc3RNYXRjaENhcHR1cmVJbmRpY2VzLFxuICAgICAgbWF0Y2hlZFJ1bGVJZDogYmVzdE1hdGNoUnVsZUlkXG4gICAgfTtcbiAgfVxuICByZXR1cm4gbnVsbDtcbn1cbmZ1bmN0aW9uIHByZXBhcmVSdWxlU2VhcmNoKHJ1bGUsIGdyYW1tYXIsIGVuZFJlZ2V4U291cmNlLCBhbGxvd0EsIGFsbG93Rykge1xuICBpZiAoVXNlT25pZ3VydW1hRmluZE9wdGlvbnMpIHtcbiAgICBjb25zdCBydWxlU2Nhbm5lcjIgPSBydWxlLmNvbXBpbGUoZ3JhbW1hciwgZW5kUmVnZXhTb3VyY2UpO1xuICAgIGNvbnN0IGZpbmRPcHRpb25zID0gZ2V0RmluZE9wdGlvbnMoYWxsb3dBLCBhbGxvd0cpO1xuICAgIHJldHVybiB7IHJ1bGVTY2FubmVyOiBydWxlU2Nhbm5lcjIsIGZpbmRPcHRpb25zIH07XG4gIH1cbiAgY29uc3QgcnVsZVNjYW5uZXIgPSBydWxlLmNvbXBpbGVBRyhncmFtbWFyLCBlbmRSZWdleFNvdXJjZSwgYWxsb3dBLCBhbGxvd0cpO1xuICByZXR1cm4geyBydWxlU2Nhbm5lciwgZmluZE9wdGlvbnM6IDAgLyogTm9uZSAqLyB9O1xufVxuZnVuY3Rpb24gcHJlcGFyZVJ1bGVXaGlsZVNlYXJjaChydWxlLCBncmFtbWFyLCBlbmRSZWdleFNvdXJjZSwgYWxsb3dBLCBhbGxvd0cpIHtcbiAgaWYgKFVzZU9uaWd1cnVtYUZpbmRPcHRpb25zKSB7XG4gICAgY29uc3QgcnVsZVNjYW5uZXIyID0gcnVsZS5jb21waWxlV2hpbGUoZ3JhbW1hciwgZW5kUmVnZXhTb3VyY2UpO1xuICAgIGNvbnN0IGZpbmRPcHRpb25zID0gZ2V0RmluZE9wdGlvbnMoYWxsb3dBLCBhbGxvd0cpO1xuICAgIHJldHVybiB7IHJ1bGVTY2FubmVyOiBydWxlU2Nhbm5lcjIsIGZpbmRPcHRpb25zIH07XG4gIH1cbiAgY29uc3QgcnVsZVNjYW5uZXIgPSBydWxlLmNvbXBpbGVXaGlsZUFHKGdyYW1tYXIsIGVuZFJlZ2V4U291cmNlLCBhbGxvd0EsIGFsbG93Ryk7XG4gIHJldHVybiB7IHJ1bGVTY2FubmVyLCBmaW5kT3B0aW9uczogMCAvKiBOb25lICovIH07XG59XG5mdW5jdGlvbiBnZXRGaW5kT3B0aW9ucyhhbGxvd0EsIGFsbG93Rykge1xuICBsZXQgb3B0aW9ucyA9IDAgLyogTm9uZSAqLztcbiAgaWYgKCFhbGxvd0EpIHtcbiAgICBvcHRpb25zIHw9IDEgLyogTm90QmVnaW5TdHJpbmcgKi87XG4gIH1cbiAgaWYgKCFhbGxvd0cpIHtcbiAgICBvcHRpb25zIHw9IDQgLyogTm90QmVnaW5Qb3NpdGlvbiAqLztcbiAgfVxuICByZXR1cm4gb3B0aW9ucztcbn1cbmZ1bmN0aW9uIGhhbmRsZUNhcHR1cmVzKGdyYW1tYXIsIGxpbmVUZXh0LCBpc0ZpcnN0TGluZSwgc3RhY2ssIGxpbmVUb2tlbnMsIGNhcHR1cmVzLCBjYXB0dXJlSW5kaWNlcykge1xuICBpZiAoY2FwdHVyZXMubGVuZ3RoID09PSAwKSB7XG4gICAgcmV0dXJuO1xuICB9XG4gIGNvbnN0IGxpbmVUZXh0Q29udGVudCA9IGxpbmVUZXh0LmNvbnRlbnQ7XG4gIGNvbnN0IGxlbiA9IE1hdGgubWluKGNhcHR1cmVzLmxlbmd0aCwgY2FwdHVyZUluZGljZXMubGVuZ3RoKTtcbiAgY29uc3QgbG9jYWxTdGFjayA9IFtdO1xuICBjb25zdCBtYXhFbmQgPSBjYXB0dXJlSW5kaWNlc1swXS5lbmQ7XG4gIGZvciAobGV0IGkgPSAwOyBpIDwgbGVuOyBpKyspIHtcbiAgICBjb25zdCBjYXB0dXJlUnVsZSA9IGNhcHR1cmVzW2ldO1xuICAgIGlmIChjYXB0dXJlUnVsZSA9PT0gbnVsbCkge1xuICAgICAgY29udGludWU7XG4gICAgfVxuICAgIGNvbnN0IGNhcHR1cmVJbmRleCA9IGNhcHR1cmVJbmRpY2VzW2ldO1xuICAgIGlmIChjYXB0dXJlSW5kZXgubGVuZ3RoID09PSAwKSB7XG4gICAgICBjb250aW51ZTtcbiAgICB9XG4gICAgaWYgKGNhcHR1cmVJbmRleC5zdGFydCA+IG1heEVuZCkge1xuICAgICAgYnJlYWs7XG4gICAgfVxuICAgIHdoaWxlIChsb2NhbFN0YWNrLmxlbmd0aCA+IDAgJiYgbG9jYWxTdGFja1tsb2NhbFN0YWNrLmxlbmd0aCAtIDFdLmVuZFBvcyA8PSBjYXB0dXJlSW5kZXguc3RhcnQpIHtcbiAgICAgIGxpbmVUb2tlbnMucHJvZHVjZUZyb21TY29wZXMobG9jYWxTdGFja1tsb2NhbFN0YWNrLmxlbmd0aCAtIDFdLnNjb3BlcywgbG9jYWxTdGFja1tsb2NhbFN0YWNrLmxlbmd0aCAtIDFdLmVuZFBvcyk7XG4gICAgICBsb2NhbFN0YWNrLnBvcCgpO1xuICAgIH1cbiAgICBpZiAobG9jYWxTdGFjay5sZW5ndGggPiAwKSB7XG4gICAgICBsaW5lVG9rZW5zLnByb2R1Y2VGcm9tU2NvcGVzKGxvY2FsU3RhY2tbbG9jYWxTdGFjay5sZW5ndGggLSAxXS5zY29wZXMsIGNhcHR1cmVJbmRleC5zdGFydCk7XG4gICAgfSBlbHNlIHtcbiAgICAgIGxpbmVUb2tlbnMucHJvZHVjZShzdGFjaywgY2FwdHVyZUluZGV4LnN0YXJ0KTtcbiAgICB9XG4gICAgaWYgKGNhcHR1cmVSdWxlLnJldG9rZW5pemVDYXB0dXJlZFdpdGhSdWxlSWQpIHtcbiAgICAgIGNvbnN0IHNjb3BlTmFtZSA9IGNhcHR1cmVSdWxlLmdldE5hbWUobGluZVRleHRDb250ZW50LCBjYXB0dXJlSW5kaWNlcyk7XG4gICAgICBjb25zdCBuYW1lU2NvcGVzTGlzdCA9IHN0YWNrLmNvbnRlbnROYW1lU2NvcGVzTGlzdC5wdXNoQXR0cmlidXRlZChzY29wZU5hbWUsIGdyYW1tYXIpO1xuICAgICAgY29uc3QgY29udGVudE5hbWUgPSBjYXB0dXJlUnVsZS5nZXRDb250ZW50TmFtZShsaW5lVGV4dENvbnRlbnQsIGNhcHR1cmVJbmRpY2VzKTtcbiAgICAgIGNvbnN0IGNvbnRlbnROYW1lU2NvcGVzTGlzdCA9IG5hbWVTY29wZXNMaXN0LnB1c2hBdHRyaWJ1dGVkKGNvbnRlbnROYW1lLCBncmFtbWFyKTtcbiAgICAgIGNvbnN0IHN0YWNrQ2xvbmUgPSBzdGFjay5wdXNoKGNhcHR1cmVSdWxlLnJldG9rZW5pemVDYXB0dXJlZFdpdGhSdWxlSWQsIGNhcHR1cmVJbmRleC5zdGFydCwgLTEsIGZhbHNlLCBudWxsLCBuYW1lU2NvcGVzTGlzdCwgY29udGVudE5hbWVTY29wZXNMaXN0KTtcbiAgICAgIGNvbnN0IG9uaWdTdWJTdHIgPSBncmFtbWFyLmNyZWF0ZU9uaWdTdHJpbmcobGluZVRleHRDb250ZW50LnN1YnN0cmluZygwLCBjYXB0dXJlSW5kZXguZW5kKSk7XG4gICAgICBfdG9rZW5pemVTdHJpbmcoXG4gICAgICAgIGdyYW1tYXIsXG4gICAgICAgIG9uaWdTdWJTdHIsXG4gICAgICAgIGlzRmlyc3RMaW5lICYmIGNhcHR1cmVJbmRleC5zdGFydCA9PT0gMCxcbiAgICAgICAgY2FwdHVyZUluZGV4LnN0YXJ0LFxuICAgICAgICBzdGFja0Nsb25lLFxuICAgICAgICBsaW5lVG9rZW5zLFxuICAgICAgICBmYWxzZSxcbiAgICAgICAgLyogbm8gdGltZSBsaW1pdCAqL1xuICAgICAgICAwXG4gICAgICApO1xuICAgICAgZGlzcG9zZU9uaWdTdHJpbmcob25pZ1N1YlN0cik7XG4gICAgICBjb250aW51ZTtcbiAgICB9XG4gICAgY29uc3QgY2FwdHVyZVJ1bGVTY29wZU5hbWUgPSBjYXB0dXJlUnVsZS5nZXROYW1lKGxpbmVUZXh0Q29udGVudCwgY2FwdHVyZUluZGljZXMpO1xuICAgIGlmIChjYXB0dXJlUnVsZVNjb3BlTmFtZSAhPT0gbnVsbCkge1xuICAgICAgY29uc3QgYmFzZSA9IGxvY2FsU3RhY2subGVuZ3RoID4gMCA/IGxvY2FsU3RhY2tbbG9jYWxTdGFjay5sZW5ndGggLSAxXS5zY29wZXMgOiBzdGFjay5jb250ZW50TmFtZVNjb3Blc0xpc3Q7XG4gICAgICBjb25zdCBjYXB0dXJlUnVsZVNjb3Blc0xpc3QgPSBiYXNlLnB1c2hBdHRyaWJ1dGVkKGNhcHR1cmVSdWxlU2NvcGVOYW1lLCBncmFtbWFyKTtcbiAgICAgIGxvY2FsU3RhY2sucHVzaChuZXcgTG9jYWxTdGFja0VsZW1lbnQoY2FwdHVyZVJ1bGVTY29wZXNMaXN0LCBjYXB0dXJlSW5kZXguZW5kKSk7XG4gICAgfVxuICB9XG4gIHdoaWxlIChsb2NhbFN0YWNrLmxlbmd0aCA+IDApIHtcbiAgICBsaW5lVG9rZW5zLnByb2R1Y2VGcm9tU2NvcGVzKGxvY2FsU3RhY2tbbG9jYWxTdGFjay5sZW5ndGggLSAxXS5zY29wZXMsIGxvY2FsU3RhY2tbbG9jYWxTdGFjay5sZW5ndGggLSAxXS5lbmRQb3MpO1xuICAgIGxvY2FsU3RhY2sucG9wKCk7XG4gIH1cbn1cbnZhciBMb2NhbFN0YWNrRWxlbWVudCA9IGNsYXNzIHtcbiAgc2NvcGVzO1xuICBlbmRQb3M7XG4gIGNvbnN0cnVjdG9yKHNjb3BlcywgZW5kUG9zKSB7XG4gICAgdGhpcy5zY29wZXMgPSBzY29wZXM7XG4gICAgdGhpcy5lbmRQb3MgPSBlbmRQb3M7XG4gIH1cbn07XG5cbi8vIHNyYy9ncmFtbWFyL2dyYW1tYXIudHNcbmZ1bmN0aW9uIGNyZWF0ZUdyYW1tYXIoc2NvcGVOYW1lLCBncmFtbWFyLCBpbml0aWFsTGFuZ3VhZ2UsIGVtYmVkZGVkTGFuZ3VhZ2VzLCB0b2tlblR5cGVzLCBiYWxhbmNlZEJyYWNrZXRTZWxlY3RvcnMsIGdyYW1tYXJSZXBvc2l0b3J5LCBvbmlnTGliKSB7XG4gIHJldHVybiBuZXcgR3JhbW1hcihcbiAgICBzY29wZU5hbWUsXG4gICAgZ3JhbW1hcixcbiAgICBpbml0aWFsTGFuZ3VhZ2UsXG4gICAgZW1iZWRkZWRMYW5ndWFnZXMsXG4gICAgdG9rZW5UeXBlcyxcbiAgICBiYWxhbmNlZEJyYWNrZXRTZWxlY3RvcnMsXG4gICAgZ3JhbW1hclJlcG9zaXRvcnksXG4gICAgb25pZ0xpYlxuICApO1xufVxuZnVuY3Rpb24gY29sbGVjdEluamVjdGlvbnMocmVzdWx0LCBzZWxlY3RvciwgcnVsZSwgcnVsZUZhY3RvcnlIZWxwZXIsIGdyYW1tYXIpIHtcbiAgY29uc3QgbWF0Y2hlcnMgPSBjcmVhdGVNYXRjaGVycyhzZWxlY3RvciwgbmFtZU1hdGNoZXIpO1xuICBjb25zdCBydWxlSWQgPSBSdWxlRmFjdG9yeS5nZXRDb21waWxlZFJ1bGVJZChydWxlLCBydWxlRmFjdG9yeUhlbHBlciwgZ3JhbW1hci5yZXBvc2l0b3J5KTtcbiAgZm9yIChjb25zdCBtYXRjaGVyIG9mIG1hdGNoZXJzKSB7XG4gICAgcmVzdWx0LnB1c2goe1xuICAgICAgZGVidWdTZWxlY3Rvcjogc2VsZWN0b3IsXG4gICAgICBtYXRjaGVyOiBtYXRjaGVyLm1hdGNoZXIsXG4gICAgICBydWxlSWQsXG4gICAgICBncmFtbWFyLFxuICAgICAgcHJpb3JpdHk6IG1hdGNoZXIucHJpb3JpdHlcbiAgICB9KTtcbiAgfVxufVxuZnVuY3Rpb24gbmFtZU1hdGNoZXIoaWRlbnRpZmVycywgc2NvcGVzKSB7XG4gIGlmIChzY29wZXMubGVuZ3RoIDwgaWRlbnRpZmVycy5sZW5ndGgpIHtcbiAgICByZXR1cm4gZmFsc2U7XG4gIH1cbiAgbGV0IGxhc3RJbmRleCA9IDA7XG4gIHJldHVybiBpZGVudGlmZXJzLmV2ZXJ5KChpZGVudGlmaWVyKSA9PiB7XG4gICAgZm9yIChsZXQgaSA9IGxhc3RJbmRleDsgaSA8IHNjb3Blcy5sZW5ndGg7IGkrKykge1xuICAgICAgaWYgKHNjb3Blc0FyZU1hdGNoaW5nKHNjb3Blc1tpXSwgaWRlbnRpZmllcikpIHtcbiAgICAgICAgbGFzdEluZGV4ID0gaSArIDE7XG4gICAgICAgIHJldHVybiB0cnVlO1xuICAgICAgfVxuICAgIH1cbiAgICByZXR1cm4gZmFsc2U7XG4gIH0pO1xufVxuZnVuY3Rpb24gc2NvcGVzQXJlTWF0Y2hpbmcodGhpc1Njb3BlTmFtZSwgc2NvcGVOYW1lKSB7XG4gIGlmICghdGhpc1Njb3BlTmFtZSkge1xuICAgIHJldHVybiBmYWxzZTtcbiAgfVxuICBpZiAodGhpc1Njb3BlTmFtZSA9PT0gc2NvcGVOYW1lKSB7XG4gICAgcmV0dXJuIHRydWU7XG4gIH1cbiAgY29uc3QgbGVuID0gc2NvcGVOYW1lLmxlbmd0aDtcbiAgcmV0dXJuIHRoaXNTY29wZU5hbWUubGVuZ3RoID4gbGVuICYmIHRoaXNTY29wZU5hbWUuc3Vic3RyKDAsIGxlbikgPT09IHNjb3BlTmFtZSAmJiB0aGlzU2NvcGVOYW1lW2xlbl0gPT09IFwiLlwiO1xufVxudmFyIEdyYW1tYXIgPSBjbGFzcyB7XG4gIGNvbnN0cnVjdG9yKF9yb290U2NvcGVOYW1lLCBncmFtbWFyLCBpbml0aWFsTGFuZ3VhZ2UsIGVtYmVkZGVkTGFuZ3VhZ2VzLCB0b2tlblR5cGVzLCBiYWxhbmNlZEJyYWNrZXRTZWxlY3RvcnMsIGdyYW1tYXJSZXBvc2l0b3J5LCBfb25pZ0xpYikge1xuICAgIHRoaXMuX3Jvb3RTY29wZU5hbWUgPSBfcm9vdFNjb3BlTmFtZTtcbiAgICB0aGlzLmJhbGFuY2VkQnJhY2tldFNlbGVjdG9ycyA9IGJhbGFuY2VkQnJhY2tldFNlbGVjdG9ycztcbiAgICB0aGlzLl9vbmlnTGliID0gX29uaWdMaWI7XG4gICAgdGhpcy5fYmFzaWNTY29wZUF0dHJpYnV0ZXNQcm92aWRlciA9IG5ldyBCYXNpY1Njb3BlQXR0cmlidXRlc1Byb3ZpZGVyKFxuICAgICAgaW5pdGlhbExhbmd1YWdlLFxuICAgICAgZW1iZWRkZWRMYW5ndWFnZXNcbiAgICApO1xuICAgIHRoaXMuX3Jvb3RJZCA9IC0xO1xuICAgIHRoaXMuX2xhc3RSdWxlSWQgPSAwO1xuICAgIHRoaXMuX3J1bGVJZDJkZXNjID0gW251bGxdO1xuICAgIHRoaXMuX2luY2x1ZGVkR3JhbW1hcnMgPSB7fTtcbiAgICB0aGlzLl9ncmFtbWFyUmVwb3NpdG9yeSA9IGdyYW1tYXJSZXBvc2l0b3J5O1xuICAgIHRoaXMuX2dyYW1tYXIgPSBpbml0R3JhbW1hcihncmFtbWFyLCBudWxsKTtcbiAgICB0aGlzLl9pbmplY3Rpb25zID0gbnVsbDtcbiAgICB0aGlzLl90b2tlblR5cGVNYXRjaGVycyA9IFtdO1xuICAgIGlmICh0b2tlblR5cGVzKSB7XG4gICAgICBmb3IgKGNvbnN0IHNlbGVjdG9yIG9mIE9iamVjdC5rZXlzKHRva2VuVHlwZXMpKSB7XG4gICAgICAgIGNvbnN0IG1hdGNoZXJzID0gY3JlYXRlTWF0Y2hlcnMoc2VsZWN0b3IsIG5hbWVNYXRjaGVyKTtcbiAgICAgICAgZm9yIChjb25zdCBtYXRjaGVyIG9mIG1hdGNoZXJzKSB7XG4gICAgICAgICAgdGhpcy5fdG9rZW5UeXBlTWF0Y2hlcnMucHVzaCh7XG4gICAgICAgICAgICBtYXRjaGVyOiBtYXRjaGVyLm1hdGNoZXIsXG4gICAgICAgICAgICB0eXBlOiB0b2tlblR5cGVzW3NlbGVjdG9yXVxuICAgICAgICAgIH0pO1xuICAgICAgICB9XG4gICAgICB9XG4gICAgfVxuICB9XG4gIF9yb290SWQ7XG4gIF9sYXN0UnVsZUlkO1xuICBfcnVsZUlkMmRlc2M7XG4gIF9pbmNsdWRlZEdyYW1tYXJzO1xuICBfZ3JhbW1hclJlcG9zaXRvcnk7XG4gIF9ncmFtbWFyO1xuICBfaW5qZWN0aW9ucztcbiAgX2Jhc2ljU2NvcGVBdHRyaWJ1dGVzUHJvdmlkZXI7XG4gIF90b2tlblR5cGVNYXRjaGVycztcbiAgZ2V0IHRoZW1lUHJvdmlkZXIoKSB7XG4gICAgcmV0dXJuIHRoaXMuX2dyYW1tYXJSZXBvc2l0b3J5O1xuICB9XG4gIGRpc3Bvc2UoKSB7XG4gICAgZm9yIChjb25zdCBydWxlIG9mIHRoaXMuX3J1bGVJZDJkZXNjKSB7XG4gICAgICBpZiAocnVsZSkge1xuICAgICAgICBydWxlLmRpc3Bvc2UoKTtcbiAgICAgIH1cbiAgICB9XG4gIH1cbiAgY3JlYXRlT25pZ1NjYW5uZXIoc291cmNlcykge1xuICAgIHJldHVybiB0aGlzLl9vbmlnTGliLmNyZWF0ZU9uaWdTY2FubmVyKHNvdXJjZXMpO1xuICB9XG4gIGNyZWF0ZU9uaWdTdHJpbmcoc291cmNlcykge1xuICAgIHJldHVybiB0aGlzLl9vbmlnTGliLmNyZWF0ZU9uaWdTdHJpbmcoc291cmNlcyk7XG4gIH1cbiAgZ2V0TWV0YWRhdGFGb3JTY29wZShzY29wZSkge1xuICAgIHJldHVybiB0aGlzLl9iYXNpY1Njb3BlQXR0cmlidXRlc1Byb3ZpZGVyLmdldEJhc2ljU2NvcGVBdHRyaWJ1dGVzKHNjb3BlKTtcbiAgfVxuICBfY29sbGVjdEluamVjdGlvbnMoKSB7XG4gICAgY29uc3QgZ3JhbW1hclJlcG9zaXRvcnkgPSB7XG4gICAgICBsb29rdXA6IChzY29wZU5hbWUyKSA9PiB7XG4gICAgICAgIGlmIChzY29wZU5hbWUyID09PSB0aGlzLl9yb290U2NvcGVOYW1lKSB7XG4gICAgICAgICAgcmV0dXJuIHRoaXMuX2dyYW1tYXI7XG4gICAgICAgIH1cbiAgICAgICAgcmV0dXJuIHRoaXMuZ2V0RXh0ZXJuYWxHcmFtbWFyKHNjb3BlTmFtZTIpO1xuICAgICAgfSxcbiAgICAgIGluamVjdGlvbnM6IChzY29wZU5hbWUyKSA9PiB7XG4gICAgICAgIHJldHVybiB0aGlzLl9ncmFtbWFyUmVwb3NpdG9yeS5pbmplY3Rpb25zKHNjb3BlTmFtZTIpO1xuICAgICAgfVxuICAgIH07XG4gICAgY29uc3QgcmVzdWx0ID0gW107XG4gICAgY29uc3Qgc2NvcGVOYW1lID0gdGhpcy5fcm9vdFNjb3BlTmFtZTtcbiAgICBjb25zdCBncmFtbWFyID0gZ3JhbW1hclJlcG9zaXRvcnkubG9va3VwKHNjb3BlTmFtZSk7XG4gICAgaWYgKGdyYW1tYXIpIHtcbiAgICAgIGNvbnN0IHJhd0luamVjdGlvbnMgPSBncmFtbWFyLmluamVjdGlvbnM7XG4gICAgICBpZiAocmF3SW5qZWN0aW9ucykge1xuICAgICAgICBmb3IgKGxldCBleHByZXNzaW9uIGluIHJhd0luamVjdGlvbnMpIHtcbiAgICAgICAgICBjb2xsZWN0SW5qZWN0aW9ucyhcbiAgICAgICAgICAgIHJlc3VsdCxcbiAgICAgICAgICAgIGV4cHJlc3Npb24sXG4gICAgICAgICAgICByYXdJbmplY3Rpb25zW2V4cHJlc3Npb25dLFxuICAgICAgICAgICAgdGhpcyxcbiAgICAgICAgICAgIGdyYW1tYXJcbiAgICAgICAgICApO1xuICAgICAgICB9XG4gICAgICB9XG4gICAgICBjb25zdCBpbmplY3Rpb25TY29wZU5hbWVzID0gdGhpcy5fZ3JhbW1hclJlcG9zaXRvcnkuaW5qZWN0aW9ucyhzY29wZU5hbWUpO1xuICAgICAgaWYgKGluamVjdGlvblNjb3BlTmFtZXMpIHtcbiAgICAgICAgaW5qZWN0aW9uU2NvcGVOYW1lcy5mb3JFYWNoKChpbmplY3Rpb25TY29wZU5hbWUpID0+IHtcbiAgICAgICAgICBjb25zdCBpbmplY3Rpb25HcmFtbWFyID0gdGhpcy5nZXRFeHRlcm5hbEdyYW1tYXIoaW5qZWN0aW9uU2NvcGVOYW1lKTtcbiAgICAgICAgICBpZiAoaW5qZWN0aW9uR3JhbW1hcikge1xuICAgICAgICAgICAgY29uc3Qgc2VsZWN0b3IgPSBpbmplY3Rpb25HcmFtbWFyLmluamVjdGlvblNlbGVjdG9yO1xuICAgICAgICAgICAgaWYgKHNlbGVjdG9yKSB7XG4gICAgICAgICAgICAgIGNvbGxlY3RJbmplY3Rpb25zKFxuICAgICAgICAgICAgICAgIHJlc3VsdCxcbiAgICAgICAgICAgICAgICBzZWxlY3RvcixcbiAgICAgICAgICAgICAgICBpbmplY3Rpb25HcmFtbWFyLFxuICAgICAgICAgICAgICAgIHRoaXMsXG4gICAgICAgICAgICAgICAgaW5qZWN0aW9uR3JhbW1hclxuICAgICAgICAgICAgICApO1xuICAgICAgICAgICAgfVxuICAgICAgICAgIH1cbiAgICAgICAgfSk7XG4gICAgICB9XG4gICAgfVxuICAgIHJlc3VsdC5zb3J0KChpMSwgaTIpID0+IGkxLnByaW9yaXR5IC0gaTIucHJpb3JpdHkpO1xuICAgIHJldHVybiByZXN1bHQ7XG4gIH1cbiAgZ2V0SW5qZWN0aW9ucygpIHtcbiAgICBpZiAodGhpcy5faW5qZWN0aW9ucyA9PT0gbnVsbCkge1xuICAgICAgdGhpcy5faW5qZWN0aW9ucyA9IHRoaXMuX2NvbGxlY3RJbmplY3Rpb25zKCk7XG4gICAgfVxuICAgIHJldHVybiB0aGlzLl9pbmplY3Rpb25zO1xuICB9XG4gIHJlZ2lzdGVyUnVsZShmYWN0b3J5KSB7XG4gICAgY29uc3QgaWQgPSArK3RoaXMuX2xhc3RSdWxlSWQ7XG4gICAgY29uc3QgcmVzdWx0ID0gZmFjdG9yeShydWxlSWRGcm9tTnVtYmVyKGlkKSk7XG4gICAgdGhpcy5fcnVsZUlkMmRlc2NbaWRdID0gcmVzdWx0O1xuICAgIHJldHVybiByZXN1bHQ7XG4gIH1cbiAgZ2V0UnVsZShydWxlSWQpIHtcbiAgICByZXR1cm4gdGhpcy5fcnVsZUlkMmRlc2NbcnVsZUlkVG9OdW1iZXIocnVsZUlkKV07XG4gIH1cbiAgZ2V0RXh0ZXJuYWxHcmFtbWFyKHNjb3BlTmFtZSwgcmVwb3NpdG9yeSkge1xuICAgIGlmICh0aGlzLl9pbmNsdWRlZEdyYW1tYXJzW3Njb3BlTmFtZV0pIHtcbiAgICAgIHJldHVybiB0aGlzLl9pbmNsdWRlZEdyYW1tYXJzW3Njb3BlTmFtZV07XG4gICAgfSBlbHNlIGlmICh0aGlzLl9ncmFtbWFyUmVwb3NpdG9yeSkge1xuICAgICAgY29uc3QgcmF3SW5jbHVkZWRHcmFtbWFyID0gdGhpcy5fZ3JhbW1hclJlcG9zaXRvcnkubG9va3VwKHNjb3BlTmFtZSk7XG4gICAgICBpZiAocmF3SW5jbHVkZWRHcmFtbWFyKSB7XG4gICAgICAgIHRoaXMuX2luY2x1ZGVkR3JhbW1hcnNbc2NvcGVOYW1lXSA9IGluaXRHcmFtbWFyKFxuICAgICAgICAgIHJhd0luY2x1ZGVkR3JhbW1hcixcbiAgICAgICAgICByZXBvc2l0b3J5ICYmIHJlcG9zaXRvcnkuJGJhc2VcbiAgICAgICAgKTtcbiAgICAgICAgcmV0dXJuIHRoaXMuX2luY2x1ZGVkR3JhbW1hcnNbc2NvcGVOYW1lXTtcbiAgICAgIH1cbiAgICB9XG4gICAgcmV0dXJuIHZvaWQgMDtcbiAgfVxuICB0b2tlbml6ZUxpbmUobGluZVRleHQsIHByZXZTdGF0ZSwgdGltZUxpbWl0ID0gMCkge1xuICAgIGNvbnN0IHIgPSB0aGlzLl90b2tlbml6ZShsaW5lVGV4dCwgcHJldlN0YXRlLCBmYWxzZSwgdGltZUxpbWl0KTtcbiAgICByZXR1cm4ge1xuICAgICAgdG9rZW5zOiByLmxpbmVUb2tlbnMuZ2V0UmVzdWx0KHIucnVsZVN0YWNrLCByLmxpbmVMZW5ndGgpLFxuICAgICAgcnVsZVN0YWNrOiByLnJ1bGVTdGFjayxcbiAgICAgIHN0b3BwZWRFYXJseTogci5zdG9wcGVkRWFybHlcbiAgICB9O1xuICB9XG4gIHRva2VuaXplTGluZTIobGluZVRleHQsIHByZXZTdGF0ZSwgdGltZUxpbWl0ID0gMCkge1xuICAgIGNvbnN0IHIgPSB0aGlzLl90b2tlbml6ZShsaW5lVGV4dCwgcHJldlN0YXRlLCB0cnVlLCB0aW1lTGltaXQpO1xuICAgIHJldHVybiB7XG4gICAgICB0b2tlbnM6IHIubGluZVRva2Vucy5nZXRCaW5hcnlSZXN1bHQoci5ydWxlU3RhY2ssIHIubGluZUxlbmd0aCksXG4gICAgICBydWxlU3RhY2s6IHIucnVsZVN0YWNrLFxuICAgICAgc3RvcHBlZEVhcmx5OiByLnN0b3BwZWRFYXJseVxuICAgIH07XG4gIH1cbiAgX3Rva2VuaXplKGxpbmVUZXh0LCBwcmV2U3RhdGUsIGVtaXRCaW5hcnlUb2tlbnMsIHRpbWVMaW1pdCkge1xuICAgIGlmICh0aGlzLl9yb290SWQgPT09IC0xKSB7XG4gICAgICB0aGlzLl9yb290SWQgPSBSdWxlRmFjdG9yeS5nZXRDb21waWxlZFJ1bGVJZChcbiAgICAgICAgdGhpcy5fZ3JhbW1hci5yZXBvc2l0b3J5LiRzZWxmLFxuICAgICAgICB0aGlzLFxuICAgICAgICB0aGlzLl9ncmFtbWFyLnJlcG9zaXRvcnlcbiAgICAgICk7XG4gICAgICB0aGlzLmdldEluamVjdGlvbnMoKTtcbiAgICB9XG4gICAgbGV0IGlzRmlyc3RMaW5lO1xuICAgIGlmICghcHJldlN0YXRlIHx8IHByZXZTdGF0ZSA9PT0gU3RhdGVTdGFja0ltcGwuTlVMTCkge1xuICAgICAgaXNGaXJzdExpbmUgPSB0cnVlO1xuICAgICAgY29uc3QgcmF3RGVmYXVsdE1ldGFkYXRhID0gdGhpcy5fYmFzaWNTY29wZUF0dHJpYnV0ZXNQcm92aWRlci5nZXREZWZhdWx0QXR0cmlidXRlcygpO1xuICAgICAgY29uc3QgZGVmYXVsdFN0eWxlID0gdGhpcy50aGVtZVByb3ZpZGVyLmdldERlZmF1bHRzKCk7XG4gICAgICBjb25zdCBkZWZhdWx0TWV0YWRhdGEgPSBFbmNvZGVkVG9rZW5NZXRhZGF0YS5zZXQoXG4gICAgICAgIDAsXG4gICAgICAgIHJhd0RlZmF1bHRNZXRhZGF0YS5sYW5ndWFnZUlkLFxuICAgICAgICByYXdEZWZhdWx0TWV0YWRhdGEudG9rZW5UeXBlLFxuICAgICAgICBudWxsLFxuICAgICAgICBkZWZhdWx0U3R5bGUuZm9udFN0eWxlLFxuICAgICAgICBkZWZhdWx0U3R5bGUuZm9yZWdyb3VuZElkLFxuICAgICAgICBkZWZhdWx0U3R5bGUuYmFja2dyb3VuZElkXG4gICAgICApO1xuICAgICAgY29uc3Qgcm9vdFNjb3BlTmFtZSA9IHRoaXMuZ2V0UnVsZSh0aGlzLl9yb290SWQpLmdldE5hbWUoXG4gICAgICAgIG51bGwsXG4gICAgICAgIG51bGxcbiAgICAgICk7XG4gICAgICBsZXQgc2NvcGVMaXN0O1xuICAgICAgaWYgKHJvb3RTY29wZU5hbWUpIHtcbiAgICAgICAgc2NvcGVMaXN0ID0gQXR0cmlidXRlZFNjb3BlU3RhY2suY3JlYXRlUm9vdEFuZExvb2tVcFNjb3BlTmFtZShcbiAgICAgICAgICByb290U2NvcGVOYW1lLFxuICAgICAgICAgIGRlZmF1bHRNZXRhZGF0YSxcbiAgICAgICAgICB0aGlzXG4gICAgICAgICk7XG4gICAgICB9IGVsc2Uge1xuICAgICAgICBzY29wZUxpc3QgPSBBdHRyaWJ1dGVkU2NvcGVTdGFjay5jcmVhdGVSb290KFxuICAgICAgICAgIFwidW5rbm93blwiLFxuICAgICAgICAgIGRlZmF1bHRNZXRhZGF0YVxuICAgICAgICApO1xuICAgICAgfVxuICAgICAgcHJldlN0YXRlID0gbmV3IFN0YXRlU3RhY2tJbXBsKFxuICAgICAgICBudWxsLFxuICAgICAgICB0aGlzLl9yb290SWQsXG4gICAgICAgIC0xLFxuICAgICAgICAtMSxcbiAgICAgICAgZmFsc2UsXG4gICAgICAgIG51bGwsXG4gICAgICAgIHNjb3BlTGlzdCxcbiAgICAgICAgc2NvcGVMaXN0XG4gICAgICApO1xuICAgIH0gZWxzZSB7XG4gICAgICBpc0ZpcnN0TGluZSA9IGZhbHNlO1xuICAgICAgcHJldlN0YXRlLnJlc2V0KCk7XG4gICAgfVxuICAgIGxpbmVUZXh0ID0gbGluZVRleHQgKyBcIlxcblwiO1xuICAgIGNvbnN0IG9uaWdMaW5lVGV4dCA9IHRoaXMuY3JlYXRlT25pZ1N0cmluZyhsaW5lVGV4dCk7XG4gICAgY29uc3QgbGluZUxlbmd0aCA9IG9uaWdMaW5lVGV4dC5jb250ZW50Lmxlbmd0aDtcbiAgICBjb25zdCBsaW5lVG9rZW5zID0gbmV3IExpbmVUb2tlbnMoXG4gICAgICBlbWl0QmluYXJ5VG9rZW5zLFxuICAgICAgbGluZVRleHQsXG4gICAgICB0aGlzLl90b2tlblR5cGVNYXRjaGVycyxcbiAgICAgIHRoaXMuYmFsYW5jZWRCcmFja2V0U2VsZWN0b3JzXG4gICAgKTtcbiAgICBjb25zdCByID0gX3Rva2VuaXplU3RyaW5nKFxuICAgICAgdGhpcyxcbiAgICAgIG9uaWdMaW5lVGV4dCxcbiAgICAgIGlzRmlyc3RMaW5lLFxuICAgICAgMCxcbiAgICAgIHByZXZTdGF0ZSxcbiAgICAgIGxpbmVUb2tlbnMsXG4gICAgICB0cnVlLFxuICAgICAgdGltZUxpbWl0XG4gICAgKTtcbiAgICBkaXNwb3NlT25pZ1N0cmluZyhvbmlnTGluZVRleHQpO1xuICAgIHJldHVybiB7XG4gICAgICBsaW5lTGVuZ3RoLFxuICAgICAgbGluZVRva2VucyxcbiAgICAgIHJ1bGVTdGFjazogci5zdGFjayxcbiAgICAgIHN0b3BwZWRFYXJseTogci5zdG9wcGVkRWFybHlcbiAgICB9O1xuICB9XG59O1xuZnVuY3Rpb24gaW5pdEdyYW1tYXIoZ3JhbW1hciwgYmFzZSkge1xuICBncmFtbWFyID0gY2xvbmUoZ3JhbW1hcik7XG4gIGdyYW1tYXIucmVwb3NpdG9yeSA9IGdyYW1tYXIucmVwb3NpdG9yeSB8fCB7fTtcbiAgZ3JhbW1hci5yZXBvc2l0b3J5LiRzZWxmID0ge1xuICAgICR2c2NvZGVUZXh0bWF0ZUxvY2F0aW9uOiBncmFtbWFyLiR2c2NvZGVUZXh0bWF0ZUxvY2F0aW9uLFxuICAgIHBhdHRlcm5zOiBncmFtbWFyLnBhdHRlcm5zLFxuICAgIG5hbWU6IGdyYW1tYXIuc2NvcGVOYW1lXG4gIH07XG4gIGdyYW1tYXIucmVwb3NpdG9yeS4kYmFzZSA9IGJhc2UgfHwgZ3JhbW1hci5yZXBvc2l0b3J5LiRzZWxmO1xuICByZXR1cm4gZ3JhbW1hcjtcbn1cbnZhciBBdHRyaWJ1dGVkU2NvcGVTdGFjayA9IGNsYXNzIF9BdHRyaWJ1dGVkU2NvcGVTdGFjayB7XG4gIC8qKlxuICAgKiBJbnZhcmlhbnQ6XG4gICAqIGBgYFxuICAgKiBpZiAocGFyZW50ICYmICFzY29wZVBhdGguZXh0ZW5kcyhwYXJlbnQuc2NvcGVQYXRoKSkge1xuICAgKiBcdHRocm93IG5ldyBFcnJvcigpO1xuICAgKiB9XG4gICAqIGBgYFxuICAgKi9cbiAgY29uc3RydWN0b3IocGFyZW50LCBzY29wZVBhdGgsIHRva2VuQXR0cmlidXRlcykge1xuICAgIHRoaXMucGFyZW50ID0gcGFyZW50O1xuICAgIHRoaXMuc2NvcGVQYXRoID0gc2NvcGVQYXRoO1xuICAgIHRoaXMudG9rZW5BdHRyaWJ1dGVzID0gdG9rZW5BdHRyaWJ1dGVzO1xuICB9XG4gIHN0YXRpYyBmcm9tRXh0ZW5zaW9uKG5hbWVzU2NvcGVMaXN0LCBjb250ZW50TmFtZVNjb3Blc0xpc3QpIHtcbiAgICBsZXQgY3VycmVudCA9IG5hbWVzU2NvcGVMaXN0O1xuICAgIGxldCBzY29wZU5hbWVzID0gbmFtZXNTY29wZUxpc3Q/LnNjb3BlUGF0aCA/PyBudWxsO1xuICAgIGZvciAoY29uc3QgZnJhbWUgb2YgY29udGVudE5hbWVTY29wZXNMaXN0KSB7XG4gICAgICBzY29wZU5hbWVzID0gU2NvcGVTdGFjay5wdXNoKHNjb3BlTmFtZXMsIGZyYW1lLnNjb3BlTmFtZXMpO1xuICAgICAgY3VycmVudCA9IG5ldyBfQXR0cmlidXRlZFNjb3BlU3RhY2soY3VycmVudCwgc2NvcGVOYW1lcywgZnJhbWUuZW5jb2RlZFRva2VuQXR0cmlidXRlcyk7XG4gICAgfVxuICAgIHJldHVybiBjdXJyZW50O1xuICB9XG4gIHN0YXRpYyBjcmVhdGVSb290KHNjb3BlTmFtZSwgdG9rZW5BdHRyaWJ1dGVzKSB7XG4gICAgcmV0dXJuIG5ldyBfQXR0cmlidXRlZFNjb3BlU3RhY2sobnVsbCwgbmV3IFNjb3BlU3RhY2sobnVsbCwgc2NvcGVOYW1lKSwgdG9rZW5BdHRyaWJ1dGVzKTtcbiAgfVxuICBzdGF0aWMgY3JlYXRlUm9vdEFuZExvb2tVcFNjb3BlTmFtZShzY29wZU5hbWUsIHRva2VuQXR0cmlidXRlcywgZ3JhbW1hcikge1xuICAgIGNvbnN0IHJhd1Jvb3RNZXRhZGF0YSA9IGdyYW1tYXIuZ2V0TWV0YWRhdGFGb3JTY29wZShzY29wZU5hbWUpO1xuICAgIGNvbnN0IHNjb3BlUGF0aCA9IG5ldyBTY29wZVN0YWNrKG51bGwsIHNjb3BlTmFtZSk7XG4gICAgY29uc3Qgcm9vdFN0eWxlID0gZ3JhbW1hci50aGVtZVByb3ZpZGVyLnRoZW1lTWF0Y2goc2NvcGVQYXRoKTtcbiAgICBjb25zdCByZXNvbHZlZFRva2VuQXR0cmlidXRlcyA9IF9BdHRyaWJ1dGVkU2NvcGVTdGFjay5tZXJnZUF0dHJpYnV0ZXMoXG4gICAgICB0b2tlbkF0dHJpYnV0ZXMsXG4gICAgICByYXdSb290TWV0YWRhdGEsXG4gICAgICByb290U3R5bGVcbiAgICApO1xuICAgIHJldHVybiBuZXcgX0F0dHJpYnV0ZWRTY29wZVN0YWNrKG51bGwsIHNjb3BlUGF0aCwgcmVzb2x2ZWRUb2tlbkF0dHJpYnV0ZXMpO1xuICB9XG4gIGdldCBzY29wZU5hbWUoKSB7XG4gICAgcmV0dXJuIHRoaXMuc2NvcGVQYXRoLnNjb3BlTmFtZTtcbiAgfVxuICB0b1N0cmluZygpIHtcbiAgICByZXR1cm4gdGhpcy5nZXRTY29wZU5hbWVzKCkuam9pbihcIiBcIik7XG4gIH1cbiAgZXF1YWxzKG90aGVyKSB7XG4gICAgcmV0dXJuIF9BdHRyaWJ1dGVkU2NvcGVTdGFjay5lcXVhbHModGhpcywgb3RoZXIpO1xuICB9XG4gIHN0YXRpYyBlcXVhbHMoYSwgYikge1xuICAgIGRvIHtcbiAgICAgIGlmIChhID09PSBiKSB7XG4gICAgICAgIHJldHVybiB0cnVlO1xuICAgICAgfVxuICAgICAgaWYgKCFhICYmICFiKSB7XG4gICAgICAgIHJldHVybiB0cnVlO1xuICAgICAgfVxuICAgICAgaWYgKCFhIHx8ICFiKSB7XG4gICAgICAgIHJldHVybiBmYWxzZTtcbiAgICAgIH1cbiAgICAgIGlmIChhLnNjb3BlTmFtZSAhPT0gYi5zY29wZU5hbWUgfHwgYS50b2tlbkF0dHJpYnV0ZXMgIT09IGIudG9rZW5BdHRyaWJ1dGVzKSB7XG4gICAgICAgIHJldHVybiBmYWxzZTtcbiAgICAgIH1cbiAgICAgIGEgPSBhLnBhcmVudDtcbiAgICAgIGIgPSBiLnBhcmVudDtcbiAgICB9IHdoaWxlICh0cnVlKTtcbiAgfVxuICBzdGF0aWMgbWVyZ2VBdHRyaWJ1dGVzKGV4aXN0aW5nVG9rZW5BdHRyaWJ1dGVzLCBiYXNpY1Njb3BlQXR0cmlidXRlcywgc3R5bGVBdHRyaWJ1dGVzKSB7XG4gICAgbGV0IGZvbnRTdHlsZSA9IC0xIC8qIE5vdFNldCAqLztcbiAgICBsZXQgZm9yZWdyb3VuZCA9IDA7XG4gICAgbGV0IGJhY2tncm91bmQgPSAwO1xuICAgIGlmIChzdHlsZUF0dHJpYnV0ZXMgIT09IG51bGwpIHtcbiAgICAgIGZvbnRTdHlsZSA9IHN0eWxlQXR0cmlidXRlcy5mb250U3R5bGU7XG4gICAgICBmb3JlZ3JvdW5kID0gc3R5bGVBdHRyaWJ1dGVzLmZvcmVncm91bmRJZDtcbiAgICAgIGJhY2tncm91bmQgPSBzdHlsZUF0dHJpYnV0ZXMuYmFja2dyb3VuZElkO1xuICAgIH1cbiAgICByZXR1cm4gRW5jb2RlZFRva2VuTWV0YWRhdGEuc2V0KFxuICAgICAgZXhpc3RpbmdUb2tlbkF0dHJpYnV0ZXMsXG4gICAgICBiYXNpY1Njb3BlQXR0cmlidXRlcy5sYW5ndWFnZUlkLFxuICAgICAgYmFzaWNTY29wZUF0dHJpYnV0ZXMudG9rZW5UeXBlLFxuICAgICAgbnVsbCxcbiAgICAgIGZvbnRTdHlsZSxcbiAgICAgIGZvcmVncm91bmQsXG4gICAgICBiYWNrZ3JvdW5kXG4gICAgKTtcbiAgfVxuICBwdXNoQXR0cmlidXRlZChzY29wZVBhdGgsIGdyYW1tYXIpIHtcbiAgICBpZiAoc2NvcGVQYXRoID09PSBudWxsKSB7XG4gICAgICByZXR1cm4gdGhpcztcbiAgICB9XG4gICAgaWYgKHNjb3BlUGF0aC5pbmRleE9mKFwiIFwiKSA9PT0gLTEpIHtcbiAgICAgIHJldHVybiBfQXR0cmlidXRlZFNjb3BlU3RhY2suX3B1c2hBdHRyaWJ1dGVkKHRoaXMsIHNjb3BlUGF0aCwgZ3JhbW1hcik7XG4gICAgfVxuICAgIGNvbnN0IHNjb3BlcyA9IHNjb3BlUGF0aC5zcGxpdCgvIC9nKTtcbiAgICBsZXQgcmVzdWx0ID0gdGhpcztcbiAgICBmb3IgKGNvbnN0IHNjb3BlIG9mIHNjb3Blcykge1xuICAgICAgcmVzdWx0ID0gX0F0dHJpYnV0ZWRTY29wZVN0YWNrLl9wdXNoQXR0cmlidXRlZChyZXN1bHQsIHNjb3BlLCBncmFtbWFyKTtcbiAgICB9XG4gICAgcmV0dXJuIHJlc3VsdDtcbiAgfVxuICBzdGF0aWMgX3B1c2hBdHRyaWJ1dGVkKHRhcmdldCwgc2NvcGVOYW1lLCBncmFtbWFyKSB7XG4gICAgY29uc3QgcmF3TWV0YWRhdGEgPSBncmFtbWFyLmdldE1ldGFkYXRhRm9yU2NvcGUoc2NvcGVOYW1lKTtcbiAgICBjb25zdCBuZXdQYXRoID0gdGFyZ2V0LnNjb3BlUGF0aC5wdXNoKHNjb3BlTmFtZSk7XG4gICAgY29uc3Qgc2NvcGVUaGVtZU1hdGNoUmVzdWx0ID0gZ3JhbW1hci50aGVtZVByb3ZpZGVyLnRoZW1lTWF0Y2gobmV3UGF0aCk7XG4gICAgY29uc3QgbWV0YWRhdGEgPSBfQXR0cmlidXRlZFNjb3BlU3RhY2subWVyZ2VBdHRyaWJ1dGVzKFxuICAgICAgdGFyZ2V0LnRva2VuQXR0cmlidXRlcyxcbiAgICAgIHJhd01ldGFkYXRhLFxuICAgICAgc2NvcGVUaGVtZU1hdGNoUmVzdWx0XG4gICAgKTtcbiAgICByZXR1cm4gbmV3IF9BdHRyaWJ1dGVkU2NvcGVTdGFjayh0YXJnZXQsIG5ld1BhdGgsIG1ldGFkYXRhKTtcbiAgfVxuICBnZXRTY29wZU5hbWVzKCkge1xuICAgIHJldHVybiB0aGlzLnNjb3BlUGF0aC5nZXRTZWdtZW50cygpO1xuICB9XG4gIGdldEV4dGVuc2lvbklmRGVmaW5lZChiYXNlKSB7XG4gICAgY29uc3QgcmVzdWx0ID0gW107XG4gICAgbGV0IHNlbGYgPSB0aGlzO1xuICAgIHdoaWxlIChzZWxmICYmIHNlbGYgIT09IGJhc2UpIHtcbiAgICAgIHJlc3VsdC5wdXNoKHtcbiAgICAgICAgZW5jb2RlZFRva2VuQXR0cmlidXRlczogc2VsZi50b2tlbkF0dHJpYnV0ZXMsXG4gICAgICAgIHNjb3BlTmFtZXM6IHNlbGYuc2NvcGVQYXRoLmdldEV4dGVuc2lvbklmRGVmaW5lZChzZWxmLnBhcmVudD8uc2NvcGVQYXRoID8/IG51bGwpXG4gICAgICB9KTtcbiAgICAgIHNlbGYgPSBzZWxmLnBhcmVudDtcbiAgICB9XG4gICAgcmV0dXJuIHNlbGYgPT09IGJhc2UgPyByZXN1bHQucmV2ZXJzZSgpIDogdm9pZCAwO1xuICB9XG59O1xudmFyIFN0YXRlU3RhY2tJbXBsID0gY2xhc3MgX1N0YXRlU3RhY2tJbXBsIHtcbiAgLyoqXG4gICAqIEludmFyaWFudDpcbiAgICogYGBgXG4gICAqIGlmIChjb250ZW50TmFtZVNjb3Blc0xpc3QgIT09IG5hbWVTY29wZXNMaXN0ICYmIGNvbnRlbnROYW1lU2NvcGVzTGlzdD8ucGFyZW50ICE9PSBuYW1lU2NvcGVzTGlzdCkge1xuICAgKiBcdHRocm93IG5ldyBFcnJvcigpO1xuICAgKiB9XG4gICAqIGlmICh0aGlzLnBhcmVudCAmJiAhbmFtZVNjb3Blc0xpc3QuZXh0ZW5kcyh0aGlzLnBhcmVudC5jb250ZW50TmFtZVNjb3Blc0xpc3QpKSB7XG4gICAqIFx0dGhyb3cgbmV3IEVycm9yKCk7XG4gICAqIH1cbiAgICogYGBgXG4gICAqL1xuICBjb25zdHJ1Y3RvcihwYXJlbnQsIHJ1bGVJZCwgZW50ZXJQb3MsIGFuY2hvclBvcywgYmVnaW5SdWxlQ2FwdHVyZWRFT0wsIGVuZFJ1bGUsIG5hbWVTY29wZXNMaXN0LCBjb250ZW50TmFtZVNjb3Blc0xpc3QpIHtcbiAgICB0aGlzLnBhcmVudCA9IHBhcmVudDtcbiAgICB0aGlzLnJ1bGVJZCA9IHJ1bGVJZDtcbiAgICB0aGlzLmJlZ2luUnVsZUNhcHR1cmVkRU9MID0gYmVnaW5SdWxlQ2FwdHVyZWRFT0w7XG4gICAgdGhpcy5lbmRSdWxlID0gZW5kUnVsZTtcbiAgICB0aGlzLm5hbWVTY29wZXNMaXN0ID0gbmFtZVNjb3Blc0xpc3Q7XG4gICAgdGhpcy5jb250ZW50TmFtZVNjb3Blc0xpc3QgPSBjb250ZW50TmFtZVNjb3Blc0xpc3Q7XG4gICAgdGhpcy5kZXB0aCA9IHRoaXMucGFyZW50ID8gdGhpcy5wYXJlbnQuZGVwdGggKyAxIDogMTtcbiAgICB0aGlzLl9lbnRlclBvcyA9IGVudGVyUG9zO1xuICAgIHRoaXMuX2FuY2hvclBvcyA9IGFuY2hvclBvcztcbiAgfVxuICBfc3RhY2tFbGVtZW50QnJhbmQgPSB2b2lkIDA7XG4gIC8vIFRPRE8gcmVtb3ZlIG1lXG4gIHN0YXRpYyBOVUxMID0gbmV3IF9TdGF0ZVN0YWNrSW1wbChcbiAgICBudWxsLFxuICAgIDAsXG4gICAgMCxcbiAgICAwLFxuICAgIGZhbHNlLFxuICAgIG51bGwsXG4gICAgbnVsbCxcbiAgICBudWxsXG4gICk7XG4gIC8qKlxuICAgKiBUaGUgcG9zaXRpb24gb24gdGhlIGN1cnJlbnQgbGluZSB3aGVyZSB0aGlzIHN0YXRlIHdhcyBwdXNoZWQuXG4gICAqIFRoaXMgaXMgcmVsZXZhbnQgb25seSB3aGlsZSB0b2tlbml6aW5nIGEgbGluZSwgdG8gZGV0ZWN0IGVuZGxlc3MgbG9vcHMuXG4gICAqIEl0cyB2YWx1ZSBpcyBtZWFuaW5nbGVzcyBhY3Jvc3MgbGluZXMuXG4gICAqL1xuICBfZW50ZXJQb3M7XG4gIC8qKlxuICAgKiBUaGUgY2FwdHVyZWQgYW5jaG9yIHBvc2l0aW9uIHdoZW4gdGhpcyBzdGFjayBlbGVtZW50IHdhcyBwdXNoZWQuXG4gICAqIFRoaXMgaXMgcmVsZXZhbnQgb25seSB3aGlsZSB0b2tlbml6aW5nIGEgbGluZSwgdG8gcmVzdG9yZSB0aGUgYW5jaG9yIHBvc2l0aW9uIHdoZW4gcG9wcGluZy5cbiAgICogSXRzIHZhbHVlIGlzIG1lYW5pbmdsZXNzIGFjcm9zcyBsaW5lcy5cbiAgICovXG4gIF9hbmNob3JQb3M7XG4gIC8qKlxuICAgKiBUaGUgZGVwdGggb2YgdGhlIHN0YWNrLlxuICAgKi9cbiAgZGVwdGg7XG4gIGVxdWFscyhvdGhlcikge1xuICAgIGlmIChvdGhlciA9PT0gbnVsbCkge1xuICAgICAgcmV0dXJuIGZhbHNlO1xuICAgIH1cbiAgICByZXR1cm4gX1N0YXRlU3RhY2tJbXBsLl9lcXVhbHModGhpcywgb3RoZXIpO1xuICB9XG4gIHN0YXRpYyBfZXF1YWxzKGEsIGIpIHtcbiAgICBpZiAoYSA9PT0gYikge1xuICAgICAgcmV0dXJuIHRydWU7XG4gICAgfVxuICAgIGlmICghdGhpcy5fc3RydWN0dXJhbEVxdWFscyhhLCBiKSkge1xuICAgICAgcmV0dXJuIGZhbHNlO1xuICAgIH1cbiAgICByZXR1cm4gQXR0cmlidXRlZFNjb3BlU3RhY2suZXF1YWxzKGEuY29udGVudE5hbWVTY29wZXNMaXN0LCBiLmNvbnRlbnROYW1lU2NvcGVzTGlzdCk7XG4gIH1cbiAgLyoqXG4gICAqIEEgc3RydWN0dXJhbCBlcXVhbHMgY2hlY2suIERvZXMgbm90IHRha2UgaW50byBhY2NvdW50IGBzY29wZXNgLlxuICAgKi9cbiAgc3RhdGljIF9zdHJ1Y3R1cmFsRXF1YWxzKGEsIGIpIHtcbiAgICBkbyB7XG4gICAgICBpZiAoYSA9PT0gYikge1xuICAgICAgICByZXR1cm4gdHJ1ZTtcbiAgICAgIH1cbiAgICAgIGlmICghYSAmJiAhYikge1xuICAgICAgICByZXR1cm4gdHJ1ZTtcbiAgICAgIH1cbiAgICAgIGlmICghYSB8fCAhYikge1xuICAgICAgICByZXR1cm4gZmFsc2U7XG4gICAgICB9XG4gICAgICBpZiAoYS5kZXB0aCAhPT0gYi5kZXB0aCB8fCBhLnJ1bGVJZCAhPT0gYi5ydWxlSWQgfHwgYS5lbmRSdWxlICE9PSBiLmVuZFJ1bGUpIHtcbiAgICAgICAgcmV0dXJuIGZhbHNlO1xuICAgICAgfVxuICAgICAgYSA9IGEucGFyZW50O1xuICAgICAgYiA9IGIucGFyZW50O1xuICAgIH0gd2hpbGUgKHRydWUpO1xuICB9XG4gIGNsb25lKCkge1xuICAgIHJldHVybiB0aGlzO1xuICB9XG4gIHN0YXRpYyBfcmVzZXQoZWwpIHtcbiAgICB3aGlsZSAoZWwpIHtcbiAgICAgIGVsLl9lbnRlclBvcyA9IC0xO1xuICAgICAgZWwuX2FuY2hvclBvcyA9IC0xO1xuICAgICAgZWwgPSBlbC5wYXJlbnQ7XG4gICAgfVxuICB9XG4gIHJlc2V0KCkge1xuICAgIF9TdGF0ZVN0YWNrSW1wbC5fcmVzZXQodGhpcyk7XG4gIH1cbiAgcG9wKCkge1xuICAgIHJldHVybiB0aGlzLnBhcmVudDtcbiAgfVxuICBzYWZlUG9wKCkge1xuICAgIGlmICh0aGlzLnBhcmVudCkge1xuICAgICAgcmV0dXJuIHRoaXMucGFyZW50O1xuICAgIH1cbiAgICByZXR1cm4gdGhpcztcbiAgfVxuICBwdXNoKHJ1bGVJZCwgZW50ZXJQb3MsIGFuY2hvclBvcywgYmVnaW5SdWxlQ2FwdHVyZWRFT0wsIGVuZFJ1bGUsIG5hbWVTY29wZXNMaXN0LCBjb250ZW50TmFtZVNjb3Blc0xpc3QpIHtcbiAgICByZXR1cm4gbmV3IF9TdGF0ZVN0YWNrSW1wbChcbiAgICAgIHRoaXMsXG4gICAgICBydWxlSWQsXG4gICAgICBlbnRlclBvcyxcbiAgICAgIGFuY2hvclBvcyxcbiAgICAgIGJlZ2luUnVsZUNhcHR1cmVkRU9MLFxuICAgICAgZW5kUnVsZSxcbiAgICAgIG5hbWVTY29wZXNMaXN0LFxuICAgICAgY29udGVudE5hbWVTY29wZXNMaXN0XG4gICAgKTtcbiAgfVxuICBnZXRFbnRlclBvcygpIHtcbiAgICByZXR1cm4gdGhpcy5fZW50ZXJQb3M7XG4gIH1cbiAgZ2V0QW5jaG9yUG9zKCkge1xuICAgIHJldHVybiB0aGlzLl9hbmNob3JQb3M7XG4gIH1cbiAgZ2V0UnVsZShncmFtbWFyKSB7XG4gICAgcmV0dXJuIGdyYW1tYXIuZ2V0UnVsZSh0aGlzLnJ1bGVJZCk7XG4gIH1cbiAgdG9TdHJpbmcoKSB7XG4gICAgY29uc3QgciA9IFtdO1xuICAgIHRoaXMuX3dyaXRlU3RyaW5nKHIsIDApO1xuICAgIHJldHVybiBcIltcIiArIHIuam9pbihcIixcIikgKyBcIl1cIjtcbiAgfVxuICBfd3JpdGVTdHJpbmcocmVzLCBvdXRJbmRleCkge1xuICAgIGlmICh0aGlzLnBhcmVudCkge1xuICAgICAgb3V0SW5kZXggPSB0aGlzLnBhcmVudC5fd3JpdGVTdHJpbmcocmVzLCBvdXRJbmRleCk7XG4gICAgfVxuICAgIHJlc1tvdXRJbmRleCsrXSA9IGAoJHt0aGlzLnJ1bGVJZH0sICR7dGhpcy5uYW1lU2NvcGVzTGlzdD8udG9TdHJpbmcoKX0sICR7dGhpcy5jb250ZW50TmFtZVNjb3Blc0xpc3Q/LnRvU3RyaW5nKCl9KWA7XG4gICAgcmV0dXJuIG91dEluZGV4O1xuICB9XG4gIHdpdGhDb250ZW50TmFtZVNjb3Blc0xpc3QoY29udGVudE5hbWVTY29wZVN0YWNrKSB7XG4gICAgaWYgKHRoaXMuY29udGVudE5hbWVTY29wZXNMaXN0ID09PSBjb250ZW50TmFtZVNjb3BlU3RhY2spIHtcbiAgICAgIHJldHVybiB0aGlzO1xuICAgIH1cbiAgICByZXR1cm4gdGhpcy5wYXJlbnQucHVzaChcbiAgICAgIHRoaXMucnVsZUlkLFxuICAgICAgdGhpcy5fZW50ZXJQb3MsXG4gICAgICB0aGlzLl9hbmNob3JQb3MsXG4gICAgICB0aGlzLmJlZ2luUnVsZUNhcHR1cmVkRU9MLFxuICAgICAgdGhpcy5lbmRSdWxlLFxuICAgICAgdGhpcy5uYW1lU2NvcGVzTGlzdCxcbiAgICAgIGNvbnRlbnROYW1lU2NvcGVTdGFja1xuICAgICk7XG4gIH1cbiAgd2l0aEVuZFJ1bGUoZW5kUnVsZSkge1xuICAgIGlmICh0aGlzLmVuZFJ1bGUgPT09IGVuZFJ1bGUpIHtcbiAgICAgIHJldHVybiB0aGlzO1xuICAgIH1cbiAgICByZXR1cm4gbmV3IF9TdGF0ZVN0YWNrSW1wbChcbiAgICAgIHRoaXMucGFyZW50LFxuICAgICAgdGhpcy5ydWxlSWQsXG4gICAgICB0aGlzLl9lbnRlclBvcyxcbiAgICAgIHRoaXMuX2FuY2hvclBvcyxcbiAgICAgIHRoaXMuYmVnaW5SdWxlQ2FwdHVyZWRFT0wsXG4gICAgICBlbmRSdWxlLFxuICAgICAgdGhpcy5uYW1lU2NvcGVzTGlzdCxcbiAgICAgIHRoaXMuY29udGVudE5hbWVTY29wZXNMaXN0XG4gICAgKTtcbiAgfVxuICAvLyBVc2VkIHRvIHdhcm4gb2YgZW5kbGVzcyBsb29wc1xuICBoYXNTYW1lUnVsZUFzKG90aGVyKSB7XG4gICAgbGV0IGVsID0gdGhpcztcbiAgICB3aGlsZSAoZWwgJiYgZWwuX2VudGVyUG9zID09PSBvdGhlci5fZW50ZXJQb3MpIHtcbiAgICAgIGlmIChlbC5ydWxlSWQgPT09IG90aGVyLnJ1bGVJZCkge1xuICAgICAgICByZXR1cm4gdHJ1ZTtcbiAgICAgIH1cbiAgICAgIGVsID0gZWwucGFyZW50O1xuICAgIH1cbiAgICByZXR1cm4gZmFsc2U7XG4gIH1cbiAgdG9TdGF0ZVN0YWNrRnJhbWUoKSB7XG4gICAgcmV0dXJuIHtcbiAgICAgIHJ1bGVJZDogcnVsZUlkVG9OdW1iZXIodGhpcy5ydWxlSWQpLFxuICAgICAgYmVnaW5SdWxlQ2FwdHVyZWRFT0w6IHRoaXMuYmVnaW5SdWxlQ2FwdHVyZWRFT0wsXG4gICAgICBlbmRSdWxlOiB0aGlzLmVuZFJ1bGUsXG4gICAgICBuYW1lU2NvcGVzTGlzdDogdGhpcy5uYW1lU2NvcGVzTGlzdD8uZ2V0RXh0ZW5zaW9uSWZEZWZpbmVkKHRoaXMucGFyZW50Py5uYW1lU2NvcGVzTGlzdCA/PyBudWxsKSA/PyBbXSxcbiAgICAgIGNvbnRlbnROYW1lU2NvcGVzTGlzdDogdGhpcy5jb250ZW50TmFtZVNjb3Blc0xpc3Q/LmdldEV4dGVuc2lvbklmRGVmaW5lZCh0aGlzLm5hbWVTY29wZXNMaXN0KSA/PyBbXVxuICAgIH07XG4gIH1cbiAgc3RhdGljIHB1c2hGcmFtZShzZWxmLCBmcmFtZSkge1xuICAgIGNvbnN0IG5hbWVzU2NvcGVMaXN0ID0gQXR0cmlidXRlZFNjb3BlU3RhY2suZnJvbUV4dGVuc2lvbihzZWxmPy5uYW1lU2NvcGVzTGlzdCA/PyBudWxsLCBmcmFtZS5uYW1lU2NvcGVzTGlzdCk7XG4gICAgcmV0dXJuIG5ldyBfU3RhdGVTdGFja0ltcGwoXG4gICAgICBzZWxmLFxuICAgICAgcnVsZUlkRnJvbU51bWJlcihmcmFtZS5ydWxlSWQpLFxuICAgICAgZnJhbWUuZW50ZXJQb3MgPz8gLTEsXG4gICAgICBmcmFtZS5hbmNob3JQb3MgPz8gLTEsXG4gICAgICBmcmFtZS5iZWdpblJ1bGVDYXB0dXJlZEVPTCxcbiAgICAgIGZyYW1lLmVuZFJ1bGUsXG4gICAgICBuYW1lc1Njb3BlTGlzdCxcbiAgICAgIEF0dHJpYnV0ZWRTY29wZVN0YWNrLmZyb21FeHRlbnNpb24obmFtZXNTY29wZUxpc3QsIGZyYW1lLmNvbnRlbnROYW1lU2NvcGVzTGlzdClcbiAgICApO1xuICB9XG59O1xudmFyIEJhbGFuY2VkQnJhY2tldFNlbGVjdG9ycyA9IGNsYXNzIHtcbiAgYmFsYW5jZWRCcmFja2V0U2NvcGVzO1xuICB1bmJhbGFuY2VkQnJhY2tldFNjb3BlcztcbiAgYWxsb3dBbnkgPSBmYWxzZTtcbiAgY29uc3RydWN0b3IoYmFsYW5jZWRCcmFja2V0U2NvcGVzLCB1bmJhbGFuY2VkQnJhY2tldFNjb3Blcykge1xuICAgIHRoaXMuYmFsYW5jZWRCcmFja2V0U2NvcGVzID0gYmFsYW5jZWRCcmFja2V0U2NvcGVzLmZsYXRNYXAoXG4gICAgICAoc2VsZWN0b3IpID0+IHtcbiAgICAgICAgaWYgKHNlbGVjdG9yID09PSBcIipcIikge1xuICAgICAgICAgIHRoaXMuYWxsb3dBbnkgPSB0cnVlO1xuICAgICAgICAgIHJldHVybiBbXTtcbiAgICAgICAgfVxuICAgICAgICByZXR1cm4gY3JlYXRlTWF0Y2hlcnMoc2VsZWN0b3IsIG5hbWVNYXRjaGVyKS5tYXAoKG0pID0+IG0ubWF0Y2hlcik7XG4gICAgICB9XG4gICAgKTtcbiAgICB0aGlzLnVuYmFsYW5jZWRCcmFja2V0U2NvcGVzID0gdW5iYWxhbmNlZEJyYWNrZXRTY29wZXMuZmxhdE1hcChcbiAgICAgIChzZWxlY3RvcikgPT4gY3JlYXRlTWF0Y2hlcnMoc2VsZWN0b3IsIG5hbWVNYXRjaGVyKS5tYXAoKG0pID0+IG0ubWF0Y2hlcilcbiAgICApO1xuICB9XG4gIGdldCBtYXRjaGVzQWx3YXlzKCkge1xuICAgIHJldHVybiB0aGlzLmFsbG93QW55ICYmIHRoaXMudW5iYWxhbmNlZEJyYWNrZXRTY29wZXMubGVuZ3RoID09PSAwO1xuICB9XG4gIGdldCBtYXRjaGVzTmV2ZXIoKSB7XG4gICAgcmV0dXJuIHRoaXMuYmFsYW5jZWRCcmFja2V0U2NvcGVzLmxlbmd0aCA9PT0gMCAmJiAhdGhpcy5hbGxvd0FueTtcbiAgfVxuICBtYXRjaChzY29wZXMpIHtcbiAgICBmb3IgKGNvbnN0IGV4Y2x1ZGVyIG9mIHRoaXMudW5iYWxhbmNlZEJyYWNrZXRTY29wZXMpIHtcbiAgICAgIGlmIChleGNsdWRlcihzY29wZXMpKSB7XG4gICAgICAgIHJldHVybiBmYWxzZTtcbiAgICAgIH1cbiAgICB9XG4gICAgZm9yIChjb25zdCBpbmNsdWRlciBvZiB0aGlzLmJhbGFuY2VkQnJhY2tldFNjb3Blcykge1xuICAgICAgaWYgKGluY2x1ZGVyKHNjb3BlcykpIHtcbiAgICAgICAgcmV0dXJuIHRydWU7XG4gICAgICB9XG4gICAgfVxuICAgIHJldHVybiB0aGlzLmFsbG93QW55O1xuICB9XG59O1xudmFyIExpbmVUb2tlbnMgPSBjbGFzcyB7XG4gIGNvbnN0cnVjdG9yKGVtaXRCaW5hcnlUb2tlbnMsIGxpbmVUZXh0LCB0b2tlblR5cGVPdmVycmlkZXMsIGJhbGFuY2VkQnJhY2tldFNlbGVjdG9ycykge1xuICAgIHRoaXMuYmFsYW5jZWRCcmFja2V0U2VsZWN0b3JzID0gYmFsYW5jZWRCcmFja2V0U2VsZWN0b3JzO1xuICAgIHRoaXMuX2VtaXRCaW5hcnlUb2tlbnMgPSBlbWl0QmluYXJ5VG9rZW5zO1xuICAgIHRoaXMuX3Rva2VuVHlwZU92ZXJyaWRlcyA9IHRva2VuVHlwZU92ZXJyaWRlcztcbiAgICBpZiAoZmFsc2UpIHtcbiAgICAgIHRoaXMuX2xpbmVUZXh0ID0gbGluZVRleHQ7XG4gICAgfSBlbHNlIHtcbiAgICAgIHRoaXMuX2xpbmVUZXh0ID0gbnVsbDtcbiAgICB9XG4gICAgdGhpcy5fdG9rZW5zID0gW107XG4gICAgdGhpcy5fYmluYXJ5VG9rZW5zID0gW107XG4gICAgdGhpcy5fbGFzdFRva2VuRW5kSW5kZXggPSAwO1xuICB9XG4gIF9lbWl0QmluYXJ5VG9rZW5zO1xuICAvKipcbiAgICogZGVmaW5lZCBvbmx5IGlmIGBmYWxzZWAuXG4gICAqL1xuICBfbGluZVRleHQ7XG4gIC8qKlxuICAgKiB1c2VkIG9ubHkgaWYgYF9lbWl0QmluYXJ5VG9rZW5zYCBpcyBmYWxzZS5cbiAgICovXG4gIF90b2tlbnM7XG4gIC8qKlxuICAgKiB1c2VkIG9ubHkgaWYgYF9lbWl0QmluYXJ5VG9rZW5zYCBpcyB0cnVlLlxuICAgKi9cbiAgX2JpbmFyeVRva2VucztcbiAgX2xhc3RUb2tlbkVuZEluZGV4O1xuICBfdG9rZW5UeXBlT3ZlcnJpZGVzO1xuICBwcm9kdWNlKHN0YWNrLCBlbmRJbmRleCkge1xuICAgIHRoaXMucHJvZHVjZUZyb21TY29wZXMoc3RhY2suY29udGVudE5hbWVTY29wZXNMaXN0LCBlbmRJbmRleCk7XG4gIH1cbiAgcHJvZHVjZUZyb21TY29wZXMoc2NvcGVzTGlzdCwgZW5kSW5kZXgpIHtcbiAgICBpZiAodGhpcy5fbGFzdFRva2VuRW5kSW5kZXggPj0gZW5kSW5kZXgpIHtcbiAgICAgIHJldHVybjtcbiAgICB9XG4gICAgaWYgKHRoaXMuX2VtaXRCaW5hcnlUb2tlbnMpIHtcbiAgICAgIGxldCBtZXRhZGF0YSA9IHNjb3Blc0xpc3Q/LnRva2VuQXR0cmlidXRlcyA/PyAwO1xuICAgICAgbGV0IGNvbnRhaW5zQmFsYW5jZWRCcmFja2V0cyA9IGZhbHNlO1xuICAgICAgaWYgKHRoaXMuYmFsYW5jZWRCcmFja2V0U2VsZWN0b3JzPy5tYXRjaGVzQWx3YXlzKSB7XG4gICAgICAgIGNvbnRhaW5zQmFsYW5jZWRCcmFja2V0cyA9IHRydWU7XG4gICAgICB9XG4gICAgICBpZiAodGhpcy5fdG9rZW5UeXBlT3ZlcnJpZGVzLmxlbmd0aCA+IDAgfHwgdGhpcy5iYWxhbmNlZEJyYWNrZXRTZWxlY3RvcnMgJiYgIXRoaXMuYmFsYW5jZWRCcmFja2V0U2VsZWN0b3JzLm1hdGNoZXNBbHdheXMgJiYgIXRoaXMuYmFsYW5jZWRCcmFja2V0U2VsZWN0b3JzLm1hdGNoZXNOZXZlcikge1xuICAgICAgICBjb25zdCBzY29wZXMyID0gc2NvcGVzTGlzdD8uZ2V0U2NvcGVOYW1lcygpID8/IFtdO1xuICAgICAgICBmb3IgKGNvbnN0IHRva2VuVHlwZSBvZiB0aGlzLl90b2tlblR5cGVPdmVycmlkZXMpIHtcbiAgICAgICAgICBpZiAodG9rZW5UeXBlLm1hdGNoZXIoc2NvcGVzMikpIHtcbiAgICAgICAgICAgIG1ldGFkYXRhID0gRW5jb2RlZFRva2VuTWV0YWRhdGEuc2V0KFxuICAgICAgICAgICAgICBtZXRhZGF0YSxcbiAgICAgICAgICAgICAgMCxcbiAgICAgICAgICAgICAgdG9PcHRpb25hbFRva2VuVHlwZSh0b2tlblR5cGUudHlwZSksXG4gICAgICAgICAgICAgIG51bGwsXG4gICAgICAgICAgICAgIC0xIC8qIE5vdFNldCAqLyxcbiAgICAgICAgICAgICAgMCxcbiAgICAgICAgICAgICAgMFxuICAgICAgICAgICAgKTtcbiAgICAgICAgICB9XG4gICAgICAgIH1cbiAgICAgICAgaWYgKHRoaXMuYmFsYW5jZWRCcmFja2V0U2VsZWN0b3JzKSB7XG4gICAgICAgICAgY29udGFpbnNCYWxhbmNlZEJyYWNrZXRzID0gdGhpcy5iYWxhbmNlZEJyYWNrZXRTZWxlY3RvcnMubWF0Y2goc2NvcGVzMik7XG4gICAgICAgIH1cbiAgICAgIH1cbiAgICAgIGlmIChjb250YWluc0JhbGFuY2VkQnJhY2tldHMpIHtcbiAgICAgICAgbWV0YWRhdGEgPSBFbmNvZGVkVG9rZW5NZXRhZGF0YS5zZXQoXG4gICAgICAgICAgbWV0YWRhdGEsXG4gICAgICAgICAgMCxcbiAgICAgICAgICA4IC8qIE5vdFNldCAqLyxcbiAgICAgICAgICBjb250YWluc0JhbGFuY2VkQnJhY2tldHMsXG4gICAgICAgICAgLTEgLyogTm90U2V0ICovLFxuICAgICAgICAgIDAsXG4gICAgICAgICAgMFxuICAgICAgICApO1xuICAgICAgfVxuICAgICAgaWYgKHRoaXMuX2JpbmFyeVRva2Vucy5sZW5ndGggPiAwICYmIHRoaXMuX2JpbmFyeVRva2Vuc1t0aGlzLl9iaW5hcnlUb2tlbnMubGVuZ3RoIC0gMV0gPT09IG1ldGFkYXRhKSB7XG4gICAgICAgIHRoaXMuX2xhc3RUb2tlbkVuZEluZGV4ID0gZW5kSW5kZXg7XG4gICAgICAgIHJldHVybjtcbiAgICAgIH1cbiAgICAgIHRoaXMuX2JpbmFyeVRva2Vucy5wdXNoKHRoaXMuX2xhc3RUb2tlbkVuZEluZGV4KTtcbiAgICAgIHRoaXMuX2JpbmFyeVRva2Vucy5wdXNoKG1ldGFkYXRhKTtcbiAgICAgIHRoaXMuX2xhc3RUb2tlbkVuZEluZGV4ID0gZW5kSW5kZXg7XG4gICAgICByZXR1cm47XG4gICAgfVxuICAgIGNvbnN0IHNjb3BlcyA9IHNjb3Blc0xpc3Q/LmdldFNjb3BlTmFtZXMoKSA/PyBbXTtcbiAgICB0aGlzLl90b2tlbnMucHVzaCh7XG4gICAgICBzdGFydEluZGV4OiB0aGlzLl9sYXN0VG9rZW5FbmRJbmRleCxcbiAgICAgIGVuZEluZGV4LFxuICAgICAgLy8gdmFsdWU6IGxpbmVUZXh0LnN1YnN0cmluZyhsYXN0VG9rZW5FbmRJbmRleCwgZW5kSW5kZXgpLFxuICAgICAgc2NvcGVzXG4gICAgfSk7XG4gICAgdGhpcy5fbGFzdFRva2VuRW5kSW5kZXggPSBlbmRJbmRleDtcbiAgfVxuICBnZXRSZXN1bHQoc3RhY2ssIGxpbmVMZW5ndGgpIHtcbiAgICBpZiAodGhpcy5fdG9rZW5zLmxlbmd0aCA+IDAgJiYgdGhpcy5fdG9rZW5zW3RoaXMuX3Rva2Vucy5sZW5ndGggLSAxXS5zdGFydEluZGV4ID09PSBsaW5lTGVuZ3RoIC0gMSkge1xuICAgICAgdGhpcy5fdG9rZW5zLnBvcCgpO1xuICAgIH1cbiAgICBpZiAodGhpcy5fdG9rZW5zLmxlbmd0aCA9PT0gMCkge1xuICAgICAgdGhpcy5fbGFzdFRva2VuRW5kSW5kZXggPSAtMTtcbiAgICAgIHRoaXMucHJvZHVjZShzdGFjaywgbGluZUxlbmd0aCk7XG4gICAgICB0aGlzLl90b2tlbnNbdGhpcy5fdG9rZW5zLmxlbmd0aCAtIDFdLnN0YXJ0SW5kZXggPSAwO1xuICAgIH1cbiAgICByZXR1cm4gdGhpcy5fdG9rZW5zO1xuICB9XG4gIGdldEJpbmFyeVJlc3VsdChzdGFjaywgbGluZUxlbmd0aCkge1xuICAgIGlmICh0aGlzLl9iaW5hcnlUb2tlbnMubGVuZ3RoID4gMCAmJiB0aGlzLl9iaW5hcnlUb2tlbnNbdGhpcy5fYmluYXJ5VG9rZW5zLmxlbmd0aCAtIDJdID09PSBsaW5lTGVuZ3RoIC0gMSkge1xuICAgICAgdGhpcy5fYmluYXJ5VG9rZW5zLnBvcCgpO1xuICAgICAgdGhpcy5fYmluYXJ5VG9rZW5zLnBvcCgpO1xuICAgIH1cbiAgICBpZiAodGhpcy5fYmluYXJ5VG9rZW5zLmxlbmd0aCA9PT0gMCkge1xuICAgICAgdGhpcy5fbGFzdFRva2VuRW5kSW5kZXggPSAtMTtcbiAgICAgIHRoaXMucHJvZHVjZShzdGFjaywgbGluZUxlbmd0aCk7XG4gICAgICB0aGlzLl9iaW5hcnlUb2tlbnNbdGhpcy5fYmluYXJ5VG9rZW5zLmxlbmd0aCAtIDJdID0gMDtcbiAgICB9XG4gICAgY29uc3QgcmVzdWx0ID0gbmV3IFVpbnQzMkFycmF5KHRoaXMuX2JpbmFyeVRva2Vucy5sZW5ndGgpO1xuICAgIGZvciAobGV0IGkgPSAwLCBsZW4gPSB0aGlzLl9iaW5hcnlUb2tlbnMubGVuZ3RoOyBpIDwgbGVuOyBpKyspIHtcbiAgICAgIHJlc3VsdFtpXSA9IHRoaXMuX2JpbmFyeVRva2Vuc1tpXTtcbiAgICB9XG4gICAgcmV0dXJuIHJlc3VsdDtcbiAgfVxufTtcblxuLy8gc3JjL3JlZ2lzdHJ5LnRzXG52YXIgU3luY1JlZ2lzdHJ5ID0gY2xhc3Mge1xuICBjb25zdHJ1Y3Rvcih0aGVtZSwgX29uaWdMaWIpIHtcbiAgICB0aGlzLl9vbmlnTGliID0gX29uaWdMaWI7XG4gICAgdGhpcy5fdGhlbWUgPSB0aGVtZTtcbiAgfVxuICBfZ3JhbW1hcnMgPSAvKiBAX19QVVJFX18gKi8gbmV3IE1hcCgpO1xuICBfcmF3R3JhbW1hcnMgPSAvKiBAX19QVVJFX18gKi8gbmV3IE1hcCgpO1xuICBfaW5qZWN0aW9uR3JhbW1hcnMgPSAvKiBAX19QVVJFX18gKi8gbmV3IE1hcCgpO1xuICBfdGhlbWU7XG4gIGRpc3Bvc2UoKSB7XG4gICAgZm9yIChjb25zdCBncmFtbWFyIG9mIHRoaXMuX2dyYW1tYXJzLnZhbHVlcygpKSB7XG4gICAgICBncmFtbWFyLmRpc3Bvc2UoKTtcbiAgICB9XG4gIH1cbiAgc2V0VGhlbWUodGhlbWUpIHtcbiAgICB0aGlzLl90aGVtZSA9IHRoZW1lO1xuICB9XG4gIGdldENvbG9yTWFwKCkge1xuICAgIHJldHVybiB0aGlzLl90aGVtZS5nZXRDb2xvck1hcCgpO1xuICB9XG4gIC8qKlxuICAgKiBBZGQgYGdyYW1tYXJgIHRvIHJlZ2lzdHJ5IGFuZCByZXR1cm4gYSBsaXN0IG9mIHJlZmVyZW5jZWQgc2NvcGUgbmFtZXNcbiAgICovXG4gIGFkZEdyYW1tYXIoZ3JhbW1hciwgaW5qZWN0aW9uU2NvcGVOYW1lcykge1xuICAgIHRoaXMuX3Jhd0dyYW1tYXJzLnNldChncmFtbWFyLnNjb3BlTmFtZSwgZ3JhbW1hcik7XG4gICAgaWYgKGluamVjdGlvblNjb3BlTmFtZXMpIHtcbiAgICAgIHRoaXMuX2luamVjdGlvbkdyYW1tYXJzLnNldChncmFtbWFyLnNjb3BlTmFtZSwgaW5qZWN0aW9uU2NvcGVOYW1lcyk7XG4gICAgfVxuICB9XG4gIC8qKlxuICAgKiBMb29rdXAgYSByYXcgZ3JhbW1hci5cbiAgICovXG4gIGxvb2t1cChzY29wZU5hbWUpIHtcbiAgICByZXR1cm4gdGhpcy5fcmF3R3JhbW1hcnMuZ2V0KHNjb3BlTmFtZSk7XG4gIH1cbiAgLyoqXG4gICAqIFJldHVybnMgdGhlIGluamVjdGlvbnMgZm9yIHRoZSBnaXZlbiBncmFtbWFyXG4gICAqL1xuICBpbmplY3Rpb25zKHRhcmdldFNjb3BlKSB7XG4gICAgcmV0dXJuIHRoaXMuX2luamVjdGlvbkdyYW1tYXJzLmdldCh0YXJnZXRTY29wZSk7XG4gIH1cbiAgLyoqXG4gICAqIEdldCB0aGUgZGVmYXVsdCB0aGVtZSBzZXR0aW5nc1xuICAgKi9cbiAgZ2V0RGVmYXVsdHMoKSB7XG4gICAgcmV0dXJuIHRoaXMuX3RoZW1lLmdldERlZmF1bHRzKCk7XG4gIH1cbiAgLyoqXG4gICAqIE1hdGNoIGEgc2NvcGUgaW4gdGhlIHRoZW1lLlxuICAgKi9cbiAgdGhlbWVNYXRjaChzY29wZVBhdGgpIHtcbiAgICByZXR1cm4gdGhpcy5fdGhlbWUubWF0Y2goc2NvcGVQYXRoKTtcbiAgfVxuICAvKipcbiAgICogTG9va3VwIGEgZ3JhbW1hci5cbiAgICovXG4gIGdyYW1tYXJGb3JTY29wZU5hbWUoc2NvcGVOYW1lLCBpbml0aWFsTGFuZ3VhZ2UsIGVtYmVkZGVkTGFuZ3VhZ2VzLCB0b2tlblR5cGVzLCBiYWxhbmNlZEJyYWNrZXRTZWxlY3RvcnMpIHtcbiAgICBpZiAoIXRoaXMuX2dyYW1tYXJzLmhhcyhzY29wZU5hbWUpKSB7XG4gICAgICBsZXQgcmF3R3JhbW1hciA9IHRoaXMuX3Jhd0dyYW1tYXJzLmdldChzY29wZU5hbWUpO1xuICAgICAgaWYgKCFyYXdHcmFtbWFyKSB7XG4gICAgICAgIHJldHVybiBudWxsO1xuICAgICAgfVxuICAgICAgdGhpcy5fZ3JhbW1hcnMuc2V0KHNjb3BlTmFtZSwgY3JlYXRlR3JhbW1hcihcbiAgICAgICAgc2NvcGVOYW1lLFxuICAgICAgICByYXdHcmFtbWFyLFxuICAgICAgICBpbml0aWFsTGFuZ3VhZ2UsXG4gICAgICAgIGVtYmVkZGVkTGFuZ3VhZ2VzLFxuICAgICAgICB0b2tlblR5cGVzLFxuICAgICAgICBiYWxhbmNlZEJyYWNrZXRTZWxlY3RvcnMsXG4gICAgICAgIHRoaXMsXG4gICAgICAgIHRoaXMuX29uaWdMaWJcbiAgICAgICkpO1xuICAgIH1cbiAgICByZXR1cm4gdGhpcy5fZ3JhbW1hcnMuZ2V0KHNjb3BlTmFtZSk7XG4gIH1cbn07XG5cbi8vIHNyYy9pbmRleC50c1xudmFyIFJlZ2lzdHJ5ID0gY2xhc3Mge1xuICBfb3B0aW9ucztcbiAgX3N5bmNSZWdpc3RyeTtcbiAgX2Vuc3VyZUdyYW1tYXJDYWNoZTtcbiAgY29uc3RydWN0b3Iob3B0aW9ucykge1xuICAgIHRoaXMuX29wdGlvbnMgPSBvcHRpb25zO1xuICAgIHRoaXMuX3N5bmNSZWdpc3RyeSA9IG5ldyBTeW5jUmVnaXN0cnkoXG4gICAgICBUaGVtZS5jcmVhdGVGcm9tUmF3VGhlbWUob3B0aW9ucy50aGVtZSwgb3B0aW9ucy5jb2xvck1hcCksXG4gICAgICBvcHRpb25zLm9uaWdMaWJcbiAgICApO1xuICAgIHRoaXMuX2Vuc3VyZUdyYW1tYXJDYWNoZSA9IC8qIEBfX1BVUkVfXyAqLyBuZXcgTWFwKCk7XG4gIH1cbiAgZGlzcG9zZSgpIHtcbiAgICB0aGlzLl9zeW5jUmVnaXN0cnkuZGlzcG9zZSgpO1xuICB9XG4gIC8qKlxuICAgKiBDaGFuZ2UgdGhlIHRoZW1lLiBPbmNlIGNhbGxlZCwgbm8gcHJldmlvdXMgYHJ1bGVTdGFja2Agc2hvdWxkIGJlIHVzZWQgYW55bW9yZS5cbiAgICovXG4gIHNldFRoZW1lKHRoZW1lLCBjb2xvck1hcCkge1xuICAgIHRoaXMuX3N5bmNSZWdpc3RyeS5zZXRUaGVtZShUaGVtZS5jcmVhdGVGcm9tUmF3VGhlbWUodGhlbWUsIGNvbG9yTWFwKSk7XG4gIH1cbiAgLyoqXG4gICAqIFJldHVybnMgYSBsb29rdXAgYXJyYXkgZm9yIGNvbG9yIGlkcy5cbiAgICovXG4gIGdldENvbG9yTWFwKCkge1xuICAgIHJldHVybiB0aGlzLl9zeW5jUmVnaXN0cnkuZ2V0Q29sb3JNYXAoKTtcbiAgfVxuICAvKipcbiAgICogTG9hZCB0aGUgZ3JhbW1hciBmb3IgYHNjb3BlTmFtZWAgYW5kIGFsbCByZWZlcmVuY2VkIGluY2x1ZGVkIGdyYW1tYXJzIGFzeW5jaHJvbm91c2x5LlxuICAgKiBQbGVhc2UgZG8gbm90IHVzZSBsYW5ndWFnZSBpZCAwLlxuICAgKi9cbiAgbG9hZEdyYW1tYXJXaXRoRW1iZWRkZWRMYW5ndWFnZXMoaW5pdGlhbFNjb3BlTmFtZSwgaW5pdGlhbExhbmd1YWdlLCBlbWJlZGRlZExhbmd1YWdlcykge1xuICAgIHJldHVybiB0aGlzLmxvYWRHcmFtbWFyV2l0aENvbmZpZ3VyYXRpb24oaW5pdGlhbFNjb3BlTmFtZSwgaW5pdGlhbExhbmd1YWdlLCB7IGVtYmVkZGVkTGFuZ3VhZ2VzIH0pO1xuICB9XG4gIC8qKlxuICAgKiBMb2FkIHRoZSBncmFtbWFyIGZvciBgc2NvcGVOYW1lYCBhbmQgYWxsIHJlZmVyZW5jZWQgaW5jbHVkZWQgZ3JhbW1hcnMgYXN5bmNocm9ub3VzbHkuXG4gICAqIFBsZWFzZSBkbyBub3QgdXNlIGxhbmd1YWdlIGlkIDAuXG4gICAqL1xuICBsb2FkR3JhbW1hcldpdGhDb25maWd1cmF0aW9uKGluaXRpYWxTY29wZU5hbWUsIGluaXRpYWxMYW5ndWFnZSwgY29uZmlndXJhdGlvbikge1xuICAgIHJldHVybiB0aGlzLl9sb2FkR3JhbW1hcihcbiAgICAgIGluaXRpYWxTY29wZU5hbWUsXG4gICAgICBpbml0aWFsTGFuZ3VhZ2UsXG4gICAgICBjb25maWd1cmF0aW9uLmVtYmVkZGVkTGFuZ3VhZ2VzLFxuICAgICAgY29uZmlndXJhdGlvbi50b2tlblR5cGVzLFxuICAgICAgbmV3IEJhbGFuY2VkQnJhY2tldFNlbGVjdG9ycyhcbiAgICAgICAgY29uZmlndXJhdGlvbi5iYWxhbmNlZEJyYWNrZXRTZWxlY3RvcnMgfHwgW10sXG4gICAgICAgIGNvbmZpZ3VyYXRpb24udW5iYWxhbmNlZEJyYWNrZXRTZWxlY3RvcnMgfHwgW11cbiAgICAgIClcbiAgICApO1xuICB9XG4gIC8qKlxuICAgKiBMb2FkIHRoZSBncmFtbWFyIGZvciBgc2NvcGVOYW1lYCBhbmQgYWxsIHJlZmVyZW5jZWQgaW5jbHVkZWQgZ3JhbW1hcnMgYXN5bmNocm9ub3VzbHkuXG4gICAqL1xuICBsb2FkR3JhbW1hcihpbml0aWFsU2NvcGVOYW1lKSB7XG4gICAgcmV0dXJuIHRoaXMuX2xvYWRHcmFtbWFyKGluaXRpYWxTY29wZU5hbWUsIDAsIG51bGwsIG51bGwsIG51bGwpO1xuICB9XG4gIF9sb2FkR3JhbW1hcihpbml0aWFsU2NvcGVOYW1lLCBpbml0aWFsTGFuZ3VhZ2UsIGVtYmVkZGVkTGFuZ3VhZ2VzLCB0b2tlblR5cGVzLCBiYWxhbmNlZEJyYWNrZXRTZWxlY3RvcnMpIHtcbiAgICBjb25zdCBkZXBlbmRlbmN5UHJvY2Vzc29yID0gbmV3IFNjb3BlRGVwZW5kZW5jeVByb2Nlc3Nvcih0aGlzLl9zeW5jUmVnaXN0cnksIGluaXRpYWxTY29wZU5hbWUpO1xuICAgIHdoaWxlIChkZXBlbmRlbmN5UHJvY2Vzc29yLlEubGVuZ3RoID4gMCkge1xuICAgICAgZGVwZW5kZW5jeVByb2Nlc3Nvci5RLm1hcCgocmVxdWVzdCkgPT4gdGhpcy5fbG9hZFNpbmdsZUdyYW1tYXIocmVxdWVzdC5zY29wZU5hbWUpKTtcbiAgICAgIGRlcGVuZGVuY3lQcm9jZXNzb3IucHJvY2Vzc1F1ZXVlKCk7XG4gICAgfVxuICAgIHJldHVybiB0aGlzLl9ncmFtbWFyRm9yU2NvcGVOYW1lKFxuICAgICAgaW5pdGlhbFNjb3BlTmFtZSxcbiAgICAgIGluaXRpYWxMYW5ndWFnZSxcbiAgICAgIGVtYmVkZGVkTGFuZ3VhZ2VzLFxuICAgICAgdG9rZW5UeXBlcyxcbiAgICAgIGJhbGFuY2VkQnJhY2tldFNlbGVjdG9yc1xuICAgICk7XG4gIH1cbiAgX2xvYWRTaW5nbGVHcmFtbWFyKHNjb3BlTmFtZSkge1xuICAgIGlmICghdGhpcy5fZW5zdXJlR3JhbW1hckNhY2hlLmhhcyhzY29wZU5hbWUpKSB7XG4gICAgICB0aGlzLl9kb0xvYWRTaW5nbGVHcmFtbWFyKHNjb3BlTmFtZSk7XG4gICAgICB0aGlzLl9lbnN1cmVHcmFtbWFyQ2FjaGUuc2V0KHNjb3BlTmFtZSwgdHJ1ZSk7XG4gICAgfVxuICB9XG4gIF9kb0xvYWRTaW5nbGVHcmFtbWFyKHNjb3BlTmFtZSkge1xuICAgIGNvbnN0IGdyYW1tYXIgPSB0aGlzLl9vcHRpb25zLmxvYWRHcmFtbWFyKHNjb3BlTmFtZSk7XG4gICAgaWYgKGdyYW1tYXIpIHtcbiAgICAgIGNvbnN0IGluamVjdGlvbnMgPSB0eXBlb2YgdGhpcy5fb3B0aW9ucy5nZXRJbmplY3Rpb25zID09PSBcImZ1bmN0aW9uXCIgPyB0aGlzLl9vcHRpb25zLmdldEluamVjdGlvbnMoc2NvcGVOYW1lKSA6IHZvaWQgMDtcbiAgICAgIHRoaXMuX3N5bmNSZWdpc3RyeS5hZGRHcmFtbWFyKGdyYW1tYXIsIGluamVjdGlvbnMpO1xuICAgIH1cbiAgfVxuICAvKipcbiAgICogQWRkcyBhIHJhd0dyYW1tYXIuXG4gICAqL1xuICBhZGRHcmFtbWFyKHJhd0dyYW1tYXIsIGluamVjdGlvbnMgPSBbXSwgaW5pdGlhbExhbmd1YWdlID0gMCwgZW1iZWRkZWRMYW5ndWFnZXMgPSBudWxsKSB7XG4gICAgdGhpcy5fc3luY1JlZ2lzdHJ5LmFkZEdyYW1tYXIocmF3R3JhbW1hciwgaW5qZWN0aW9ucyk7XG4gICAgcmV0dXJuIHRoaXMuX2dyYW1tYXJGb3JTY29wZU5hbWUocmF3R3JhbW1hci5zY29wZU5hbWUsIGluaXRpYWxMYW5ndWFnZSwgZW1iZWRkZWRMYW5ndWFnZXMpO1xuICB9XG4gIC8qKlxuICAgKiBHZXQgdGhlIGdyYW1tYXIgZm9yIGBzY29wZU5hbWVgLiBUaGUgZ3JhbW1hciBtdXN0IGZpcnN0IGJlIGNyZWF0ZWQgdmlhIGBsb2FkR3JhbW1hcmAgb3IgYGFkZEdyYW1tYXJgLlxuICAgKi9cbiAgX2dyYW1tYXJGb3JTY29wZU5hbWUoc2NvcGVOYW1lLCBpbml0aWFsTGFuZ3VhZ2UgPSAwLCBlbWJlZGRlZExhbmd1YWdlcyA9IG51bGwsIHRva2VuVHlwZXMgPSBudWxsLCBiYWxhbmNlZEJyYWNrZXRTZWxlY3RvcnMgPSBudWxsKSB7XG4gICAgcmV0dXJuIHRoaXMuX3N5bmNSZWdpc3RyeS5ncmFtbWFyRm9yU2NvcGVOYW1lKFxuICAgICAgc2NvcGVOYW1lLFxuICAgICAgaW5pdGlhbExhbmd1YWdlLFxuICAgICAgZW1iZWRkZWRMYW5ndWFnZXMsXG4gICAgICB0b2tlblR5cGVzLFxuICAgICAgYmFsYW5jZWRCcmFja2V0U2VsZWN0b3JzXG4gICAgKTtcbiAgfVxufTtcbnZhciBJTklUSUFMID0gU3RhdGVTdGFja0ltcGwuTlVMTDtcbmV4cG9ydCB7XG4gIEVuY29kZWRUb2tlbk1ldGFkYXRhLFxuICBGaW5kT3B0aW9uLFxuICBGb250U3R5bGUsXG4gIElOSVRJQUwsXG4gIFJlZ2lzdHJ5LFxuICBUaGVtZSxcbiAgZGlzcG9zZU9uaWdTdHJpbmdcbn07XG4iLCJpbXBvcnQgeyBTaGlraUVycm9yIGFzIFNoaWtpRXJyb3IkMSB9IGZyb20gJ0BzaGlraWpzL3R5cGVzJztcbmV4cG9ydCAqIGZyb20gJ0BzaGlraWpzL3R5cGVzJztcbmltcG9ydCB7IGNyZWF0ZU9uaWd1cnVtYUVuZ2luZSBhcyBjcmVhdGVPbmlndXJ1bWFFbmdpbmUkMSwgbG9hZFdhc20gYXMgbG9hZFdhc20kMSwgZ2V0RGVmYXVsdFdhc21Mb2FkZXIgfSBmcm9tICdAc2hpa2lqcy9lbmdpbmUtb25pZ3VydW1hJztcbmltcG9ydCB7IHcgYXMgd2FybkRlcHJlY2F0ZWQgfSBmcm9tICcuL3NoYXJlZC9jb3JlLkRWVjhjNFJaLm1qcyc7XG5leHBvcnQgeyBlIGFzIGVuYWJsZURlcHJlY2F0aW9uV2FybmluZ3MgfSBmcm9tICcuL3NoYXJlZC9jb3JlLkRWVjhjNFJaLm1qcyc7XG5pbXBvcnQgeyBGb250U3R5bGUsIElOSVRJQUwsIEVuY29kZWRUb2tlbk1ldGFkYXRhLCBSZWdpc3RyeSBhcyBSZWdpc3RyeSQxLCBUaGVtZSB9IGZyb20gJ0BzaGlraWpzL3ZzY29kZS10ZXh0bWF0ZSc7XG5leHBvcnQgeyBGb250U3R5bGUsIEVuY29kZWRUb2tlbk1ldGFkYXRhIGFzIFN0YWNrRWxlbWVudE1ldGFkYXRhIH0gZnJvbSAnQHNoaWtpanMvdnNjb2RlLXRleHRtYXRlJztcbmltcG9ydCB7IHRvSHRtbCB9IGZyb20gJ2hhc3QtdXRpbC10by1odG1sJztcbmV4cG9ydCB7IHRvSHRtbCBhcyBoYXN0VG9IdG1sIH0gZnJvbSAnaGFzdC11dGlsLXRvLWh0bWwnO1xuaW1wb3J0IHsgY3JlYXRlSmF2YVNjcmlwdFJlZ2V4RW5naW5lIGFzIGNyZWF0ZUphdmFTY3JpcHRSZWdleEVuZ2luZSQxLCBkZWZhdWx0SmF2YVNjcmlwdFJlZ2V4Q29uc3RydWN0b3IgYXMgZGVmYXVsdEphdmFTY3JpcHRSZWdleENvbnN0cnVjdG9yJDEgfSBmcm9tICdAc2hpa2lqcy9lbmdpbmUtamF2YXNjcmlwdCc7XG5cbmZ1bmN0aW9uIGNyZWF0ZU9uaWd1cnVtYUVuZ2luZShvcHRpb25zKSB7XG4gIHdhcm5EZXByZWNhdGVkKFwiaW1wb3J0IGBjcmVhdGVPbmlndXJ1bWFFbmdpbmVgIGZyb20gYEBzaGlraWpzL2VuZ2luZS1vbmlndXJ1bWFgIG9yIGBzaGlraS9lbmdpbmUvb25pZ3VydW1hYCBpbnN0ZWFkXCIpO1xuICByZXR1cm4gY3JlYXRlT25pZ3VydW1hRW5naW5lJDEob3B0aW9ucyk7XG59XG5mdW5jdGlvbiBjcmVhdGVXYXNtT25pZ0VuZ2luZShvcHRpb25zKSB7XG4gIHdhcm5EZXByZWNhdGVkKFwiaW1wb3J0IGBjcmVhdGVPbmlndXJ1bWFFbmdpbmVgIGZyb20gYEBzaGlraWpzL2VuZ2luZS1vbmlndXJ1bWFgIG9yIGBzaGlraS9lbmdpbmUvb25pZ3VydW1hYCBpbnN0ZWFkXCIpO1xuICByZXR1cm4gY3JlYXRlT25pZ3VydW1hRW5naW5lJDEob3B0aW9ucyk7XG59XG5mdW5jdGlvbiBsb2FkV2FzbShvcHRpb25zKSB7XG4gIHdhcm5EZXByZWNhdGVkKFwiaW1wb3J0IGBsb2FkV2FzbWAgZnJvbSBgQHNoaWtpanMvZW5naW5lLW9uaWd1cnVtYWAgb3IgYHNoaWtpL2VuZ2luZS9vbmlndXJ1bWFgIGluc3RlYWRcIik7XG4gIHJldHVybiBsb2FkV2FzbSQxKG9wdGlvbnMpO1xufVxuXG5mdW5jdGlvbiB0b0FycmF5KHgpIHtcbiAgcmV0dXJuIEFycmF5LmlzQXJyYXkoeCkgPyB4IDogW3hdO1xufVxuZnVuY3Rpb24gc3BsaXRMaW5lcyhjb2RlLCBwcmVzZXJ2ZUVuZGluZyA9IGZhbHNlKSB7XG4gIGNvbnN0IHBhcnRzID0gY29kZS5zcGxpdCgvKFxccj9cXG4pL2cpO1xuICBsZXQgaW5kZXggPSAwO1xuICBjb25zdCBsaW5lcyA9IFtdO1xuICBmb3IgKGxldCBpID0gMDsgaSA8IHBhcnRzLmxlbmd0aDsgaSArPSAyKSB7XG4gICAgY29uc3QgbGluZSA9IHByZXNlcnZlRW5kaW5nID8gcGFydHNbaV0gKyAocGFydHNbaSArIDFdIHx8IFwiXCIpIDogcGFydHNbaV07XG4gICAgbGluZXMucHVzaChbbGluZSwgaW5kZXhdKTtcbiAgICBpbmRleCArPSBwYXJ0c1tpXS5sZW5ndGg7XG4gICAgaW5kZXggKz0gcGFydHNbaSArIDFdPy5sZW5ndGggfHwgMDtcbiAgfVxuICByZXR1cm4gbGluZXM7XG59XG5mdW5jdGlvbiBpc1BsYWluTGFuZyhsYW5nKSB7XG4gIHJldHVybiAhbGFuZyB8fCBbXCJwbGFpbnRleHRcIiwgXCJ0eHRcIiwgXCJ0ZXh0XCIsIFwicGxhaW5cIl0uaW5jbHVkZXMobGFuZyk7XG59XG5mdW5jdGlvbiBpc1NwZWNpYWxMYW5nKGxhbmcpIHtcbiAgcmV0dXJuIGxhbmcgPT09IFwiYW5zaVwiIHx8IGlzUGxhaW5MYW5nKGxhbmcpO1xufVxuZnVuY3Rpb24gaXNOb25lVGhlbWUodGhlbWUpIHtcbiAgcmV0dXJuIHRoZW1lID09PSBcIm5vbmVcIjtcbn1cbmZ1bmN0aW9uIGlzU3BlY2lhbFRoZW1lKHRoZW1lKSB7XG4gIHJldHVybiBpc05vbmVUaGVtZSh0aGVtZSk7XG59XG5mdW5jdGlvbiBhZGRDbGFzc1RvSGFzdChub2RlLCBjbGFzc05hbWUpIHtcbiAgaWYgKCFjbGFzc05hbWUpXG4gICAgcmV0dXJuIG5vZGU7XG4gIG5vZGUucHJvcGVydGllcyB8fD0ge307XG4gIG5vZGUucHJvcGVydGllcy5jbGFzcyB8fD0gW107XG4gIGlmICh0eXBlb2Ygbm9kZS5wcm9wZXJ0aWVzLmNsYXNzID09PSBcInN0cmluZ1wiKVxuICAgIG5vZGUucHJvcGVydGllcy5jbGFzcyA9IG5vZGUucHJvcGVydGllcy5jbGFzcy5zcGxpdCgvXFxzKy9nKTtcbiAgaWYgKCFBcnJheS5pc0FycmF5KG5vZGUucHJvcGVydGllcy5jbGFzcykpXG4gICAgbm9kZS5wcm9wZXJ0aWVzLmNsYXNzID0gW107XG4gIGNvbnN0IHRhcmdldHMgPSBBcnJheS5pc0FycmF5KGNsYXNzTmFtZSkgPyBjbGFzc05hbWUgOiBjbGFzc05hbWUuc3BsaXQoL1xccysvZyk7XG4gIGZvciAoY29uc3QgYyBvZiB0YXJnZXRzKSB7XG4gICAgaWYgKGMgJiYgIW5vZGUucHJvcGVydGllcy5jbGFzcy5pbmNsdWRlcyhjKSlcbiAgICAgIG5vZGUucHJvcGVydGllcy5jbGFzcy5wdXNoKGMpO1xuICB9XG4gIHJldHVybiBub2RlO1xufVxuZnVuY3Rpb24gc3BsaXRUb2tlbih0b2tlbiwgb2Zmc2V0cykge1xuICBsZXQgbGFzdE9mZnNldCA9IDA7XG4gIGNvbnN0IHRva2VucyA9IFtdO1xuICBmb3IgKGNvbnN0IG9mZnNldCBvZiBvZmZzZXRzKSB7XG4gICAgaWYgKG9mZnNldCA+IGxhc3RPZmZzZXQpIHtcbiAgICAgIHRva2Vucy5wdXNoKHtcbiAgICAgICAgLi4udG9rZW4sXG4gICAgICAgIGNvbnRlbnQ6IHRva2VuLmNvbnRlbnQuc2xpY2UobGFzdE9mZnNldCwgb2Zmc2V0KSxcbiAgICAgICAgb2Zmc2V0OiB0b2tlbi5vZmZzZXQgKyBsYXN0T2Zmc2V0XG4gICAgICB9KTtcbiAgICB9XG4gICAgbGFzdE9mZnNldCA9IG9mZnNldDtcbiAgfVxuICBpZiAobGFzdE9mZnNldCA8IHRva2VuLmNvbnRlbnQubGVuZ3RoKSB7XG4gICAgdG9rZW5zLnB1c2goe1xuICAgICAgLi4udG9rZW4sXG4gICAgICBjb250ZW50OiB0b2tlbi5jb250ZW50LnNsaWNlKGxhc3RPZmZzZXQpLFxuICAgICAgb2Zmc2V0OiB0b2tlbi5vZmZzZXQgKyBsYXN0T2Zmc2V0XG4gICAgfSk7XG4gIH1cbiAgcmV0dXJuIHRva2Vucztcbn1cbmZ1bmN0aW9uIHNwbGl0VG9rZW5zKHRva2VucywgYnJlYWtwb2ludHMpIHtcbiAgY29uc3Qgc29ydGVkID0gQXJyYXkuZnJvbShicmVha3BvaW50cyBpbnN0YW5jZW9mIFNldCA/IGJyZWFrcG9pbnRzIDogbmV3IFNldChicmVha3BvaW50cykpLnNvcnQoKGEsIGIpID0+IGEgLSBiKTtcbiAgaWYgKCFzb3J0ZWQubGVuZ3RoKVxuICAgIHJldHVybiB0b2tlbnM7XG4gIHJldHVybiB0b2tlbnMubWFwKChsaW5lKSA9PiB7XG4gICAgcmV0dXJuIGxpbmUuZmxhdE1hcCgodG9rZW4pID0+IHtcbiAgICAgIGNvbnN0IGJyZWFrcG9pbnRzSW5Ub2tlbiA9IHNvcnRlZC5maWx0ZXIoKGkpID0+IHRva2VuLm9mZnNldCA8IGkgJiYgaSA8IHRva2VuLm9mZnNldCArIHRva2VuLmNvbnRlbnQubGVuZ3RoKS5tYXAoKGkpID0+IGkgLSB0b2tlbi5vZmZzZXQpLnNvcnQoKGEsIGIpID0+IGEgLSBiKTtcbiAgICAgIGlmICghYnJlYWtwb2ludHNJblRva2VuLmxlbmd0aClcbiAgICAgICAgcmV0dXJuIHRva2VuO1xuICAgICAgcmV0dXJuIHNwbGl0VG9rZW4odG9rZW4sIGJyZWFrcG9pbnRzSW5Ub2tlbik7XG4gICAgfSk7XG4gIH0pO1xufVxuYXN5bmMgZnVuY3Rpb24gbm9ybWFsaXplR2V0dGVyKHApIHtcbiAgcmV0dXJuIFByb21pc2UucmVzb2x2ZSh0eXBlb2YgcCA9PT0gXCJmdW5jdGlvblwiID8gcCgpIDogcCkudGhlbigocikgPT4gci5kZWZhdWx0IHx8IHIpO1xufVxuZnVuY3Rpb24gcmVzb2x2ZUNvbG9yUmVwbGFjZW1lbnRzKHRoZW1lLCBvcHRpb25zKSB7XG4gIGNvbnN0IHJlcGxhY2VtZW50cyA9IHR5cGVvZiB0aGVtZSA9PT0gXCJzdHJpbmdcIiA/IHt9IDogeyAuLi50aGVtZS5jb2xvclJlcGxhY2VtZW50cyB9O1xuICBjb25zdCB0aGVtZU5hbWUgPSB0eXBlb2YgdGhlbWUgPT09IFwic3RyaW5nXCIgPyB0aGVtZSA6IHRoZW1lLm5hbWU7XG4gIGZvciAoY29uc3QgW2tleSwgdmFsdWVdIG9mIE9iamVjdC5lbnRyaWVzKG9wdGlvbnM/LmNvbG9yUmVwbGFjZW1lbnRzIHx8IHt9KSkge1xuICAgIGlmICh0eXBlb2YgdmFsdWUgPT09IFwic3RyaW5nXCIpXG4gICAgICByZXBsYWNlbWVudHNba2V5XSA9IHZhbHVlO1xuICAgIGVsc2UgaWYgKGtleSA9PT0gdGhlbWVOYW1lKVxuICAgICAgT2JqZWN0LmFzc2lnbihyZXBsYWNlbWVudHMsIHZhbHVlKTtcbiAgfVxuICByZXR1cm4gcmVwbGFjZW1lbnRzO1xufVxuZnVuY3Rpb24gYXBwbHlDb2xvclJlcGxhY2VtZW50cyhjb2xvciwgcmVwbGFjZW1lbnRzKSB7XG4gIGlmICghY29sb3IpXG4gICAgcmV0dXJuIGNvbG9yO1xuICByZXR1cm4gcmVwbGFjZW1lbnRzPy5bY29sb3I/LnRvTG93ZXJDYXNlKCldIHx8IGNvbG9yO1xufVxuZnVuY3Rpb24gZ2V0VG9rZW5TdHlsZU9iamVjdCh0b2tlbikge1xuICBjb25zdCBzdHlsZXMgPSB7fTtcbiAgaWYgKHRva2VuLmNvbG9yKVxuICAgIHN0eWxlcy5jb2xvciA9IHRva2VuLmNvbG9yO1xuICBpZiAodG9rZW4uYmdDb2xvcilcbiAgICBzdHlsZXNbXCJiYWNrZ3JvdW5kLWNvbG9yXCJdID0gdG9rZW4uYmdDb2xvcjtcbiAgaWYgKHRva2VuLmZvbnRTdHlsZSkge1xuICAgIGlmICh0b2tlbi5mb250U3R5bGUgJiBGb250U3R5bGUuSXRhbGljKVxuICAgICAgc3R5bGVzW1wiZm9udC1zdHlsZVwiXSA9IFwiaXRhbGljXCI7XG4gICAgaWYgKHRva2VuLmZvbnRTdHlsZSAmIEZvbnRTdHlsZS5Cb2xkKVxuICAgICAgc3R5bGVzW1wiZm9udC13ZWlnaHRcIl0gPSBcImJvbGRcIjtcbiAgICBpZiAodG9rZW4uZm9udFN0eWxlICYgRm9udFN0eWxlLlVuZGVybGluZSlcbiAgICAgIHN0eWxlc1tcInRleHQtZGVjb3JhdGlvblwiXSA9IFwidW5kZXJsaW5lXCI7XG4gIH1cbiAgcmV0dXJuIHN0eWxlcztcbn1cbmZ1bmN0aW9uIHN0cmluZ2lmeVRva2VuU3R5bGUodG9rZW4pIHtcbiAgaWYgKHR5cGVvZiB0b2tlbiA9PT0gXCJzdHJpbmdcIilcbiAgICByZXR1cm4gdG9rZW47XG4gIHJldHVybiBPYmplY3QuZW50cmllcyh0b2tlbikubWFwKChba2V5LCB2YWx1ZV0pID0+IGAke2tleX06JHt2YWx1ZX1gKS5qb2luKFwiO1wiKTtcbn1cbmZ1bmN0aW9uIGNyZWF0ZVBvc2l0aW9uQ29udmVydGVyKGNvZGUpIHtcbiAgY29uc3QgbGluZXMgPSBzcGxpdExpbmVzKGNvZGUsIHRydWUpLm1hcCgoW2xpbmVdKSA9PiBsaW5lKTtcbiAgZnVuY3Rpb24gaW5kZXhUb1BvcyhpbmRleCkge1xuICAgIGlmIChpbmRleCA9PT0gY29kZS5sZW5ndGgpIHtcbiAgICAgIHJldHVybiB7XG4gICAgICAgIGxpbmU6IGxpbmVzLmxlbmd0aCAtIDEsXG4gICAgICAgIGNoYXJhY3RlcjogbGluZXNbbGluZXMubGVuZ3RoIC0gMV0ubGVuZ3RoXG4gICAgICB9O1xuICAgIH1cbiAgICBsZXQgY2hhcmFjdGVyID0gaW5kZXg7XG4gICAgbGV0IGxpbmUgPSAwO1xuICAgIGZvciAoY29uc3QgbGluZVRleHQgb2YgbGluZXMpIHtcbiAgICAgIGlmIChjaGFyYWN0ZXIgPCBsaW5lVGV4dC5sZW5ndGgpXG4gICAgICAgIGJyZWFrO1xuICAgICAgY2hhcmFjdGVyIC09IGxpbmVUZXh0Lmxlbmd0aDtcbiAgICAgIGxpbmUrKztcbiAgICB9XG4gICAgcmV0dXJuIHsgbGluZSwgY2hhcmFjdGVyIH07XG4gIH1cbiAgZnVuY3Rpb24gcG9zVG9JbmRleChsaW5lLCBjaGFyYWN0ZXIpIHtcbiAgICBsZXQgaW5kZXggPSAwO1xuICAgIGZvciAobGV0IGkgPSAwOyBpIDwgbGluZTsgaSsrKVxuICAgICAgaW5kZXggKz0gbGluZXNbaV0ubGVuZ3RoO1xuICAgIGluZGV4ICs9IGNoYXJhY3RlcjtcbiAgICByZXR1cm4gaW5kZXg7XG4gIH1cbiAgcmV0dXJuIHtcbiAgICBsaW5lcyxcbiAgICBpbmRleFRvUG9zLFxuICAgIHBvc1RvSW5kZXhcbiAgfTtcbn1cblxuY2xhc3MgU2hpa2lFcnJvciBleHRlbmRzIEVycm9yIHtcbiAgY29uc3RydWN0b3IobWVzc2FnZSkge1xuICAgIHN1cGVyKG1lc3NhZ2UpO1xuICAgIHRoaXMubmFtZSA9IFwiU2hpa2lFcnJvclwiO1xuICB9XG59XG5cbmNvbnN0IF9ncmFtbWFyU3RhdGVNYXAgPSAvKiBAX19QVVJFX18gKi8gbmV3IFdlYWtNYXAoKTtcbmZ1bmN0aW9uIHNldExhc3RHcmFtbWFyU3RhdGVUb01hcChrZXlzLCBzdGF0ZSkge1xuICBfZ3JhbW1hclN0YXRlTWFwLnNldChrZXlzLCBzdGF0ZSk7XG59XG5mdW5jdGlvbiBnZXRMYXN0R3JhbW1hclN0YXRlRnJvbU1hcChrZXlzKSB7XG4gIHJldHVybiBfZ3JhbW1hclN0YXRlTWFwLmdldChrZXlzKTtcbn1cbmNsYXNzIEdyYW1tYXJTdGF0ZSB7XG4gIC8qKlxuICAgKiBUaGVtZSB0byBTdGFjayBtYXBwaW5nXG4gICAqL1xuICBfc3RhY2tzID0ge307XG4gIGxhbmc7XG4gIGdldCB0aGVtZXMoKSB7XG4gICAgcmV0dXJuIE9iamVjdC5rZXlzKHRoaXMuX3N0YWNrcyk7XG4gIH1cbiAgZ2V0IHRoZW1lKCkge1xuICAgIHJldHVybiB0aGlzLnRoZW1lc1swXTtcbiAgfVxuICBnZXQgX3N0YWNrKCkge1xuICAgIHJldHVybiB0aGlzLl9zdGFja3NbdGhpcy50aGVtZV07XG4gIH1cbiAgLyoqXG4gICAqIFN0YXRpYyBtZXRob2QgdG8gY3JlYXRlIGEgaW5pdGlhbCBncmFtbWFyIHN0YXRlLlxuICAgKi9cbiAgc3RhdGljIGluaXRpYWwobGFuZywgdGhlbWVzKSB7XG4gICAgcmV0dXJuIG5ldyBHcmFtbWFyU3RhdGUoXG4gICAgICBPYmplY3QuZnJvbUVudHJpZXModG9BcnJheSh0aGVtZXMpLm1hcCgodGhlbWUpID0+IFt0aGVtZSwgSU5JVElBTF0pKSxcbiAgICAgIGxhbmdcbiAgICApO1xuICB9XG4gIGNvbnN0cnVjdG9yKC4uLmFyZ3MpIHtcbiAgICBpZiAoYXJncy5sZW5ndGggPT09IDIpIHtcbiAgICAgIGNvbnN0IFtzdGFja3NNYXAsIGxhbmddID0gYXJncztcbiAgICAgIHRoaXMubGFuZyA9IGxhbmc7XG4gICAgICB0aGlzLl9zdGFja3MgPSBzdGFja3NNYXA7XG4gICAgfSBlbHNlIHtcbiAgICAgIGNvbnN0IFtzdGFjaywgbGFuZywgdGhlbWVdID0gYXJncztcbiAgICAgIHRoaXMubGFuZyA9IGxhbmc7XG4gICAgICB0aGlzLl9zdGFja3MgPSB7IFt0aGVtZV06IHN0YWNrIH07XG4gICAgfVxuICB9XG4gIC8qKlxuICAgKiBHZXQgdGhlIGludGVybmFsIHN0YWNrIG9iamVjdC5cbiAgICogQGludGVybmFsXG4gICAqL1xuICBnZXRJbnRlcm5hbFN0YWNrKHRoZW1lID0gdGhpcy50aGVtZSkge1xuICAgIHJldHVybiB0aGlzLl9zdGFja3NbdGhlbWVdO1xuICB9XG4gIC8qKlxuICAgKiBAZGVwcmVjYXRlZCB1c2UgYGdldFNjb3Blc2AgaW5zdGVhZFxuICAgKi9cbiAgZ2V0IHNjb3BlcygpIHtcbiAgICByZXR1cm4gZ2V0U2NvcGVzKHRoaXMuX3N0YWNrc1t0aGlzLnRoZW1lXSk7XG4gIH1cbiAgZ2V0U2NvcGVzKHRoZW1lID0gdGhpcy50aGVtZSkge1xuICAgIHJldHVybiBnZXRTY29wZXModGhpcy5fc3RhY2tzW3RoZW1lXSk7XG4gIH1cbiAgdG9KU09OKCkge1xuICAgIHJldHVybiB7XG4gICAgICBsYW5nOiB0aGlzLmxhbmcsXG4gICAgICB0aGVtZTogdGhpcy50aGVtZSxcbiAgICAgIHRoZW1lczogdGhpcy50aGVtZXMsXG4gICAgICBzY29wZXM6IHRoaXMuc2NvcGVzXG4gICAgfTtcbiAgfVxufVxuZnVuY3Rpb24gZ2V0U2NvcGVzKHN0YWNrKSB7XG4gIGNvbnN0IHNjb3BlcyA9IFtdO1xuICBjb25zdCB2aXNpdGVkID0gLyogQF9fUFVSRV9fICovIG5ldyBTZXQoKTtcbiAgZnVuY3Rpb24gcHVzaFNjb3BlKHN0YWNrMikge1xuICAgIGlmICh2aXNpdGVkLmhhcyhzdGFjazIpKVxuICAgICAgcmV0dXJuO1xuICAgIHZpc2l0ZWQuYWRkKHN0YWNrMik7XG4gICAgY29uc3QgbmFtZSA9IHN0YWNrMj8ubmFtZVNjb3Blc0xpc3Q/LnNjb3BlTmFtZTtcbiAgICBpZiAobmFtZSlcbiAgICAgIHNjb3Blcy5wdXNoKG5hbWUpO1xuICAgIGlmIChzdGFjazIucGFyZW50KVxuICAgICAgcHVzaFNjb3BlKHN0YWNrMi5wYXJlbnQpO1xuICB9XG4gIHB1c2hTY29wZShzdGFjayk7XG4gIHJldHVybiBzY29wZXM7XG59XG5mdW5jdGlvbiBnZXRHcmFtbWFyU3RhY2soc3RhdGUsIHRoZW1lKSB7XG4gIGlmICghKHN0YXRlIGluc3RhbmNlb2YgR3JhbW1hclN0YXRlKSlcbiAgICB0aHJvdyBuZXcgU2hpa2lFcnJvcihcIkludmFsaWQgZ3JhbW1hciBzdGF0ZVwiKTtcbiAgcmV0dXJuIHN0YXRlLmdldEludGVybmFsU3RhY2sodGhlbWUpO1xufVxuXG5mdW5jdGlvbiB0cmFuc2Zvcm1lckRlY29yYXRpb25zKCkge1xuICBjb25zdCBtYXAgPSAvKiBAX19QVVJFX18gKi8gbmV3IFdlYWtNYXAoKTtcbiAgZnVuY3Rpb24gZ2V0Q29udGV4dChzaGlraSkge1xuICAgIGlmICghbWFwLmhhcyhzaGlraS5tZXRhKSkge1xuICAgICAgbGV0IG5vcm1hbGl6ZVBvc2l0aW9uID0gZnVuY3Rpb24ocCkge1xuICAgICAgICBpZiAodHlwZW9mIHAgPT09IFwibnVtYmVyXCIpIHtcbiAgICAgICAgICBpZiAocCA8IDAgfHwgcCA+IHNoaWtpLnNvdXJjZS5sZW5ndGgpXG4gICAgICAgICAgICB0aHJvdyBuZXcgU2hpa2lFcnJvcihgSW52YWxpZCBkZWNvcmF0aW9uIG9mZnNldDogJHtwfS4gQ29kZSBsZW5ndGg6ICR7c2hpa2kuc291cmNlLmxlbmd0aH1gKTtcbiAgICAgICAgICByZXR1cm4ge1xuICAgICAgICAgICAgLi4uY29udmVydGVyLmluZGV4VG9Qb3MocCksXG4gICAgICAgICAgICBvZmZzZXQ6IHBcbiAgICAgICAgICB9O1xuICAgICAgICB9IGVsc2Uge1xuICAgICAgICAgIGNvbnN0IGxpbmUgPSBjb252ZXJ0ZXIubGluZXNbcC5saW5lXTtcbiAgICAgICAgICBpZiAobGluZSA9PT0gdm9pZCAwKVxuICAgICAgICAgICAgdGhyb3cgbmV3IFNoaWtpRXJyb3IoYEludmFsaWQgZGVjb3JhdGlvbiBwb3NpdGlvbiAke0pTT04uc3RyaW5naWZ5KHApfS4gTGluZXMgbGVuZ3RoOiAke2NvbnZlcnRlci5saW5lcy5sZW5ndGh9YCk7XG4gICAgICAgICAgaWYgKHAuY2hhcmFjdGVyIDwgMCB8fCBwLmNoYXJhY3RlciA+IGxpbmUubGVuZ3RoKVxuICAgICAgICAgICAgdGhyb3cgbmV3IFNoaWtpRXJyb3IoYEludmFsaWQgZGVjb3JhdGlvbiBwb3NpdGlvbiAke0pTT04uc3RyaW5naWZ5KHApfS4gTGluZSAke3AubGluZX0gbGVuZ3RoOiAke2xpbmUubGVuZ3RofWApO1xuICAgICAgICAgIHJldHVybiB7XG4gICAgICAgICAgICAuLi5wLFxuICAgICAgICAgICAgb2Zmc2V0OiBjb252ZXJ0ZXIucG9zVG9JbmRleChwLmxpbmUsIHAuY2hhcmFjdGVyKVxuICAgICAgICAgIH07XG4gICAgICAgIH1cbiAgICAgIH07XG4gICAgICBjb25zdCBjb252ZXJ0ZXIgPSBjcmVhdGVQb3NpdGlvbkNvbnZlcnRlcihzaGlraS5zb3VyY2UpO1xuICAgICAgY29uc3QgZGVjb3JhdGlvbnMgPSAoc2hpa2kub3B0aW9ucy5kZWNvcmF0aW9ucyB8fCBbXSkubWFwKChkKSA9PiAoe1xuICAgICAgICAuLi5kLFxuICAgICAgICBzdGFydDogbm9ybWFsaXplUG9zaXRpb24oZC5zdGFydCksXG4gICAgICAgIGVuZDogbm9ybWFsaXplUG9zaXRpb24oZC5lbmQpXG4gICAgICB9KSk7XG4gICAgICB2ZXJpZnlJbnRlcnNlY3Rpb25zKGRlY29yYXRpb25zKTtcbiAgICAgIG1hcC5zZXQoc2hpa2kubWV0YSwge1xuICAgICAgICBkZWNvcmF0aW9ucyxcbiAgICAgICAgY29udmVydGVyLFxuICAgICAgICBzb3VyY2U6IHNoaWtpLnNvdXJjZVxuICAgICAgfSk7XG4gICAgfVxuICAgIHJldHVybiBtYXAuZ2V0KHNoaWtpLm1ldGEpO1xuICB9XG4gIHJldHVybiB7XG4gICAgbmFtZTogXCJzaGlraTpkZWNvcmF0aW9uc1wiLFxuICAgIHRva2Vucyh0b2tlbnMpIHtcbiAgICAgIGlmICghdGhpcy5vcHRpb25zLmRlY29yYXRpb25zPy5sZW5ndGgpXG4gICAgICAgIHJldHVybjtcbiAgICAgIGNvbnN0IGN0eCA9IGdldENvbnRleHQodGhpcyk7XG4gICAgICBjb25zdCBicmVha3BvaW50cyA9IGN0eC5kZWNvcmF0aW9ucy5mbGF0TWFwKChkKSA9PiBbZC5zdGFydC5vZmZzZXQsIGQuZW5kLm9mZnNldF0pO1xuICAgICAgY29uc3Qgc3BsaXR0ZWQgPSBzcGxpdFRva2Vucyh0b2tlbnMsIGJyZWFrcG9pbnRzKTtcbiAgICAgIHJldHVybiBzcGxpdHRlZDtcbiAgICB9LFxuICAgIGNvZGUoY29kZUVsKSB7XG4gICAgICBpZiAoIXRoaXMub3B0aW9ucy5kZWNvcmF0aW9ucz8ubGVuZ3RoKVxuICAgICAgICByZXR1cm47XG4gICAgICBjb25zdCBjdHggPSBnZXRDb250ZXh0KHRoaXMpO1xuICAgICAgY29uc3QgbGluZXMgPSBBcnJheS5mcm9tKGNvZGVFbC5jaGlsZHJlbikuZmlsdGVyKChpKSA9PiBpLnR5cGUgPT09IFwiZWxlbWVudFwiICYmIGkudGFnTmFtZSA9PT0gXCJzcGFuXCIpO1xuICAgICAgaWYgKGxpbmVzLmxlbmd0aCAhPT0gY3R4LmNvbnZlcnRlci5saW5lcy5sZW5ndGgpXG4gICAgICAgIHRocm93IG5ldyBTaGlraUVycm9yKGBOdW1iZXIgb2YgbGluZXMgaW4gY29kZSBlbGVtZW50ICgke2xpbmVzLmxlbmd0aH0pIGRvZXMgbm90IG1hdGNoIHRoZSBudW1iZXIgb2YgbGluZXMgaW4gdGhlIHNvdXJjZSAoJHtjdHguY29udmVydGVyLmxpbmVzLmxlbmd0aH0pLiBGYWlsZWQgdG8gYXBwbHkgZGVjb3JhdGlvbnMuYCk7XG4gICAgICBmdW5jdGlvbiBhcHBseUxpbmVTZWN0aW9uKGxpbmUsIHN0YXJ0LCBlbmQsIGRlY29yYXRpb24pIHtcbiAgICAgICAgY29uc3QgbGluZUVsID0gbGluZXNbbGluZV07XG4gICAgICAgIGxldCB0ZXh0ID0gXCJcIjtcbiAgICAgICAgbGV0IHN0YXJ0SW5kZXggPSAtMTtcbiAgICAgICAgbGV0IGVuZEluZGV4ID0gLTE7XG4gICAgICAgIGlmIChzdGFydCA9PT0gMClcbiAgICAgICAgICBzdGFydEluZGV4ID0gMDtcbiAgICAgICAgaWYgKGVuZCA9PT0gMClcbiAgICAgICAgICBlbmRJbmRleCA9IDA7XG4gICAgICAgIGlmIChlbmQgPT09IE51bWJlci5QT1NJVElWRV9JTkZJTklUWSlcbiAgICAgICAgICBlbmRJbmRleCA9IGxpbmVFbC5jaGlsZHJlbi5sZW5ndGg7XG4gICAgICAgIGlmIChzdGFydEluZGV4ID09PSAtMSB8fCBlbmRJbmRleCA9PT0gLTEpIHtcbiAgICAgICAgICBmb3IgKGxldCBpID0gMDsgaSA8IGxpbmVFbC5jaGlsZHJlbi5sZW5ndGg7IGkrKykge1xuICAgICAgICAgICAgdGV4dCArPSBzdHJpbmdpZnkobGluZUVsLmNoaWxkcmVuW2ldKTtcbiAgICAgICAgICAgIGlmIChzdGFydEluZGV4ID09PSAtMSAmJiB0ZXh0Lmxlbmd0aCA9PT0gc3RhcnQpXG4gICAgICAgICAgICAgIHN0YXJ0SW5kZXggPSBpICsgMTtcbiAgICAgICAgICAgIGlmIChlbmRJbmRleCA9PT0gLTEgJiYgdGV4dC5sZW5ndGggPT09IGVuZClcbiAgICAgICAgICAgICAgZW5kSW5kZXggPSBpICsgMTtcbiAgICAgICAgICB9XG4gICAgICAgIH1cbiAgICAgICAgaWYgKHN0YXJ0SW5kZXggPT09IC0xKVxuICAgICAgICAgIHRocm93IG5ldyBTaGlraUVycm9yKGBGYWlsZWQgdG8gZmluZCBzdGFydCBpbmRleCBmb3IgZGVjb3JhdGlvbiAke0pTT04uc3RyaW5naWZ5KGRlY29yYXRpb24uc3RhcnQpfWApO1xuICAgICAgICBpZiAoZW5kSW5kZXggPT09IC0xKVxuICAgICAgICAgIHRocm93IG5ldyBTaGlraUVycm9yKGBGYWlsZWQgdG8gZmluZCBlbmQgaW5kZXggZm9yIGRlY29yYXRpb24gJHtKU09OLnN0cmluZ2lmeShkZWNvcmF0aW9uLmVuZCl9YCk7XG4gICAgICAgIGNvbnN0IGNoaWxkcmVuID0gbGluZUVsLmNoaWxkcmVuLnNsaWNlKHN0YXJ0SW5kZXgsIGVuZEluZGV4KTtcbiAgICAgICAgaWYgKCFkZWNvcmF0aW9uLmFsd2F5c1dyYXAgJiYgY2hpbGRyZW4ubGVuZ3RoID09PSBsaW5lRWwuY2hpbGRyZW4ubGVuZ3RoKSB7XG4gICAgICAgICAgYXBwbHlEZWNvcmF0aW9uKGxpbmVFbCwgZGVjb3JhdGlvbiwgXCJsaW5lXCIpO1xuICAgICAgICB9IGVsc2UgaWYgKCFkZWNvcmF0aW9uLmFsd2F5c1dyYXAgJiYgY2hpbGRyZW4ubGVuZ3RoID09PSAxICYmIGNoaWxkcmVuWzBdLnR5cGUgPT09IFwiZWxlbWVudFwiKSB7XG4gICAgICAgICAgYXBwbHlEZWNvcmF0aW9uKGNoaWxkcmVuWzBdLCBkZWNvcmF0aW9uLCBcInRva2VuXCIpO1xuICAgICAgICB9IGVsc2Uge1xuICAgICAgICAgIGNvbnN0IHdyYXBwZXIgPSB7XG4gICAgICAgICAgICB0eXBlOiBcImVsZW1lbnRcIixcbiAgICAgICAgICAgIHRhZ05hbWU6IFwic3BhblwiLFxuICAgICAgICAgICAgcHJvcGVydGllczoge30sXG4gICAgICAgICAgICBjaGlsZHJlblxuICAgICAgICAgIH07XG4gICAgICAgICAgYXBwbHlEZWNvcmF0aW9uKHdyYXBwZXIsIGRlY29yYXRpb24sIFwid3JhcHBlclwiKTtcbiAgICAgICAgICBsaW5lRWwuY2hpbGRyZW4uc3BsaWNlKHN0YXJ0SW5kZXgsIGNoaWxkcmVuLmxlbmd0aCwgd3JhcHBlcik7XG4gICAgICAgIH1cbiAgICAgIH1cbiAgICAgIGZ1bmN0aW9uIGFwcGx5TGluZShsaW5lLCBkZWNvcmF0aW9uKSB7XG4gICAgICAgIGxpbmVzW2xpbmVdID0gYXBwbHlEZWNvcmF0aW9uKGxpbmVzW2xpbmVdLCBkZWNvcmF0aW9uLCBcImxpbmVcIik7XG4gICAgICB9XG4gICAgICBmdW5jdGlvbiBhcHBseURlY29yYXRpb24oZWwsIGRlY29yYXRpb24sIHR5cGUpIHtcbiAgICAgICAgY29uc3QgcHJvcGVydGllcyA9IGRlY29yYXRpb24ucHJvcGVydGllcyB8fCB7fTtcbiAgICAgICAgY29uc3QgdHJhbnNmb3JtID0gZGVjb3JhdGlvbi50cmFuc2Zvcm0gfHwgKChpKSA9PiBpKTtcbiAgICAgICAgZWwudGFnTmFtZSA9IGRlY29yYXRpb24udGFnTmFtZSB8fCBcInNwYW5cIjtcbiAgICAgICAgZWwucHJvcGVydGllcyA9IHtcbiAgICAgICAgICAuLi5lbC5wcm9wZXJ0aWVzLFxuICAgICAgICAgIC4uLnByb3BlcnRpZXMsXG4gICAgICAgICAgY2xhc3M6IGVsLnByb3BlcnRpZXMuY2xhc3NcbiAgICAgICAgfTtcbiAgICAgICAgaWYgKGRlY29yYXRpb24ucHJvcGVydGllcz8uY2xhc3MpXG4gICAgICAgICAgYWRkQ2xhc3NUb0hhc3QoZWwsIGRlY29yYXRpb24ucHJvcGVydGllcy5jbGFzcyk7XG4gICAgICAgIGVsID0gdHJhbnNmb3JtKGVsLCB0eXBlKSB8fCBlbDtcbiAgICAgICAgcmV0dXJuIGVsO1xuICAgICAgfVxuICAgICAgY29uc3QgbGluZUFwcGxpZXMgPSBbXTtcbiAgICAgIGNvbnN0IHNvcnRlZCA9IGN0eC5kZWNvcmF0aW9ucy5zb3J0KChhLCBiKSA9PiBiLnN0YXJ0Lm9mZnNldCAtIGEuc3RhcnQub2Zmc2V0KTtcbiAgICAgIGZvciAoY29uc3QgZGVjb3JhdGlvbiBvZiBzb3J0ZWQpIHtcbiAgICAgICAgY29uc3QgeyBzdGFydCwgZW5kIH0gPSBkZWNvcmF0aW9uO1xuICAgICAgICBpZiAoc3RhcnQubGluZSA9PT0gZW5kLmxpbmUpIHtcbiAgICAgICAgICBhcHBseUxpbmVTZWN0aW9uKHN0YXJ0LmxpbmUsIHN0YXJ0LmNoYXJhY3RlciwgZW5kLmNoYXJhY3RlciwgZGVjb3JhdGlvbik7XG4gICAgICAgIH0gZWxzZSBpZiAoc3RhcnQubGluZSA8IGVuZC5saW5lKSB7XG4gICAgICAgICAgYXBwbHlMaW5lU2VjdGlvbihzdGFydC5saW5lLCBzdGFydC5jaGFyYWN0ZXIsIE51bWJlci5QT1NJVElWRV9JTkZJTklUWSwgZGVjb3JhdGlvbik7XG4gICAgICAgICAgZm9yIChsZXQgaSA9IHN0YXJ0LmxpbmUgKyAxOyBpIDwgZW5kLmxpbmU7IGkrKylcbiAgICAgICAgICAgIGxpbmVBcHBsaWVzLnVuc2hpZnQoKCkgPT4gYXBwbHlMaW5lKGksIGRlY29yYXRpb24pKTtcbiAgICAgICAgICBhcHBseUxpbmVTZWN0aW9uKGVuZC5saW5lLCAwLCBlbmQuY2hhcmFjdGVyLCBkZWNvcmF0aW9uKTtcbiAgICAgICAgfVxuICAgICAgfVxuICAgICAgbGluZUFwcGxpZXMuZm9yRWFjaCgoaSkgPT4gaSgpKTtcbiAgICB9XG4gIH07XG59XG5mdW5jdGlvbiB2ZXJpZnlJbnRlcnNlY3Rpb25zKGl0ZW1zKSB7XG4gIGZvciAobGV0IGkgPSAwOyBpIDwgaXRlbXMubGVuZ3RoOyBpKyspIHtcbiAgICBjb25zdCBmb28gPSBpdGVtc1tpXTtcbiAgICBpZiAoZm9vLnN0YXJ0Lm9mZnNldCA+IGZvby5lbmQub2Zmc2V0KVxuICAgICAgdGhyb3cgbmV3IFNoaWtpRXJyb3IoYEludmFsaWQgZGVjb3JhdGlvbiByYW5nZTogJHtKU09OLnN0cmluZ2lmeShmb28uc3RhcnQpfSAtICR7SlNPTi5zdHJpbmdpZnkoZm9vLmVuZCl9YCk7XG4gICAgZm9yIChsZXQgaiA9IGkgKyAxOyBqIDwgaXRlbXMubGVuZ3RoOyBqKyspIHtcbiAgICAgIGNvbnN0IGJhciA9IGl0ZW1zW2pdO1xuICAgICAgY29uc3QgaXNGb29IYXNCYXJTdGFydCA9IGZvby5zdGFydC5vZmZzZXQgPCBiYXIuc3RhcnQub2Zmc2V0ICYmIGJhci5zdGFydC5vZmZzZXQgPCBmb28uZW5kLm9mZnNldDtcbiAgICAgIGNvbnN0IGlzRm9vSGFzQmFyRW5kID0gZm9vLnN0YXJ0Lm9mZnNldCA8IGJhci5lbmQub2Zmc2V0ICYmIGJhci5lbmQub2Zmc2V0IDwgZm9vLmVuZC5vZmZzZXQ7XG4gICAgICBjb25zdCBpc0Jhckhhc0Zvb1N0YXJ0ID0gYmFyLnN0YXJ0Lm9mZnNldCA8IGZvby5zdGFydC5vZmZzZXQgJiYgZm9vLnN0YXJ0Lm9mZnNldCA8IGJhci5lbmQub2Zmc2V0O1xuICAgICAgY29uc3QgaXNCYXJIYXNGb29FbmQgPSBiYXIuc3RhcnQub2Zmc2V0IDwgZm9vLmVuZC5vZmZzZXQgJiYgZm9vLmVuZC5vZmZzZXQgPCBiYXIuZW5kLm9mZnNldDtcbiAgICAgIGlmIChpc0Zvb0hhc0JhclN0YXJ0IHx8IGlzRm9vSGFzQmFyRW5kIHx8IGlzQmFySGFzRm9vU3RhcnQgfHwgaXNCYXJIYXNGb29FbmQpIHtcbiAgICAgICAgaWYgKGlzRm9vSGFzQmFyRW5kICYmIGlzRm9vSGFzQmFyRW5kKVxuICAgICAgICAgIGNvbnRpbnVlO1xuICAgICAgICBpZiAoaXNCYXJIYXNGb29TdGFydCAmJiBpc0Jhckhhc0Zvb0VuZClcbiAgICAgICAgICBjb250aW51ZTtcbiAgICAgICAgdGhyb3cgbmV3IFNoaWtpRXJyb3IoYERlY29yYXRpb25zICR7SlNPTi5zdHJpbmdpZnkoZm9vLnN0YXJ0KX0gYW5kICR7SlNPTi5zdHJpbmdpZnkoYmFyLnN0YXJ0KX0gaW50ZXJzZWN0LmApO1xuICAgICAgfVxuICAgIH1cbiAgfVxufVxuZnVuY3Rpb24gc3RyaW5naWZ5KGVsKSB7XG4gIGlmIChlbC50eXBlID09PSBcInRleHRcIilcbiAgICByZXR1cm4gZWwudmFsdWU7XG4gIGlmIChlbC50eXBlID09PSBcImVsZW1lbnRcIilcbiAgICByZXR1cm4gZWwuY2hpbGRyZW4ubWFwKHN0cmluZ2lmeSkuam9pbihcIlwiKTtcbiAgcmV0dXJuIFwiXCI7XG59XG5cbmNvbnN0IGJ1aWx0SW5UcmFuc2Zvcm1lcnMgPSBbXG4gIC8qIEBfX1BVUkVfXyAqLyB0cmFuc2Zvcm1lckRlY29yYXRpb25zKClcbl07XG5mdW5jdGlvbiBnZXRUcmFuc2Zvcm1lcnMob3B0aW9ucykge1xuICByZXR1cm4gW1xuICAgIC4uLm9wdGlvbnMudHJhbnNmb3JtZXJzIHx8IFtdLFxuICAgIC4uLmJ1aWx0SW5UcmFuc2Zvcm1lcnNcbiAgXTtcbn1cblxuLy8gc3JjL2NvbG9ycy50c1xudmFyIG5hbWVkQ29sb3JzID0gW1xuICBcImJsYWNrXCIsXG4gIFwicmVkXCIsXG4gIFwiZ3JlZW5cIixcbiAgXCJ5ZWxsb3dcIixcbiAgXCJibHVlXCIsXG4gIFwibWFnZW50YVwiLFxuICBcImN5YW5cIixcbiAgXCJ3aGl0ZVwiLFxuICBcImJyaWdodEJsYWNrXCIsXG4gIFwiYnJpZ2h0UmVkXCIsXG4gIFwiYnJpZ2h0R3JlZW5cIixcbiAgXCJicmlnaHRZZWxsb3dcIixcbiAgXCJicmlnaHRCbHVlXCIsXG4gIFwiYnJpZ2h0TWFnZW50YVwiLFxuICBcImJyaWdodEN5YW5cIixcbiAgXCJicmlnaHRXaGl0ZVwiXG5dO1xuXG4vLyBzcmMvZGVjb3JhdGlvbnMudHNcbnZhciBkZWNvcmF0aW9ucyA9IHtcbiAgMTogXCJib2xkXCIsXG4gIDI6IFwiZGltXCIsXG4gIDM6IFwiaXRhbGljXCIsXG4gIDQ6IFwidW5kZXJsaW5lXCIsXG4gIDc6IFwicmV2ZXJzZVwiLFxuICA5OiBcInN0cmlrZXRocm91Z2hcIlxufTtcblxuLy8gc3JjL3BhcnNlci50c1xuZnVuY3Rpb24gZmluZFNlcXVlbmNlKHZhbHVlLCBwb3NpdGlvbikge1xuICBjb25zdCBuZXh0RXNjYXBlID0gdmFsdWUuaW5kZXhPZihcIlxceDFCW1wiLCBwb3NpdGlvbik7XG4gIGlmIChuZXh0RXNjYXBlICE9PSAtMSkge1xuICAgIGNvbnN0IG5leHRDbG9zZSA9IHZhbHVlLmluZGV4T2YoXCJtXCIsIG5leHRFc2NhcGUpO1xuICAgIHJldHVybiB7XG4gICAgICBzZXF1ZW5jZTogdmFsdWUuc3Vic3RyaW5nKG5leHRFc2NhcGUgKyAyLCBuZXh0Q2xvc2UpLnNwbGl0KFwiO1wiKSxcbiAgICAgIHN0YXJ0UG9zaXRpb246IG5leHRFc2NhcGUsXG4gICAgICBwb3NpdGlvbjogbmV4dENsb3NlICsgMVxuICAgIH07XG4gIH1cbiAgcmV0dXJuIHtcbiAgICBwb3NpdGlvbjogdmFsdWUubGVuZ3RoXG4gIH07XG59XG5mdW5jdGlvbiBwYXJzZUNvbG9yKHNlcXVlbmNlLCBpbmRleCkge1xuICBsZXQgb2Zmc2V0ID0gMTtcbiAgY29uc3QgY29sb3JNb2RlID0gc2VxdWVuY2VbaW5kZXggKyBvZmZzZXQrK107XG4gIGxldCBjb2xvcjtcbiAgaWYgKGNvbG9yTW9kZSA9PT0gXCIyXCIpIHtcbiAgICBjb25zdCByZ2IgPSBbXG4gICAgICBzZXF1ZW5jZVtpbmRleCArIG9mZnNldCsrXSxcbiAgICAgIHNlcXVlbmNlW2luZGV4ICsgb2Zmc2V0KytdLFxuICAgICAgc2VxdWVuY2VbaW5kZXggKyBvZmZzZXRdXG4gICAgXS5tYXAoKHgpID0+IE51bWJlci5wYXJzZUludCh4KSk7XG4gICAgaWYgKHJnYi5sZW5ndGggPT09IDMgJiYgIXJnYi5zb21lKCh4KSA9PiBOdW1iZXIuaXNOYU4oeCkpKSB7XG4gICAgICBjb2xvciA9IHtcbiAgICAgICAgdHlwZTogXCJyZ2JcIixcbiAgICAgICAgcmdiXG4gICAgICB9O1xuICAgIH1cbiAgfSBlbHNlIGlmIChjb2xvck1vZGUgPT09IFwiNVwiKSB7XG4gICAgY29uc3QgY29sb3JJbmRleCA9IE51bWJlci5wYXJzZUludChzZXF1ZW5jZVtpbmRleCArIG9mZnNldF0pO1xuICAgIGlmICghTnVtYmVyLmlzTmFOKGNvbG9ySW5kZXgpKSB7XG4gICAgICBjb2xvciA9IHsgdHlwZTogXCJ0YWJsZVwiLCBpbmRleDogTnVtYmVyKGNvbG9ySW5kZXgpIH07XG4gICAgfVxuICB9XG4gIHJldHVybiBbb2Zmc2V0LCBjb2xvcl07XG59XG5mdW5jdGlvbiBwYXJzZVNlcXVlbmNlKHNlcXVlbmNlKSB7XG4gIGNvbnN0IGNvbW1hbmRzID0gW107XG4gIGZvciAobGV0IGkgPSAwOyBpIDwgc2VxdWVuY2UubGVuZ3RoOyBpKyspIHtcbiAgICBjb25zdCBjb2RlID0gc2VxdWVuY2VbaV07XG4gICAgY29uc3QgY29kZUludCA9IE51bWJlci5wYXJzZUludChjb2RlKTtcbiAgICBpZiAoTnVtYmVyLmlzTmFOKGNvZGVJbnQpKVxuICAgICAgY29udGludWU7XG4gICAgaWYgKGNvZGVJbnQgPT09IDApIHtcbiAgICAgIGNvbW1hbmRzLnB1c2goeyB0eXBlOiBcInJlc2V0QWxsXCIgfSk7XG4gICAgfSBlbHNlIGlmIChjb2RlSW50IDw9IDkpIHtcbiAgICAgIGNvbnN0IGRlY29yYXRpb24gPSBkZWNvcmF0aW9uc1tjb2RlSW50XTtcbiAgICAgIGlmIChkZWNvcmF0aW9uKSB7XG4gICAgICAgIGNvbW1hbmRzLnB1c2goe1xuICAgICAgICAgIHR5cGU6IFwic2V0RGVjb3JhdGlvblwiLFxuICAgICAgICAgIHZhbHVlOiBkZWNvcmF0aW9uc1tjb2RlSW50XVxuICAgICAgICB9KTtcbiAgICAgIH1cbiAgICB9IGVsc2UgaWYgKGNvZGVJbnQgPD0gMjkpIHtcbiAgICAgIGNvbnN0IGRlY29yYXRpb24gPSBkZWNvcmF0aW9uc1tjb2RlSW50IC0gMjBdO1xuICAgICAgaWYgKGRlY29yYXRpb24pIHtcbiAgICAgICAgY29tbWFuZHMucHVzaCh7XG4gICAgICAgICAgdHlwZTogXCJyZXNldERlY29yYXRpb25cIixcbiAgICAgICAgICB2YWx1ZTogZGVjb3JhdGlvblxuICAgICAgICB9KTtcbiAgICAgIH1cbiAgICB9IGVsc2UgaWYgKGNvZGVJbnQgPD0gMzcpIHtcbiAgICAgIGNvbW1hbmRzLnB1c2goe1xuICAgICAgICB0eXBlOiBcInNldEZvcmVncm91bmRDb2xvclwiLFxuICAgICAgICB2YWx1ZTogeyB0eXBlOiBcIm5hbWVkXCIsIG5hbWU6IG5hbWVkQ29sb3JzW2NvZGVJbnQgLSAzMF0gfVxuICAgICAgfSk7XG4gICAgfSBlbHNlIGlmIChjb2RlSW50ID09PSAzOCkge1xuICAgICAgY29uc3QgW29mZnNldCwgY29sb3JdID0gcGFyc2VDb2xvcihzZXF1ZW5jZSwgaSk7XG4gICAgICBpZiAoY29sb3IpIHtcbiAgICAgICAgY29tbWFuZHMucHVzaCh7XG4gICAgICAgICAgdHlwZTogXCJzZXRGb3JlZ3JvdW5kQ29sb3JcIixcbiAgICAgICAgICB2YWx1ZTogY29sb3JcbiAgICAgICAgfSk7XG4gICAgICB9XG4gICAgICBpICs9IG9mZnNldDtcbiAgICB9IGVsc2UgaWYgKGNvZGVJbnQgPT09IDM5KSB7XG4gICAgICBjb21tYW5kcy5wdXNoKHtcbiAgICAgICAgdHlwZTogXCJyZXNldEZvcmVncm91bmRDb2xvclwiXG4gICAgICB9KTtcbiAgICB9IGVsc2UgaWYgKGNvZGVJbnQgPD0gNDcpIHtcbiAgICAgIGNvbW1hbmRzLnB1c2goe1xuICAgICAgICB0eXBlOiBcInNldEJhY2tncm91bmRDb2xvclwiLFxuICAgICAgICB2YWx1ZTogeyB0eXBlOiBcIm5hbWVkXCIsIG5hbWU6IG5hbWVkQ29sb3JzW2NvZGVJbnQgLSA0MF0gfVxuICAgICAgfSk7XG4gICAgfSBlbHNlIGlmIChjb2RlSW50ID09PSA0OCkge1xuICAgICAgY29uc3QgW29mZnNldCwgY29sb3JdID0gcGFyc2VDb2xvcihzZXF1ZW5jZSwgaSk7XG4gICAgICBpZiAoY29sb3IpIHtcbiAgICAgICAgY29tbWFuZHMucHVzaCh7XG4gICAgICAgICAgdHlwZTogXCJzZXRCYWNrZ3JvdW5kQ29sb3JcIixcbiAgICAgICAgICB2YWx1ZTogY29sb3JcbiAgICAgICAgfSk7XG4gICAgICB9XG4gICAgICBpICs9IG9mZnNldDtcbiAgICB9IGVsc2UgaWYgKGNvZGVJbnQgPT09IDQ5KSB7XG4gICAgICBjb21tYW5kcy5wdXNoKHtcbiAgICAgICAgdHlwZTogXCJyZXNldEJhY2tncm91bmRDb2xvclwiXG4gICAgICB9KTtcbiAgICB9IGVsc2UgaWYgKGNvZGVJbnQgPj0gOTAgJiYgY29kZUludCA8PSA5Nykge1xuICAgICAgY29tbWFuZHMucHVzaCh7XG4gICAgICAgIHR5cGU6IFwic2V0Rm9yZWdyb3VuZENvbG9yXCIsXG4gICAgICAgIHZhbHVlOiB7IHR5cGU6IFwibmFtZWRcIiwgbmFtZTogbmFtZWRDb2xvcnNbY29kZUludCAtIDkwICsgOF0gfVxuICAgICAgfSk7XG4gICAgfSBlbHNlIGlmIChjb2RlSW50ID49IDEwMCAmJiBjb2RlSW50IDw9IDEwNykge1xuICAgICAgY29tbWFuZHMucHVzaCh7XG4gICAgICAgIHR5cGU6IFwic2V0QmFja2dyb3VuZENvbG9yXCIsXG4gICAgICAgIHZhbHVlOiB7IHR5cGU6IFwibmFtZWRcIiwgbmFtZTogbmFtZWRDb2xvcnNbY29kZUludCAtIDEwMCArIDhdIH1cbiAgICAgIH0pO1xuICAgIH1cbiAgfVxuICByZXR1cm4gY29tbWFuZHM7XG59XG5mdW5jdGlvbiBjcmVhdGVBbnNpU2VxdWVuY2VQYXJzZXIoKSB7XG4gIGxldCBmb3JlZ3JvdW5kID0gbnVsbDtcbiAgbGV0IGJhY2tncm91bmQgPSBudWxsO1xuICBsZXQgZGVjb3JhdGlvbnMyID0gLyogQF9fUFVSRV9fICovIG5ldyBTZXQoKTtcbiAgcmV0dXJuIHtcbiAgICBwYXJzZSh2YWx1ZSkge1xuICAgICAgY29uc3QgdG9rZW5zID0gW107XG4gICAgICBsZXQgcG9zaXRpb24gPSAwO1xuICAgICAgZG8ge1xuICAgICAgICBjb25zdCBmaW5kUmVzdWx0ID0gZmluZFNlcXVlbmNlKHZhbHVlLCBwb3NpdGlvbik7XG4gICAgICAgIGNvbnN0IHRleHQgPSBmaW5kUmVzdWx0LnNlcXVlbmNlID8gdmFsdWUuc3Vic3RyaW5nKHBvc2l0aW9uLCBmaW5kUmVzdWx0LnN0YXJ0UG9zaXRpb24pIDogdmFsdWUuc3Vic3RyaW5nKHBvc2l0aW9uKTtcbiAgICAgICAgaWYgKHRleHQubGVuZ3RoID4gMCkge1xuICAgICAgICAgIHRva2Vucy5wdXNoKHtcbiAgICAgICAgICAgIHZhbHVlOiB0ZXh0LFxuICAgICAgICAgICAgZm9yZWdyb3VuZCxcbiAgICAgICAgICAgIGJhY2tncm91bmQsXG4gICAgICAgICAgICBkZWNvcmF0aW9uczogbmV3IFNldChkZWNvcmF0aW9uczIpXG4gICAgICAgICAgfSk7XG4gICAgICAgIH1cbiAgICAgICAgaWYgKGZpbmRSZXN1bHQuc2VxdWVuY2UpIHtcbiAgICAgICAgICBjb25zdCBjb21tYW5kcyA9IHBhcnNlU2VxdWVuY2UoZmluZFJlc3VsdC5zZXF1ZW5jZSk7XG4gICAgICAgICAgZm9yIChjb25zdCBzdHlsZVRva2VuIG9mIGNvbW1hbmRzKSB7XG4gICAgICAgICAgICBpZiAoc3R5bGVUb2tlbi50eXBlID09PSBcInJlc2V0QWxsXCIpIHtcbiAgICAgICAgICAgICAgZm9yZWdyb3VuZCA9IG51bGw7XG4gICAgICAgICAgICAgIGJhY2tncm91bmQgPSBudWxsO1xuICAgICAgICAgICAgICBkZWNvcmF0aW9uczIuY2xlYXIoKTtcbiAgICAgICAgICAgIH0gZWxzZSBpZiAoc3R5bGVUb2tlbi50eXBlID09PSBcInJlc2V0Rm9yZWdyb3VuZENvbG9yXCIpIHtcbiAgICAgICAgICAgICAgZm9yZWdyb3VuZCA9IG51bGw7XG4gICAgICAgICAgICB9IGVsc2UgaWYgKHN0eWxlVG9rZW4udHlwZSA9PT0gXCJyZXNldEJhY2tncm91bmRDb2xvclwiKSB7XG4gICAgICAgICAgICAgIGJhY2tncm91bmQgPSBudWxsO1xuICAgICAgICAgICAgfSBlbHNlIGlmIChzdHlsZVRva2VuLnR5cGUgPT09IFwicmVzZXREZWNvcmF0aW9uXCIpIHtcbiAgICAgICAgICAgICAgZGVjb3JhdGlvbnMyLmRlbGV0ZShzdHlsZVRva2VuLnZhbHVlKTtcbiAgICAgICAgICAgIH1cbiAgICAgICAgICB9XG4gICAgICAgICAgZm9yIChjb25zdCBzdHlsZVRva2VuIG9mIGNvbW1hbmRzKSB7XG4gICAgICAgICAgICBpZiAoc3R5bGVUb2tlbi50eXBlID09PSBcInNldEZvcmVncm91bmRDb2xvclwiKSB7XG4gICAgICAgICAgICAgIGZvcmVncm91bmQgPSBzdHlsZVRva2VuLnZhbHVlO1xuICAgICAgICAgICAgfSBlbHNlIGlmIChzdHlsZVRva2VuLnR5cGUgPT09IFwic2V0QmFja2dyb3VuZENvbG9yXCIpIHtcbiAgICAgICAgICAgICAgYmFja2dyb3VuZCA9IHN0eWxlVG9rZW4udmFsdWU7XG4gICAgICAgICAgICB9IGVsc2UgaWYgKHN0eWxlVG9rZW4udHlwZSA9PT0gXCJzZXREZWNvcmF0aW9uXCIpIHtcbiAgICAgICAgICAgICAgZGVjb3JhdGlvbnMyLmFkZChzdHlsZVRva2VuLnZhbHVlKTtcbiAgICAgICAgICAgIH1cbiAgICAgICAgICB9XG4gICAgICAgIH1cbiAgICAgICAgcG9zaXRpb24gPSBmaW5kUmVzdWx0LnBvc2l0aW9uO1xuICAgICAgfSB3aGlsZSAocG9zaXRpb24gPCB2YWx1ZS5sZW5ndGgpO1xuICAgICAgcmV0dXJuIHRva2VucztcbiAgICB9XG4gIH07XG59XG5cbi8vIHNyYy9wYWxldHRlLnRzXG52YXIgZGVmYXVsdE5hbWVkQ29sb3JzTWFwID0ge1xuICBibGFjazogXCIjMDAwMDAwXCIsXG4gIHJlZDogXCIjYmIwMDAwXCIsXG4gIGdyZWVuOiBcIiMwMGJiMDBcIixcbiAgeWVsbG93OiBcIiNiYmJiMDBcIixcbiAgYmx1ZTogXCIjMDAwMGJiXCIsXG4gIG1hZ2VudGE6IFwiI2ZmMDBmZlwiLFxuICBjeWFuOiBcIiMwMGJiYmJcIixcbiAgd2hpdGU6IFwiI2VlZWVlZVwiLFxuICBicmlnaHRCbGFjazogXCIjNTU1NTU1XCIsXG4gIGJyaWdodFJlZDogXCIjZmY1NTU1XCIsXG4gIGJyaWdodEdyZWVuOiBcIiMwMGZmMDBcIixcbiAgYnJpZ2h0WWVsbG93OiBcIiNmZmZmNTVcIixcbiAgYnJpZ2h0Qmx1ZTogXCIjNTU1NWZmXCIsXG4gIGJyaWdodE1hZ2VudGE6IFwiI2ZmNTVmZlwiLFxuICBicmlnaHRDeWFuOiBcIiM1NWZmZmZcIixcbiAgYnJpZ2h0V2hpdGU6IFwiI2ZmZmZmZlwiXG59O1xuZnVuY3Rpb24gY3JlYXRlQ29sb3JQYWxldHRlKG5hbWVkQ29sb3JzTWFwID0gZGVmYXVsdE5hbWVkQ29sb3JzTWFwKSB7XG4gIGZ1bmN0aW9uIG5hbWVkQ29sb3IobmFtZSkge1xuICAgIHJldHVybiBuYW1lZENvbG9yc01hcFtuYW1lXTtcbiAgfVxuICBmdW5jdGlvbiByZ2JDb2xvcihyZ2IpIHtcbiAgICByZXR1cm4gYCMke3JnYi5tYXAoKHgpID0+IE1hdGgubWF4KDAsIE1hdGgubWluKHgsIDI1NSkpLnRvU3RyaW5nKDE2KS5wYWRTdGFydCgyLCBcIjBcIikpLmpvaW4oXCJcIil9YDtcbiAgfVxuICBsZXQgY29sb3JUYWJsZTtcbiAgZnVuY3Rpb24gZ2V0Q29sb3JUYWJsZSgpIHtcbiAgICBpZiAoY29sb3JUYWJsZSkge1xuICAgICAgcmV0dXJuIGNvbG9yVGFibGU7XG4gICAgfVxuICAgIGNvbG9yVGFibGUgPSBbXTtcbiAgICBmb3IgKGxldCBpID0gMDsgaSA8IG5hbWVkQ29sb3JzLmxlbmd0aDsgaSsrKSB7XG4gICAgICBjb2xvclRhYmxlLnB1c2gobmFtZWRDb2xvcihuYW1lZENvbG9yc1tpXSkpO1xuICAgIH1cbiAgICBsZXQgbGV2ZWxzID0gWzAsIDk1LCAxMzUsIDE3NSwgMjE1LCAyNTVdO1xuICAgIGZvciAobGV0IHIgPSAwOyByIDwgNjsgcisrKSB7XG4gICAgICBmb3IgKGxldCBnID0gMDsgZyA8IDY7IGcrKykge1xuICAgICAgICBmb3IgKGxldCBiID0gMDsgYiA8IDY7IGIrKykge1xuICAgICAgICAgIGNvbG9yVGFibGUucHVzaChyZ2JDb2xvcihbbGV2ZWxzW3JdLCBsZXZlbHNbZ10sIGxldmVsc1tiXV0pKTtcbiAgICAgICAgfVxuICAgICAgfVxuICAgIH1cbiAgICBsZXQgbGV2ZWwgPSA4O1xuICAgIGZvciAobGV0IGkgPSAwOyBpIDwgMjQ7IGkrKywgbGV2ZWwgKz0gMTApIHtcbiAgICAgIGNvbG9yVGFibGUucHVzaChyZ2JDb2xvcihbbGV2ZWwsIGxldmVsLCBsZXZlbF0pKTtcbiAgICB9XG4gICAgcmV0dXJuIGNvbG9yVGFibGU7XG4gIH1cbiAgZnVuY3Rpb24gdGFibGVDb2xvcihpbmRleCkge1xuICAgIHJldHVybiBnZXRDb2xvclRhYmxlKClbaW5kZXhdO1xuICB9XG4gIGZ1bmN0aW9uIHZhbHVlKGNvbG9yKSB7XG4gICAgc3dpdGNoIChjb2xvci50eXBlKSB7XG4gICAgICBjYXNlIFwibmFtZWRcIjpcbiAgICAgICAgcmV0dXJuIG5hbWVkQ29sb3IoY29sb3IubmFtZSk7XG4gICAgICBjYXNlIFwicmdiXCI6XG4gICAgICAgIHJldHVybiByZ2JDb2xvcihjb2xvci5yZ2IpO1xuICAgICAgY2FzZSBcInRhYmxlXCI6XG4gICAgICAgIHJldHVybiB0YWJsZUNvbG9yKGNvbG9yLmluZGV4KTtcbiAgICB9XG4gIH1cbiAgcmV0dXJuIHtcbiAgICB2YWx1ZVxuICB9O1xufVxuXG5mdW5jdGlvbiB0b2tlbml6ZUFuc2lXaXRoVGhlbWUodGhlbWUsIGZpbGVDb250ZW50cywgb3B0aW9ucykge1xuICBjb25zdCBjb2xvclJlcGxhY2VtZW50cyA9IHJlc29sdmVDb2xvclJlcGxhY2VtZW50cyh0aGVtZSwgb3B0aW9ucyk7XG4gIGNvbnN0IGxpbmVzID0gc3BsaXRMaW5lcyhmaWxlQ29udGVudHMpO1xuICBjb25zdCBjb2xvclBhbGV0dGUgPSBjcmVhdGVDb2xvclBhbGV0dGUoXG4gICAgT2JqZWN0LmZyb21FbnRyaWVzKFxuICAgICAgbmFtZWRDb2xvcnMubWFwKChuYW1lKSA9PiBbXG4gICAgICAgIG5hbWUsXG4gICAgICAgIHRoZW1lLmNvbG9ycz8uW2B0ZXJtaW5hbC5hbnNpJHtuYW1lWzBdLnRvVXBwZXJDYXNlKCl9JHtuYW1lLnN1YnN0cmluZygxKX1gXVxuICAgICAgXSlcbiAgICApXG4gICk7XG4gIGNvbnN0IHBhcnNlciA9IGNyZWF0ZUFuc2lTZXF1ZW5jZVBhcnNlcigpO1xuICByZXR1cm4gbGluZXMubWFwKFxuICAgIChsaW5lKSA9PiBwYXJzZXIucGFyc2UobGluZVswXSkubWFwKCh0b2tlbikgPT4ge1xuICAgICAgbGV0IGNvbG9yO1xuICAgICAgbGV0IGJnQ29sb3I7XG4gICAgICBpZiAodG9rZW4uZGVjb3JhdGlvbnMuaGFzKFwicmV2ZXJzZVwiKSkge1xuICAgICAgICBjb2xvciA9IHRva2VuLmJhY2tncm91bmQgPyBjb2xvclBhbGV0dGUudmFsdWUodG9rZW4uYmFja2dyb3VuZCkgOiB0aGVtZS5iZztcbiAgICAgICAgYmdDb2xvciA9IHRva2VuLmZvcmVncm91bmQgPyBjb2xvclBhbGV0dGUudmFsdWUodG9rZW4uZm9yZWdyb3VuZCkgOiB0aGVtZS5mZztcbiAgICAgIH0gZWxzZSB7XG4gICAgICAgIGNvbG9yID0gdG9rZW4uZm9yZWdyb3VuZCA/IGNvbG9yUGFsZXR0ZS52YWx1ZSh0b2tlbi5mb3JlZ3JvdW5kKSA6IHRoZW1lLmZnO1xuICAgICAgICBiZ0NvbG9yID0gdG9rZW4uYmFja2dyb3VuZCA/IGNvbG9yUGFsZXR0ZS52YWx1ZSh0b2tlbi5iYWNrZ3JvdW5kKSA6IHZvaWQgMDtcbiAgICAgIH1cbiAgICAgIGNvbG9yID0gYXBwbHlDb2xvclJlcGxhY2VtZW50cyhjb2xvciwgY29sb3JSZXBsYWNlbWVudHMpO1xuICAgICAgYmdDb2xvciA9IGFwcGx5Q29sb3JSZXBsYWNlbWVudHMoYmdDb2xvciwgY29sb3JSZXBsYWNlbWVudHMpO1xuICAgICAgaWYgKHRva2VuLmRlY29yYXRpb25zLmhhcyhcImRpbVwiKSlcbiAgICAgICAgY29sb3IgPSBkaW1Db2xvcihjb2xvcik7XG4gICAgICBsZXQgZm9udFN0eWxlID0gRm9udFN0eWxlLk5vbmU7XG4gICAgICBpZiAodG9rZW4uZGVjb3JhdGlvbnMuaGFzKFwiYm9sZFwiKSlcbiAgICAgICAgZm9udFN0eWxlIHw9IEZvbnRTdHlsZS5Cb2xkO1xuICAgICAgaWYgKHRva2VuLmRlY29yYXRpb25zLmhhcyhcIml0YWxpY1wiKSlcbiAgICAgICAgZm9udFN0eWxlIHw9IEZvbnRTdHlsZS5JdGFsaWM7XG4gICAgICBpZiAodG9rZW4uZGVjb3JhdGlvbnMuaGFzKFwidW5kZXJsaW5lXCIpKVxuICAgICAgICBmb250U3R5bGUgfD0gRm9udFN0eWxlLlVuZGVybGluZTtcbiAgICAgIHJldHVybiB7XG4gICAgICAgIGNvbnRlbnQ6IHRva2VuLnZhbHVlLFxuICAgICAgICBvZmZzZXQ6IGxpbmVbMV0sXG4gICAgICAgIC8vIFRPRE86IG1vcmUgYWNjdXJhdGUgb2Zmc2V0PyBtaWdodCBuZWVkIHRvIGZvcmsgYW5zaS1zZXF1ZW5jZS1wYXJzZXJcbiAgICAgICAgY29sb3IsXG4gICAgICAgIGJnQ29sb3IsXG4gICAgICAgIGZvbnRTdHlsZVxuICAgICAgfTtcbiAgICB9KVxuICApO1xufVxuZnVuY3Rpb24gZGltQ29sb3IoY29sb3IpIHtcbiAgY29uc3QgaGV4TWF0Y2ggPSBjb2xvci5tYXRjaCgvIyhbMC05YS1mXXszfSkoWzAtOWEtZl17M30pPyhbMC05YS1mXXsyfSk/Lyk7XG4gIGlmIChoZXhNYXRjaCkge1xuICAgIGlmIChoZXhNYXRjaFszXSkge1xuICAgICAgY29uc3QgYWxwaGEgPSBNYXRoLnJvdW5kKE51bWJlci5wYXJzZUludChoZXhNYXRjaFszXSwgMTYpIC8gMikudG9TdHJpbmcoMTYpLnBhZFN0YXJ0KDIsIFwiMFwiKTtcbiAgICAgIHJldHVybiBgIyR7aGV4TWF0Y2hbMV19JHtoZXhNYXRjaFsyXX0ke2FscGhhfWA7XG4gICAgfSBlbHNlIGlmIChoZXhNYXRjaFsyXSkge1xuICAgICAgcmV0dXJuIGAjJHtoZXhNYXRjaFsxXX0ke2hleE1hdGNoWzJdfTgwYDtcbiAgICB9IGVsc2Uge1xuICAgICAgcmV0dXJuIGAjJHtBcnJheS5mcm9tKGhleE1hdGNoWzFdKS5tYXAoKHgpID0+IGAke3h9JHt4fWApLmpvaW4oXCJcIil9ODBgO1xuICAgIH1cbiAgfVxuICBjb25zdCBjc3NWYXJNYXRjaCA9IGNvbG9yLm1hdGNoKC92YXJcXCgoLS1bXFx3LV0rLWFuc2ktW1xcdy1dKylcXCkvKTtcbiAgaWYgKGNzc1Zhck1hdGNoKVxuICAgIHJldHVybiBgdmFyKCR7Y3NzVmFyTWF0Y2hbMV19LWRpbSlgO1xuICByZXR1cm4gY29sb3I7XG59XG5cbmZ1bmN0aW9uIGNvZGVUb1Rva2Vuc0Jhc2UoaW50ZXJuYWwsIGNvZGUsIG9wdGlvbnMgPSB7fSkge1xuICBjb25zdCB7XG4gICAgbGFuZyA9IFwidGV4dFwiLFxuICAgIHRoZW1lOiB0aGVtZU5hbWUgPSBpbnRlcm5hbC5nZXRMb2FkZWRUaGVtZXMoKVswXVxuICB9ID0gb3B0aW9ucztcbiAgaWYgKGlzUGxhaW5MYW5nKGxhbmcpIHx8IGlzTm9uZVRoZW1lKHRoZW1lTmFtZSkpXG4gICAgcmV0dXJuIHNwbGl0TGluZXMoY29kZSkubWFwKChsaW5lKSA9PiBbeyBjb250ZW50OiBsaW5lWzBdLCBvZmZzZXQ6IGxpbmVbMV0gfV0pO1xuICBjb25zdCB7IHRoZW1lLCBjb2xvck1hcCB9ID0gaW50ZXJuYWwuc2V0VGhlbWUodGhlbWVOYW1lKTtcbiAgaWYgKGxhbmcgPT09IFwiYW5zaVwiKVxuICAgIHJldHVybiB0b2tlbml6ZUFuc2lXaXRoVGhlbWUodGhlbWUsIGNvZGUsIG9wdGlvbnMpO1xuICBjb25zdCBfZ3JhbW1hciA9IGludGVybmFsLmdldExhbmd1YWdlKGxhbmcpO1xuICBpZiAob3B0aW9ucy5ncmFtbWFyU3RhdGUpIHtcbiAgICBpZiAob3B0aW9ucy5ncmFtbWFyU3RhdGUubGFuZyAhPT0gX2dyYW1tYXIubmFtZSkge1xuICAgICAgdGhyb3cgbmV3IFNoaWtpRXJyb3IkMShgR3JhbW1hciBzdGF0ZSBsYW5ndWFnZSBcIiR7b3B0aW9ucy5ncmFtbWFyU3RhdGUubGFuZ31cIiBkb2VzIG5vdCBtYXRjaCBoaWdobGlnaHQgbGFuZ3VhZ2UgXCIke19ncmFtbWFyLm5hbWV9XCJgKTtcbiAgICB9XG4gICAgaWYgKCFvcHRpb25zLmdyYW1tYXJTdGF0ZS50aGVtZXMuaW5jbHVkZXModGhlbWUubmFtZSkpIHtcbiAgICAgIHRocm93IG5ldyBTaGlraUVycm9yJDEoYEdyYW1tYXIgc3RhdGUgdGhlbWVzIFwiJHtvcHRpb25zLmdyYW1tYXJTdGF0ZS50aGVtZXN9XCIgZG8gbm90IGNvbnRhaW4gaGlnaGxpZ2h0IHRoZW1lIFwiJHt0aGVtZS5uYW1lfVwiYCk7XG4gICAgfVxuICB9XG4gIHJldHVybiB0b2tlbml6ZVdpdGhUaGVtZShjb2RlLCBfZ3JhbW1hciwgdGhlbWUsIGNvbG9yTWFwLCBvcHRpb25zKTtcbn1cbmZ1bmN0aW9uIGdldExhc3RHcmFtbWFyU3RhdGUoLi4uYXJncykge1xuICBpZiAoYXJncy5sZW5ndGggPT09IDIpIHtcbiAgICByZXR1cm4gZ2V0TGFzdEdyYW1tYXJTdGF0ZUZyb21NYXAoYXJnc1sxXSk7XG4gIH1cbiAgY29uc3QgW2ludGVybmFsLCBjb2RlLCBvcHRpb25zID0ge31dID0gYXJncztcbiAgY29uc3Qge1xuICAgIGxhbmcgPSBcInRleHRcIixcbiAgICB0aGVtZTogdGhlbWVOYW1lID0gaW50ZXJuYWwuZ2V0TG9hZGVkVGhlbWVzKClbMF1cbiAgfSA9IG9wdGlvbnM7XG4gIGlmIChpc1BsYWluTGFuZyhsYW5nKSB8fCBpc05vbmVUaGVtZSh0aGVtZU5hbWUpKVxuICAgIHRocm93IG5ldyBTaGlraUVycm9yJDEoXCJQbGFpbiBsYW5ndWFnZSBkb2VzIG5vdCBoYXZlIGdyYW1tYXIgc3RhdGVcIik7XG4gIGlmIChsYW5nID09PSBcImFuc2lcIilcbiAgICB0aHJvdyBuZXcgU2hpa2lFcnJvciQxKFwiQU5TSSBsYW5ndWFnZSBkb2VzIG5vdCBoYXZlIGdyYW1tYXIgc3RhdGVcIik7XG4gIGNvbnN0IHsgdGhlbWUsIGNvbG9yTWFwIH0gPSBpbnRlcm5hbC5zZXRUaGVtZSh0aGVtZU5hbWUpO1xuICBjb25zdCBfZ3JhbW1hciA9IGludGVybmFsLmdldExhbmd1YWdlKGxhbmcpO1xuICByZXR1cm4gbmV3IEdyYW1tYXJTdGF0ZShcbiAgICBfdG9rZW5pemVXaXRoVGhlbWUoY29kZSwgX2dyYW1tYXIsIHRoZW1lLCBjb2xvck1hcCwgb3B0aW9ucykuc3RhdGVTdGFjayxcbiAgICBfZ3JhbW1hci5uYW1lLFxuICAgIHRoZW1lLm5hbWVcbiAgKTtcbn1cbmZ1bmN0aW9uIHRva2VuaXplV2l0aFRoZW1lKGNvZGUsIGdyYW1tYXIsIHRoZW1lLCBjb2xvck1hcCwgb3B0aW9ucykge1xuICBjb25zdCByZXN1bHQgPSBfdG9rZW5pemVXaXRoVGhlbWUoY29kZSwgZ3JhbW1hciwgdGhlbWUsIGNvbG9yTWFwLCBvcHRpb25zKTtcbiAgY29uc3QgZ3JhbW1hclN0YXRlID0gbmV3IEdyYW1tYXJTdGF0ZShcbiAgICBfdG9rZW5pemVXaXRoVGhlbWUoY29kZSwgZ3JhbW1hciwgdGhlbWUsIGNvbG9yTWFwLCBvcHRpb25zKS5zdGF0ZVN0YWNrLFxuICAgIGdyYW1tYXIubmFtZSxcbiAgICB0aGVtZS5uYW1lXG4gICk7XG4gIHNldExhc3RHcmFtbWFyU3RhdGVUb01hcChyZXN1bHQudG9rZW5zLCBncmFtbWFyU3RhdGUpO1xuICByZXR1cm4gcmVzdWx0LnRva2Vucztcbn1cbmZ1bmN0aW9uIF90b2tlbml6ZVdpdGhUaGVtZShjb2RlLCBncmFtbWFyLCB0aGVtZSwgY29sb3JNYXAsIG9wdGlvbnMpIHtcbiAgY29uc3QgY29sb3JSZXBsYWNlbWVudHMgPSByZXNvbHZlQ29sb3JSZXBsYWNlbWVudHModGhlbWUsIG9wdGlvbnMpO1xuICBjb25zdCB7XG4gICAgdG9rZW5pemVNYXhMaW5lTGVuZ3RoID0gMCxcbiAgICB0b2tlbml6ZVRpbWVMaW1pdCA9IDUwMFxuICB9ID0gb3B0aW9ucztcbiAgY29uc3QgbGluZXMgPSBzcGxpdExpbmVzKGNvZGUpO1xuICBsZXQgc3RhdGVTdGFjayA9IG9wdGlvbnMuZ3JhbW1hclN0YXRlID8gZ2V0R3JhbW1hclN0YWNrKG9wdGlvbnMuZ3JhbW1hclN0YXRlLCB0aGVtZS5uYW1lKSA/PyBJTklUSUFMIDogb3B0aW9ucy5ncmFtbWFyQ29udGV4dENvZGUgIT0gbnVsbCA/IF90b2tlbml6ZVdpdGhUaGVtZShcbiAgICBvcHRpb25zLmdyYW1tYXJDb250ZXh0Q29kZSxcbiAgICBncmFtbWFyLFxuICAgIHRoZW1lLFxuICAgIGNvbG9yTWFwLFxuICAgIHtcbiAgICAgIC4uLm9wdGlvbnMsXG4gICAgICBncmFtbWFyU3RhdGU6IHZvaWQgMCxcbiAgICAgIGdyYW1tYXJDb250ZXh0Q29kZTogdm9pZCAwXG4gICAgfVxuICApLnN0YXRlU3RhY2sgOiBJTklUSUFMO1xuICBsZXQgYWN0dWFsID0gW107XG4gIGNvbnN0IGZpbmFsID0gW107XG4gIGZvciAobGV0IGkgPSAwLCBsZW4gPSBsaW5lcy5sZW5ndGg7IGkgPCBsZW47IGkrKykge1xuICAgIGNvbnN0IFtsaW5lLCBsaW5lT2Zmc2V0XSA9IGxpbmVzW2ldO1xuICAgIGlmIChsaW5lID09PSBcIlwiKSB7XG4gICAgICBhY3R1YWwgPSBbXTtcbiAgICAgIGZpbmFsLnB1c2goW10pO1xuICAgICAgY29udGludWU7XG4gICAgfVxuICAgIGlmICh0b2tlbml6ZU1heExpbmVMZW5ndGggPiAwICYmIGxpbmUubGVuZ3RoID49IHRva2VuaXplTWF4TGluZUxlbmd0aCkge1xuICAgICAgYWN0dWFsID0gW107XG4gICAgICBmaW5hbC5wdXNoKFt7XG4gICAgICAgIGNvbnRlbnQ6IGxpbmUsXG4gICAgICAgIG9mZnNldDogbGluZU9mZnNldCxcbiAgICAgICAgY29sb3I6IFwiXCIsXG4gICAgICAgIGZvbnRTdHlsZTogMFxuICAgICAgfV0pO1xuICAgICAgY29udGludWU7XG4gICAgfVxuICAgIGxldCByZXN1bHRXaXRoU2NvcGVzO1xuICAgIGxldCB0b2tlbnNXaXRoU2NvcGVzO1xuICAgIGxldCB0b2tlbnNXaXRoU2NvcGVzSW5kZXg7XG4gICAgaWYgKG9wdGlvbnMuaW5jbHVkZUV4cGxhbmF0aW9uKSB7XG4gICAgICByZXN1bHRXaXRoU2NvcGVzID0gZ3JhbW1hci50b2tlbml6ZUxpbmUobGluZSwgc3RhdGVTdGFjayk7XG4gICAgICB0b2tlbnNXaXRoU2NvcGVzID0gcmVzdWx0V2l0aFNjb3Blcy50b2tlbnM7XG4gICAgICB0b2tlbnNXaXRoU2NvcGVzSW5kZXggPSAwO1xuICAgIH1cbiAgICBjb25zdCByZXN1bHQgPSBncmFtbWFyLnRva2VuaXplTGluZTIobGluZSwgc3RhdGVTdGFjaywgdG9rZW5pemVUaW1lTGltaXQpO1xuICAgIGNvbnN0IHRva2Vuc0xlbmd0aCA9IHJlc3VsdC50b2tlbnMubGVuZ3RoIC8gMjtcbiAgICBmb3IgKGxldCBqID0gMDsgaiA8IHRva2Vuc0xlbmd0aDsgaisrKSB7XG4gICAgICBjb25zdCBzdGFydEluZGV4ID0gcmVzdWx0LnRva2Vuc1syICogal07XG4gICAgICBjb25zdCBuZXh0U3RhcnRJbmRleCA9IGogKyAxIDwgdG9rZW5zTGVuZ3RoID8gcmVzdWx0LnRva2Vuc1syICogaiArIDJdIDogbGluZS5sZW5ndGg7XG4gICAgICBpZiAoc3RhcnRJbmRleCA9PT0gbmV4dFN0YXJ0SW5kZXgpXG4gICAgICAgIGNvbnRpbnVlO1xuICAgICAgY29uc3QgbWV0YWRhdGEgPSByZXN1bHQudG9rZW5zWzIgKiBqICsgMV07XG4gICAgICBjb25zdCBjb2xvciA9IGFwcGx5Q29sb3JSZXBsYWNlbWVudHMoXG4gICAgICAgIGNvbG9yTWFwW0VuY29kZWRUb2tlbk1ldGFkYXRhLmdldEZvcmVncm91bmQobWV0YWRhdGEpXSxcbiAgICAgICAgY29sb3JSZXBsYWNlbWVudHNcbiAgICAgICk7XG4gICAgICBjb25zdCBmb250U3R5bGUgPSBFbmNvZGVkVG9rZW5NZXRhZGF0YS5nZXRGb250U3R5bGUobWV0YWRhdGEpO1xuICAgICAgY29uc3QgdG9rZW4gPSB7XG4gICAgICAgIGNvbnRlbnQ6IGxpbmUuc3Vic3RyaW5nKHN0YXJ0SW5kZXgsIG5leHRTdGFydEluZGV4KSxcbiAgICAgICAgb2Zmc2V0OiBsaW5lT2Zmc2V0ICsgc3RhcnRJbmRleCxcbiAgICAgICAgY29sb3IsXG4gICAgICAgIGZvbnRTdHlsZVxuICAgICAgfTtcbiAgICAgIGlmIChvcHRpb25zLmluY2x1ZGVFeHBsYW5hdGlvbikge1xuICAgICAgICBjb25zdCB0aGVtZVNldHRpbmdzU2VsZWN0b3JzID0gW107XG4gICAgICAgIGlmIChvcHRpb25zLmluY2x1ZGVFeHBsYW5hdGlvbiAhPT0gXCJzY29wZU5hbWVcIikge1xuICAgICAgICAgIGZvciAoY29uc3Qgc2V0dGluZyBvZiB0aGVtZS5zZXR0aW5ncykge1xuICAgICAgICAgICAgbGV0IHNlbGVjdG9ycztcbiAgICAgICAgICAgIHN3aXRjaCAodHlwZW9mIHNldHRpbmcuc2NvcGUpIHtcbiAgICAgICAgICAgICAgY2FzZSBcInN0cmluZ1wiOlxuICAgICAgICAgICAgICAgIHNlbGVjdG9ycyA9IHNldHRpbmcuc2NvcGUuc3BsaXQoLywvKS5tYXAoKHNjb3BlKSA9PiBzY29wZS50cmltKCkpO1xuICAgICAgICAgICAgICAgIGJyZWFrO1xuICAgICAgICAgICAgICBjYXNlIFwib2JqZWN0XCI6XG4gICAgICAgICAgICAgICAgc2VsZWN0b3JzID0gc2V0dGluZy5zY29wZTtcbiAgICAgICAgICAgICAgICBicmVhaztcbiAgICAgICAgICAgICAgZGVmYXVsdDpcbiAgICAgICAgICAgICAgICBjb250aW51ZTtcbiAgICAgICAgICAgIH1cbiAgICAgICAgICAgIHRoZW1lU2V0dGluZ3NTZWxlY3RvcnMucHVzaCh7XG4gICAgICAgICAgICAgIHNldHRpbmdzOiBzZXR0aW5nLFxuICAgICAgICAgICAgICBzZWxlY3RvcnM6IHNlbGVjdG9ycy5tYXAoKHNlbGVjdG9yKSA9PiBzZWxlY3Rvci5zcGxpdCgvIC8pKVxuICAgICAgICAgICAgfSk7XG4gICAgICAgICAgfVxuICAgICAgICB9XG4gICAgICAgIHRva2VuLmV4cGxhbmF0aW9uID0gW107XG4gICAgICAgIGxldCBvZmZzZXQgPSAwO1xuICAgICAgICB3aGlsZSAoc3RhcnRJbmRleCArIG9mZnNldCA8IG5leHRTdGFydEluZGV4KSB7XG4gICAgICAgICAgY29uc3QgdG9rZW5XaXRoU2NvcGVzID0gdG9rZW5zV2l0aFNjb3Blc1t0b2tlbnNXaXRoU2NvcGVzSW5kZXhdO1xuICAgICAgICAgIGNvbnN0IHRva2VuV2l0aFNjb3Blc1RleHQgPSBsaW5lLnN1YnN0cmluZyhcbiAgICAgICAgICAgIHRva2VuV2l0aFNjb3Blcy5zdGFydEluZGV4LFxuICAgICAgICAgICAgdG9rZW5XaXRoU2NvcGVzLmVuZEluZGV4XG4gICAgICAgICAgKTtcbiAgICAgICAgICBvZmZzZXQgKz0gdG9rZW5XaXRoU2NvcGVzVGV4dC5sZW5ndGg7XG4gICAgICAgICAgdG9rZW4uZXhwbGFuYXRpb24ucHVzaCh7XG4gICAgICAgICAgICBjb250ZW50OiB0b2tlbldpdGhTY29wZXNUZXh0LFxuICAgICAgICAgICAgc2NvcGVzOiBvcHRpb25zLmluY2x1ZGVFeHBsYW5hdGlvbiA9PT0gXCJzY29wZU5hbWVcIiA/IGV4cGxhaW5UaGVtZVNjb3Blc05hbWVPbmx5KFxuICAgICAgICAgICAgICB0b2tlbldpdGhTY29wZXMuc2NvcGVzXG4gICAgICAgICAgICApIDogZXhwbGFpblRoZW1lU2NvcGVzRnVsbChcbiAgICAgICAgICAgICAgdGhlbWVTZXR0aW5nc1NlbGVjdG9ycyxcbiAgICAgICAgICAgICAgdG9rZW5XaXRoU2NvcGVzLnNjb3Blc1xuICAgICAgICAgICAgKVxuICAgICAgICAgIH0pO1xuICAgICAgICAgIHRva2Vuc1dpdGhTY29wZXNJbmRleCArPSAxO1xuICAgICAgICB9XG4gICAgICB9XG4gICAgICBhY3R1YWwucHVzaCh0b2tlbik7XG4gICAgfVxuICAgIGZpbmFsLnB1c2goYWN0dWFsKTtcbiAgICBhY3R1YWwgPSBbXTtcbiAgICBzdGF0ZVN0YWNrID0gcmVzdWx0LnJ1bGVTdGFjaztcbiAgfVxuICByZXR1cm4ge1xuICAgIHRva2VuczogZmluYWwsXG4gICAgc3RhdGVTdGFja1xuICB9O1xufVxuZnVuY3Rpb24gZXhwbGFpblRoZW1lU2NvcGVzTmFtZU9ubHkoc2NvcGVzKSB7XG4gIHJldHVybiBzY29wZXMubWFwKChzY29wZSkgPT4gKHsgc2NvcGVOYW1lOiBzY29wZSB9KSk7XG59XG5mdW5jdGlvbiBleHBsYWluVGhlbWVTY29wZXNGdWxsKHRoZW1lU2VsZWN0b3JzLCBzY29wZXMpIHtcbiAgY29uc3QgcmVzdWx0ID0gW107XG4gIGZvciAobGV0IGkgPSAwLCBsZW4gPSBzY29wZXMubGVuZ3RoOyBpIDwgbGVuOyBpKyspIHtcbiAgICBjb25zdCBzY29wZSA9IHNjb3Blc1tpXTtcbiAgICByZXN1bHRbaV0gPSB7XG4gICAgICBzY29wZU5hbWU6IHNjb3BlLFxuICAgICAgdGhlbWVNYXRjaGVzOiBleHBsYWluVGhlbWVTY29wZSh0aGVtZVNlbGVjdG9ycywgc2NvcGUsIHNjb3Blcy5zbGljZSgwLCBpKSlcbiAgICB9O1xuICB9XG4gIHJldHVybiByZXN1bHQ7XG59XG5mdW5jdGlvbiBtYXRjaGVzT25lKHNlbGVjdG9yLCBzY29wZSkge1xuICByZXR1cm4gc2VsZWN0b3IgPT09IHNjb3BlIHx8IHNjb3BlLnN1YnN0cmluZygwLCBzZWxlY3Rvci5sZW5ndGgpID09PSBzZWxlY3RvciAmJiBzY29wZVtzZWxlY3Rvci5sZW5ndGhdID09PSBcIi5cIjtcbn1cbmZ1bmN0aW9uIG1hdGNoZXMoc2VsZWN0b3JzLCBzY29wZSwgcGFyZW50U2NvcGVzKSB7XG4gIGlmICghbWF0Y2hlc09uZShzZWxlY3RvcnNbc2VsZWN0b3JzLmxlbmd0aCAtIDFdLCBzY29wZSkpXG4gICAgcmV0dXJuIGZhbHNlO1xuICBsZXQgc2VsZWN0b3JQYXJlbnRJbmRleCA9IHNlbGVjdG9ycy5sZW5ndGggLSAyO1xuICBsZXQgcGFyZW50SW5kZXggPSBwYXJlbnRTY29wZXMubGVuZ3RoIC0gMTtcbiAgd2hpbGUgKHNlbGVjdG9yUGFyZW50SW5kZXggPj0gMCAmJiBwYXJlbnRJbmRleCA+PSAwKSB7XG4gICAgaWYgKG1hdGNoZXNPbmUoc2VsZWN0b3JzW3NlbGVjdG9yUGFyZW50SW5kZXhdLCBwYXJlbnRTY29wZXNbcGFyZW50SW5kZXhdKSlcbiAgICAgIHNlbGVjdG9yUGFyZW50SW5kZXggLT0gMTtcbiAgICBwYXJlbnRJbmRleCAtPSAxO1xuICB9XG4gIGlmIChzZWxlY3RvclBhcmVudEluZGV4ID09PSAtMSlcbiAgICByZXR1cm4gdHJ1ZTtcbiAgcmV0dXJuIGZhbHNlO1xufVxuZnVuY3Rpb24gZXhwbGFpblRoZW1lU2NvcGUodGhlbWVTZXR0aW5nc1NlbGVjdG9ycywgc2NvcGUsIHBhcmVudFNjb3Blcykge1xuICBjb25zdCByZXN1bHQgPSBbXTtcbiAgZm9yIChjb25zdCB7IHNlbGVjdG9ycywgc2V0dGluZ3MgfSBvZiB0aGVtZVNldHRpbmdzU2VsZWN0b3JzKSB7XG4gICAgZm9yIChjb25zdCBzZWxlY3RvclBpZWNlcyBvZiBzZWxlY3RvcnMpIHtcbiAgICAgIGlmIChtYXRjaGVzKHNlbGVjdG9yUGllY2VzLCBzY29wZSwgcGFyZW50U2NvcGVzKSkge1xuICAgICAgICByZXN1bHQucHVzaChzZXR0aW5ncyk7XG4gICAgICAgIGJyZWFrO1xuICAgICAgfVxuICAgIH1cbiAgfVxuICByZXR1cm4gcmVzdWx0O1xufVxuXG5mdW5jdGlvbiBjb2RlVG9Ub2tlbnNXaXRoVGhlbWVzKGludGVybmFsLCBjb2RlLCBvcHRpb25zKSB7XG4gIGNvbnN0IHRoZW1lcyA9IE9iamVjdC5lbnRyaWVzKG9wdGlvbnMudGhlbWVzKS5maWx0ZXIoKGkpID0+IGlbMV0pLm1hcCgoaSkgPT4gKHsgY29sb3I6IGlbMF0sIHRoZW1lOiBpWzFdIH0pKTtcbiAgY29uc3QgdGhlbWVkVG9rZW5zID0gdGhlbWVzLm1hcCgodCkgPT4ge1xuICAgIGNvbnN0IHRva2VuczIgPSBjb2RlVG9Ub2tlbnNCYXNlKGludGVybmFsLCBjb2RlLCB7XG4gICAgICAuLi5vcHRpb25zLFxuICAgICAgdGhlbWU6IHQudGhlbWVcbiAgICB9KTtcbiAgICBjb25zdCBzdGF0ZSA9IGdldExhc3RHcmFtbWFyU3RhdGVGcm9tTWFwKHRva2VuczIpO1xuICAgIGNvbnN0IHRoZW1lID0gdHlwZW9mIHQudGhlbWUgPT09IFwic3RyaW5nXCIgPyB0LnRoZW1lIDogdC50aGVtZS5uYW1lO1xuICAgIHJldHVybiB7XG4gICAgICB0b2tlbnM6IHRva2VuczIsXG4gICAgICBzdGF0ZSxcbiAgICAgIHRoZW1lXG4gICAgfTtcbiAgfSk7XG4gIGNvbnN0IHRva2VucyA9IHN5bmNUaGVtZXNUb2tlbml6YXRpb24oXG4gICAgLi4udGhlbWVkVG9rZW5zLm1hcCgoaSkgPT4gaS50b2tlbnMpXG4gICk7XG4gIGNvbnN0IG1lcmdlZFRva2VucyA9IHRva2Vuc1swXS5tYXAoXG4gICAgKGxpbmUsIGxpbmVJZHgpID0+IGxpbmUubWFwKChfdG9rZW4sIHRva2VuSWR4KSA9PiB7XG4gICAgICBjb25zdCBtZXJnZWRUb2tlbiA9IHtcbiAgICAgICAgY29udGVudDogX3Rva2VuLmNvbnRlbnQsXG4gICAgICAgIHZhcmlhbnRzOiB7fSxcbiAgICAgICAgb2Zmc2V0OiBfdG9rZW4ub2Zmc2V0XG4gICAgICB9O1xuICAgICAgaWYgKFwiaW5jbHVkZUV4cGxhbmF0aW9uXCIgaW4gb3B0aW9ucyAmJiBvcHRpb25zLmluY2x1ZGVFeHBsYW5hdGlvbikge1xuICAgICAgICBtZXJnZWRUb2tlbi5leHBsYW5hdGlvbiA9IF90b2tlbi5leHBsYW5hdGlvbjtcbiAgICAgIH1cbiAgICAgIHRva2Vucy5mb3JFYWNoKCh0LCB0aGVtZUlkeCkgPT4ge1xuICAgICAgICBjb25zdCB7XG4gICAgICAgICAgY29udGVudDogXyxcbiAgICAgICAgICBleHBsYW5hdGlvbjogX18sXG4gICAgICAgICAgb2Zmc2V0OiBfX18sXG4gICAgICAgICAgLi4uc3R5bGVzXG4gICAgICAgIH0gPSB0W2xpbmVJZHhdW3Rva2VuSWR4XTtcbiAgICAgICAgbWVyZ2VkVG9rZW4udmFyaWFudHNbdGhlbWVzW3RoZW1lSWR4XS5jb2xvcl0gPSBzdHlsZXM7XG4gICAgICB9KTtcbiAgICAgIHJldHVybiBtZXJnZWRUb2tlbjtcbiAgICB9KVxuICApO1xuICBjb25zdCBtZXJnZWRHcmFtbWFyU3RhdGUgPSB0aGVtZWRUb2tlbnNbMF0uc3RhdGUgPyBuZXcgR3JhbW1hclN0YXRlKFxuICAgIE9iamVjdC5mcm9tRW50cmllcyh0aGVtZWRUb2tlbnMubWFwKChzKSA9PiBbcy50aGVtZSwgcy5zdGF0ZT8uZ2V0SW50ZXJuYWxTdGFjayhzLnRoZW1lKV0pKSxcbiAgICB0aGVtZWRUb2tlbnNbMF0uc3RhdGUubGFuZ1xuICApIDogdm9pZCAwO1xuICBpZiAobWVyZ2VkR3JhbW1hclN0YXRlKVxuICAgIHNldExhc3RHcmFtbWFyU3RhdGVUb01hcChtZXJnZWRUb2tlbnMsIG1lcmdlZEdyYW1tYXJTdGF0ZSk7XG4gIHJldHVybiBtZXJnZWRUb2tlbnM7XG59XG5mdW5jdGlvbiBzeW5jVGhlbWVzVG9rZW5pemF0aW9uKC4uLnRoZW1lcykge1xuICBjb25zdCBvdXRUaGVtZXMgPSB0aGVtZXMubWFwKCgpID0+IFtdKTtcbiAgY29uc3QgY291bnQgPSB0aGVtZXMubGVuZ3RoO1xuICBmb3IgKGxldCBpID0gMDsgaSA8IHRoZW1lc1swXS5sZW5ndGg7IGkrKykge1xuICAgIGNvbnN0IGxpbmVzID0gdGhlbWVzLm1hcCgodCkgPT4gdFtpXSk7XG4gICAgY29uc3Qgb3V0TGluZXMgPSBvdXRUaGVtZXMubWFwKCgpID0+IFtdKTtcbiAgICBvdXRUaGVtZXMuZm9yRWFjaCgodCwgaTIpID0+IHQucHVzaChvdXRMaW5lc1tpMl0pKTtcbiAgICBjb25zdCBpbmRleGVzID0gbGluZXMubWFwKCgpID0+IDApO1xuICAgIGNvbnN0IGN1cnJlbnQgPSBsaW5lcy5tYXAoKGwpID0+IGxbMF0pO1xuICAgIHdoaWxlIChjdXJyZW50LmV2ZXJ5KCh0KSA9PiB0KSkge1xuICAgICAgY29uc3QgbWluTGVuZ3RoID0gTWF0aC5taW4oLi4uY3VycmVudC5tYXAoKHQpID0+IHQuY29udGVudC5sZW5ndGgpKTtcbiAgICAgIGZvciAobGV0IG4gPSAwOyBuIDwgY291bnQ7IG4rKykge1xuICAgICAgICBjb25zdCB0b2tlbiA9IGN1cnJlbnRbbl07XG4gICAgICAgIGlmICh0b2tlbi5jb250ZW50Lmxlbmd0aCA9PT0gbWluTGVuZ3RoKSB7XG4gICAgICAgICAgb3V0TGluZXNbbl0ucHVzaCh0b2tlbik7XG4gICAgICAgICAgaW5kZXhlc1tuXSArPSAxO1xuICAgICAgICAgIGN1cnJlbnRbbl0gPSBsaW5lc1tuXVtpbmRleGVzW25dXTtcbiAgICAgICAgfSBlbHNlIHtcbiAgICAgICAgICBvdXRMaW5lc1tuXS5wdXNoKHtcbiAgICAgICAgICAgIC4uLnRva2VuLFxuICAgICAgICAgICAgY29udGVudDogdG9rZW4uY29udGVudC5zbGljZSgwLCBtaW5MZW5ndGgpXG4gICAgICAgICAgfSk7XG4gICAgICAgICAgY3VycmVudFtuXSA9IHtcbiAgICAgICAgICAgIC4uLnRva2VuLFxuICAgICAgICAgICAgY29udGVudDogdG9rZW4uY29udGVudC5zbGljZShtaW5MZW5ndGgpLFxuICAgICAgICAgICAgb2Zmc2V0OiB0b2tlbi5vZmZzZXQgKyBtaW5MZW5ndGhcbiAgICAgICAgICB9O1xuICAgICAgICB9XG4gICAgICB9XG4gICAgfVxuICB9XG4gIHJldHVybiBvdXRUaGVtZXM7XG59XG5cbmZ1bmN0aW9uIGNvZGVUb1Rva2VucyhpbnRlcm5hbCwgY29kZSwgb3B0aW9ucykge1xuICBsZXQgYmc7XG4gIGxldCBmZztcbiAgbGV0IHRva2VucztcbiAgbGV0IHRoZW1lTmFtZTtcbiAgbGV0IHJvb3RTdHlsZTtcbiAgbGV0IGdyYW1tYXJTdGF0ZTtcbiAgaWYgKFwidGhlbWVzXCIgaW4gb3B0aW9ucykge1xuICAgIGNvbnN0IHtcbiAgICAgIGRlZmF1bHRDb2xvciA9IFwibGlnaHRcIixcbiAgICAgIGNzc1ZhcmlhYmxlUHJlZml4ID0gXCItLXNoaWtpLVwiXG4gICAgfSA9IG9wdGlvbnM7XG4gICAgY29uc3QgdGhlbWVzID0gT2JqZWN0LmVudHJpZXMob3B0aW9ucy50aGVtZXMpLmZpbHRlcigoaSkgPT4gaVsxXSkubWFwKChpKSA9PiAoeyBjb2xvcjogaVswXSwgdGhlbWU6IGlbMV0gfSkpLnNvcnQoKGEsIGIpID0+IGEuY29sb3IgPT09IGRlZmF1bHRDb2xvciA/IC0xIDogYi5jb2xvciA9PT0gZGVmYXVsdENvbG9yID8gMSA6IDApO1xuICAgIGlmICh0aGVtZXMubGVuZ3RoID09PSAwKVxuICAgICAgdGhyb3cgbmV3IFNoaWtpRXJyb3IkMShcImB0aGVtZXNgIG9wdGlvbiBtdXN0IG5vdCBiZSBlbXB0eVwiKTtcbiAgICBjb25zdCB0aGVtZVRva2VucyA9IGNvZGVUb1Rva2Vuc1dpdGhUaGVtZXMoXG4gICAgICBpbnRlcm5hbCxcbiAgICAgIGNvZGUsXG4gICAgICBvcHRpb25zXG4gICAgKTtcbiAgICBncmFtbWFyU3RhdGUgPSBnZXRMYXN0R3JhbW1hclN0YXRlRnJvbU1hcCh0aGVtZVRva2Vucyk7XG4gICAgaWYgKGRlZmF1bHRDb2xvciAmJiAhdGhlbWVzLmZpbmQoKHQpID0+IHQuY29sb3IgPT09IGRlZmF1bHRDb2xvcikpXG4gICAgICB0aHJvdyBuZXcgU2hpa2lFcnJvciQxKGBcXGB0aGVtZXNcXGAgb3B0aW9uIG11c3QgY29udGFpbiB0aGUgZGVmYXVsdENvbG9yIGtleSBcXGAke2RlZmF1bHRDb2xvcn1cXGBgKTtcbiAgICBjb25zdCB0aGVtZVJlZ3MgPSB0aGVtZXMubWFwKCh0KSA9PiBpbnRlcm5hbC5nZXRUaGVtZSh0LnRoZW1lKSk7XG4gICAgY29uc3QgdGhlbWVzT3JkZXIgPSB0aGVtZXMubWFwKCh0KSA9PiB0LmNvbG9yKTtcbiAgICB0b2tlbnMgPSB0aGVtZVRva2Vucy5tYXAoKGxpbmUpID0+IGxpbmUubWFwKCh0b2tlbikgPT4gbWVyZ2VUb2tlbih0b2tlbiwgdGhlbWVzT3JkZXIsIGNzc1ZhcmlhYmxlUHJlZml4LCBkZWZhdWx0Q29sb3IpKSk7XG4gICAgaWYgKGdyYW1tYXJTdGF0ZSlcbiAgICAgIHNldExhc3RHcmFtbWFyU3RhdGVUb01hcCh0b2tlbnMsIGdyYW1tYXJTdGF0ZSk7XG4gICAgY29uc3QgdGhlbWVDb2xvclJlcGxhY2VtZW50cyA9IHRoZW1lcy5tYXAoKHQpID0+IHJlc29sdmVDb2xvclJlcGxhY2VtZW50cyh0LnRoZW1lLCBvcHRpb25zKSk7XG4gICAgZmcgPSB0aGVtZXMubWFwKCh0LCBpZHgpID0+IChpZHggPT09IDAgJiYgZGVmYXVsdENvbG9yID8gXCJcIiA6IGAke2Nzc1ZhcmlhYmxlUHJlZml4ICsgdC5jb2xvcn06YCkgKyAoYXBwbHlDb2xvclJlcGxhY2VtZW50cyh0aGVtZVJlZ3NbaWR4XS5mZywgdGhlbWVDb2xvclJlcGxhY2VtZW50c1tpZHhdKSB8fCBcImluaGVyaXRcIikpLmpvaW4oXCI7XCIpO1xuICAgIGJnID0gdGhlbWVzLm1hcCgodCwgaWR4KSA9PiAoaWR4ID09PSAwICYmIGRlZmF1bHRDb2xvciA/IFwiXCIgOiBgJHtjc3NWYXJpYWJsZVByZWZpeCArIHQuY29sb3J9LWJnOmApICsgKGFwcGx5Q29sb3JSZXBsYWNlbWVudHModGhlbWVSZWdzW2lkeF0uYmcsIHRoZW1lQ29sb3JSZXBsYWNlbWVudHNbaWR4XSkgfHwgXCJpbmhlcml0XCIpKS5qb2luKFwiO1wiKTtcbiAgICB0aGVtZU5hbWUgPSBgc2hpa2ktdGhlbWVzICR7dGhlbWVSZWdzLm1hcCgodCkgPT4gdC5uYW1lKS5qb2luKFwiIFwiKX1gO1xuICAgIHJvb3RTdHlsZSA9IGRlZmF1bHRDb2xvciA/IHZvaWQgMCA6IFtmZywgYmddLmpvaW4oXCI7XCIpO1xuICB9IGVsc2UgaWYgKFwidGhlbWVcIiBpbiBvcHRpb25zKSB7XG4gICAgY29uc3QgY29sb3JSZXBsYWNlbWVudHMgPSByZXNvbHZlQ29sb3JSZXBsYWNlbWVudHMob3B0aW9ucy50aGVtZSwgb3B0aW9ucyk7XG4gICAgdG9rZW5zID0gY29kZVRvVG9rZW5zQmFzZShcbiAgICAgIGludGVybmFsLFxuICAgICAgY29kZSxcbiAgICAgIG9wdGlvbnNcbiAgICApO1xuICAgIGNvbnN0IF90aGVtZSA9IGludGVybmFsLmdldFRoZW1lKG9wdGlvbnMudGhlbWUpO1xuICAgIGJnID0gYXBwbHlDb2xvclJlcGxhY2VtZW50cyhfdGhlbWUuYmcsIGNvbG9yUmVwbGFjZW1lbnRzKTtcbiAgICBmZyA9IGFwcGx5Q29sb3JSZXBsYWNlbWVudHMoX3RoZW1lLmZnLCBjb2xvclJlcGxhY2VtZW50cyk7XG4gICAgdGhlbWVOYW1lID0gX3RoZW1lLm5hbWU7XG4gICAgZ3JhbW1hclN0YXRlID0gZ2V0TGFzdEdyYW1tYXJTdGF0ZUZyb21NYXAodG9rZW5zKTtcbiAgfSBlbHNlIHtcbiAgICB0aHJvdyBuZXcgU2hpa2lFcnJvciQxKFwiSW52YWxpZCBvcHRpb25zLCBlaXRoZXIgYHRoZW1lYCBvciBgdGhlbWVzYCBtdXN0IGJlIHByb3ZpZGVkXCIpO1xuICB9XG4gIHJldHVybiB7XG4gICAgdG9rZW5zLFxuICAgIGZnLFxuICAgIGJnLFxuICAgIHRoZW1lTmFtZSxcbiAgICByb290U3R5bGUsXG4gICAgZ3JhbW1hclN0YXRlXG4gIH07XG59XG5mdW5jdGlvbiBtZXJnZVRva2VuKG1lcmdlZCwgdmFyaWFudHNPcmRlciwgY3NzVmFyaWFibGVQcmVmaXgsIGRlZmF1bHRDb2xvcikge1xuICBjb25zdCB0b2tlbiA9IHtcbiAgICBjb250ZW50OiBtZXJnZWQuY29udGVudCxcbiAgICBleHBsYW5hdGlvbjogbWVyZ2VkLmV4cGxhbmF0aW9uLFxuICAgIG9mZnNldDogbWVyZ2VkLm9mZnNldFxuICB9O1xuICBjb25zdCBzdHlsZXMgPSB2YXJpYW50c09yZGVyLm1hcCgodCkgPT4gZ2V0VG9rZW5TdHlsZU9iamVjdChtZXJnZWQudmFyaWFudHNbdF0pKTtcbiAgY29uc3Qgc3R5bGVLZXlzID0gbmV3IFNldChzdHlsZXMuZmxhdE1hcCgodCkgPT4gT2JqZWN0LmtleXModCkpKTtcbiAgY29uc3QgbWVyZ2VkU3R5bGVzID0ge307XG4gIHN0eWxlcy5mb3JFYWNoKChjdXIsIGlkeCkgPT4ge1xuICAgIGZvciAoY29uc3Qga2V5IG9mIHN0eWxlS2V5cykge1xuICAgICAgY29uc3QgdmFsdWUgPSBjdXJba2V5XSB8fCBcImluaGVyaXRcIjtcbiAgICAgIGlmIChpZHggPT09IDAgJiYgZGVmYXVsdENvbG9yKSB7XG4gICAgICAgIG1lcmdlZFN0eWxlc1trZXldID0gdmFsdWU7XG4gICAgICB9IGVsc2Uge1xuICAgICAgICBjb25zdCBrZXlOYW1lID0ga2V5ID09PSBcImNvbG9yXCIgPyBcIlwiIDoga2V5ID09PSBcImJhY2tncm91bmQtY29sb3JcIiA/IFwiLWJnXCIgOiBgLSR7a2V5fWA7XG4gICAgICAgIGNvbnN0IHZhcktleSA9IGNzc1ZhcmlhYmxlUHJlZml4ICsgdmFyaWFudHNPcmRlcltpZHhdICsgKGtleSA9PT0gXCJjb2xvclwiID8gXCJcIiA6IGtleU5hbWUpO1xuICAgICAgICBtZXJnZWRTdHlsZXNbdmFyS2V5XSA9IHZhbHVlO1xuICAgICAgfVxuICAgIH1cbiAgfSk7XG4gIHRva2VuLmh0bWxTdHlsZSA9IG1lcmdlZFN0eWxlcztcbiAgcmV0dXJuIHRva2VuO1xufVxuXG5mdW5jdGlvbiBjb2RlVG9IYXN0KGludGVybmFsLCBjb2RlLCBvcHRpb25zLCB0cmFuc2Zvcm1lckNvbnRleHQgPSB7XG4gIG1ldGE6IHt9LFxuICBvcHRpb25zLFxuICBjb2RlVG9IYXN0OiAoX2NvZGUsIF9vcHRpb25zKSA9PiBjb2RlVG9IYXN0KGludGVybmFsLCBfY29kZSwgX29wdGlvbnMpLFxuICBjb2RlVG9Ub2tlbnM6IChfY29kZSwgX29wdGlvbnMpID0+IGNvZGVUb1Rva2VucyhpbnRlcm5hbCwgX2NvZGUsIF9vcHRpb25zKVxufSkge1xuICBsZXQgaW5wdXQgPSBjb2RlO1xuICBmb3IgKGNvbnN0IHRyYW5zZm9ybWVyIG9mIGdldFRyYW5zZm9ybWVycyhvcHRpb25zKSlcbiAgICBpbnB1dCA9IHRyYW5zZm9ybWVyLnByZXByb2Nlc3M/LmNhbGwodHJhbnNmb3JtZXJDb250ZXh0LCBpbnB1dCwgb3B0aW9ucykgfHwgaW5wdXQ7XG4gIGxldCB7XG4gICAgdG9rZW5zLFxuICAgIGZnLFxuICAgIGJnLFxuICAgIHRoZW1lTmFtZSxcbiAgICByb290U3R5bGUsXG4gICAgZ3JhbW1hclN0YXRlXG4gIH0gPSBjb2RlVG9Ub2tlbnMoaW50ZXJuYWwsIGlucHV0LCBvcHRpb25zKTtcbiAgY29uc3Qge1xuICAgIG1lcmdlV2hpdGVzcGFjZXMgPSB0cnVlXG4gIH0gPSBvcHRpb25zO1xuICBpZiAobWVyZ2VXaGl0ZXNwYWNlcyA9PT0gdHJ1ZSlcbiAgICB0b2tlbnMgPSBtZXJnZVdoaXRlc3BhY2VUb2tlbnModG9rZW5zKTtcbiAgZWxzZSBpZiAobWVyZ2VXaGl0ZXNwYWNlcyA9PT0gXCJuZXZlclwiKVxuICAgIHRva2VucyA9IHNwbGl0V2hpdGVzcGFjZVRva2Vucyh0b2tlbnMpO1xuICBjb25zdCBjb250ZXh0U291cmNlID0ge1xuICAgIC4uLnRyYW5zZm9ybWVyQ29udGV4dCxcbiAgICBnZXQgc291cmNlKCkge1xuICAgICAgcmV0dXJuIGlucHV0O1xuICAgIH1cbiAgfTtcbiAgZm9yIChjb25zdCB0cmFuc2Zvcm1lciBvZiBnZXRUcmFuc2Zvcm1lcnMob3B0aW9ucykpXG4gICAgdG9rZW5zID0gdHJhbnNmb3JtZXIudG9rZW5zPy5jYWxsKGNvbnRleHRTb3VyY2UsIHRva2VucykgfHwgdG9rZW5zO1xuICByZXR1cm4gdG9rZW5zVG9IYXN0KFxuICAgIHRva2VucyxcbiAgICB7XG4gICAgICAuLi5vcHRpb25zLFxuICAgICAgZmcsXG4gICAgICBiZyxcbiAgICAgIHRoZW1lTmFtZSxcbiAgICAgIHJvb3RTdHlsZVxuICAgIH0sXG4gICAgY29udGV4dFNvdXJjZSxcbiAgICBncmFtbWFyU3RhdGVcbiAgKTtcbn1cbmZ1bmN0aW9uIHRva2Vuc1RvSGFzdCh0b2tlbnMsIG9wdGlvbnMsIHRyYW5zZm9ybWVyQ29udGV4dCwgZ3JhbW1hclN0YXRlID0gZ2V0TGFzdEdyYW1tYXJTdGF0ZUZyb21NYXAodG9rZW5zKSkge1xuICBjb25zdCB0cmFuc2Zvcm1lcnMgPSBnZXRUcmFuc2Zvcm1lcnMob3B0aW9ucyk7XG4gIGNvbnN0IGxpbmVzID0gW107XG4gIGNvbnN0IHJvb3QgPSB7XG4gICAgdHlwZTogXCJyb290XCIsXG4gICAgY2hpbGRyZW46IFtdXG4gIH07XG4gIGNvbnN0IHtcbiAgICBzdHJ1Y3R1cmUgPSBcImNsYXNzaWNcIixcbiAgICB0YWJpbmRleCA9IFwiMFwiXG4gIH0gPSBvcHRpb25zO1xuICBsZXQgcHJlTm9kZSA9IHtcbiAgICB0eXBlOiBcImVsZW1lbnRcIixcbiAgICB0YWdOYW1lOiBcInByZVwiLFxuICAgIHByb3BlcnRpZXM6IHtcbiAgICAgIGNsYXNzOiBgc2hpa2kgJHtvcHRpb25zLnRoZW1lTmFtZSB8fCBcIlwifWAsXG4gICAgICBzdHlsZTogb3B0aW9ucy5yb290U3R5bGUgfHwgYGJhY2tncm91bmQtY29sb3I6JHtvcHRpb25zLmJnfTtjb2xvcjoke29wdGlvbnMuZmd9YCxcbiAgICAgIC4uLnRhYmluZGV4ICE9PSBmYWxzZSAmJiB0YWJpbmRleCAhPSBudWxsID8ge1xuICAgICAgICB0YWJpbmRleDogdGFiaW5kZXgudG9TdHJpbmcoKVxuICAgICAgfSA6IHt9LFxuICAgICAgLi4uT2JqZWN0LmZyb21FbnRyaWVzKFxuICAgICAgICBBcnJheS5mcm9tKFxuICAgICAgICAgIE9iamVjdC5lbnRyaWVzKG9wdGlvbnMubWV0YSB8fCB7fSlcbiAgICAgICAgKS5maWx0ZXIoKFtrZXldKSA9PiAha2V5LnN0YXJ0c1dpdGgoXCJfXCIpKVxuICAgICAgKVxuICAgIH0sXG4gICAgY2hpbGRyZW46IFtdXG4gIH07XG4gIGxldCBjb2RlTm9kZSA9IHtcbiAgICB0eXBlOiBcImVsZW1lbnRcIixcbiAgICB0YWdOYW1lOiBcImNvZGVcIixcbiAgICBwcm9wZXJ0aWVzOiB7fSxcbiAgICBjaGlsZHJlbjogbGluZXNcbiAgfTtcbiAgY29uc3QgbGluZU5vZGVzID0gW107XG4gIGNvbnN0IGNvbnRleHQgPSB7XG4gICAgLi4udHJhbnNmb3JtZXJDb250ZXh0LFxuICAgIHN0cnVjdHVyZSxcbiAgICBhZGRDbGFzc1RvSGFzdCxcbiAgICBnZXQgc291cmNlKCkge1xuICAgICAgcmV0dXJuIHRyYW5zZm9ybWVyQ29udGV4dC5zb3VyY2U7XG4gICAgfSxcbiAgICBnZXQgdG9rZW5zKCkge1xuICAgICAgcmV0dXJuIHRva2VucztcbiAgICB9LFxuICAgIGdldCBvcHRpb25zKCkge1xuICAgICAgcmV0dXJuIG9wdGlvbnM7XG4gICAgfSxcbiAgICBnZXQgcm9vdCgpIHtcbiAgICAgIHJldHVybiByb290O1xuICAgIH0sXG4gICAgZ2V0IHByZSgpIHtcbiAgICAgIHJldHVybiBwcmVOb2RlO1xuICAgIH0sXG4gICAgZ2V0IGNvZGUoKSB7XG4gICAgICByZXR1cm4gY29kZU5vZGU7XG4gICAgfSxcbiAgICBnZXQgbGluZXMoKSB7XG4gICAgICByZXR1cm4gbGluZU5vZGVzO1xuICAgIH1cbiAgfTtcbiAgdG9rZW5zLmZvckVhY2goKGxpbmUsIGlkeCkgPT4ge1xuICAgIGlmIChpZHgpIHtcbiAgICAgIGlmIChzdHJ1Y3R1cmUgPT09IFwiaW5saW5lXCIpXG4gICAgICAgIHJvb3QuY2hpbGRyZW4ucHVzaCh7IHR5cGU6IFwiZWxlbWVudFwiLCB0YWdOYW1lOiBcImJyXCIsIHByb3BlcnRpZXM6IHt9LCBjaGlsZHJlbjogW10gfSk7XG4gICAgICBlbHNlIGlmIChzdHJ1Y3R1cmUgPT09IFwiY2xhc3NpY1wiKVxuICAgICAgICBsaW5lcy5wdXNoKHsgdHlwZTogXCJ0ZXh0XCIsIHZhbHVlOiBcIlxcblwiIH0pO1xuICAgIH1cbiAgICBsZXQgbGluZU5vZGUgPSB7XG4gICAgICB0eXBlOiBcImVsZW1lbnRcIixcbiAgICAgIHRhZ05hbWU6IFwic3BhblwiLFxuICAgICAgcHJvcGVydGllczogeyBjbGFzczogXCJsaW5lXCIgfSxcbiAgICAgIGNoaWxkcmVuOiBbXVxuICAgIH07XG4gICAgbGV0IGNvbCA9IDA7XG4gICAgZm9yIChjb25zdCB0b2tlbiBvZiBsaW5lKSB7XG4gICAgICBsZXQgdG9rZW5Ob2RlID0ge1xuICAgICAgICB0eXBlOiBcImVsZW1lbnRcIixcbiAgICAgICAgdGFnTmFtZTogXCJzcGFuXCIsXG4gICAgICAgIHByb3BlcnRpZXM6IHtcbiAgICAgICAgICAuLi50b2tlbi5odG1sQXR0cnNcbiAgICAgICAgfSxcbiAgICAgICAgY2hpbGRyZW46IFt7IHR5cGU6IFwidGV4dFwiLCB2YWx1ZTogdG9rZW4uY29udGVudCB9XVxuICAgICAgfTtcbiAgICAgIGlmICh0eXBlb2YgdG9rZW4uaHRtbFN0eWxlID09PSBcInN0cmluZ1wiKVxuICAgICAgICB3YXJuRGVwcmVjYXRlZChcImBodG1sU3R5bGVgIGFzIGEgc3RyaW5nIGlzIGRlcHJlY2F0ZWQuIFVzZSBhbiBvYmplY3QgaW5zdGVhZC5cIik7XG4gICAgICBjb25zdCBzdHlsZSA9IHN0cmluZ2lmeVRva2VuU3R5bGUodG9rZW4uaHRtbFN0eWxlIHx8IGdldFRva2VuU3R5bGVPYmplY3QodG9rZW4pKTtcbiAgICAgIGlmIChzdHlsZSlcbiAgICAgICAgdG9rZW5Ob2RlLnByb3BlcnRpZXMuc3R5bGUgPSBzdHlsZTtcbiAgICAgIGZvciAoY29uc3QgdHJhbnNmb3JtZXIgb2YgdHJhbnNmb3JtZXJzKVxuICAgICAgICB0b2tlbk5vZGUgPSB0cmFuc2Zvcm1lcj8uc3Bhbj8uY2FsbChjb250ZXh0LCB0b2tlbk5vZGUsIGlkeCArIDEsIGNvbCwgbGluZU5vZGUsIHRva2VuKSB8fCB0b2tlbk5vZGU7XG4gICAgICBpZiAoc3RydWN0dXJlID09PSBcImlubGluZVwiKVxuICAgICAgICByb290LmNoaWxkcmVuLnB1c2godG9rZW5Ob2RlKTtcbiAgICAgIGVsc2UgaWYgKHN0cnVjdHVyZSA9PT0gXCJjbGFzc2ljXCIpXG4gICAgICAgIGxpbmVOb2RlLmNoaWxkcmVuLnB1c2godG9rZW5Ob2RlKTtcbiAgICAgIGNvbCArPSB0b2tlbi5jb250ZW50Lmxlbmd0aDtcbiAgICB9XG4gICAgaWYgKHN0cnVjdHVyZSA9PT0gXCJjbGFzc2ljXCIpIHtcbiAgICAgIGZvciAoY29uc3QgdHJhbnNmb3JtZXIgb2YgdHJhbnNmb3JtZXJzKVxuICAgICAgICBsaW5lTm9kZSA9IHRyYW5zZm9ybWVyPy5saW5lPy5jYWxsKGNvbnRleHQsIGxpbmVOb2RlLCBpZHggKyAxKSB8fCBsaW5lTm9kZTtcbiAgICAgIGxpbmVOb2Rlcy5wdXNoKGxpbmVOb2RlKTtcbiAgICAgIGxpbmVzLnB1c2gobGluZU5vZGUpO1xuICAgIH1cbiAgfSk7XG4gIGlmIChzdHJ1Y3R1cmUgPT09IFwiY2xhc3NpY1wiKSB7XG4gICAgZm9yIChjb25zdCB0cmFuc2Zvcm1lciBvZiB0cmFuc2Zvcm1lcnMpXG4gICAgICBjb2RlTm9kZSA9IHRyYW5zZm9ybWVyPy5jb2RlPy5jYWxsKGNvbnRleHQsIGNvZGVOb2RlKSB8fCBjb2RlTm9kZTtcbiAgICBwcmVOb2RlLmNoaWxkcmVuLnB1c2goY29kZU5vZGUpO1xuICAgIGZvciAoY29uc3QgdHJhbnNmb3JtZXIgb2YgdHJhbnNmb3JtZXJzKVxuICAgICAgcHJlTm9kZSA9IHRyYW5zZm9ybWVyPy5wcmU/LmNhbGwoY29udGV4dCwgcHJlTm9kZSkgfHwgcHJlTm9kZTtcbiAgICByb290LmNoaWxkcmVuLnB1c2gocHJlTm9kZSk7XG4gIH1cbiAgbGV0IHJlc3VsdCA9IHJvb3Q7XG4gIGZvciAoY29uc3QgdHJhbnNmb3JtZXIgb2YgdHJhbnNmb3JtZXJzKVxuICAgIHJlc3VsdCA9IHRyYW5zZm9ybWVyPy5yb290Py5jYWxsKGNvbnRleHQsIHJlc3VsdCkgfHwgcmVzdWx0O1xuICBpZiAoZ3JhbW1hclN0YXRlKVxuICAgIHNldExhc3RHcmFtbWFyU3RhdGVUb01hcChyZXN1bHQsIGdyYW1tYXJTdGF0ZSk7XG4gIHJldHVybiByZXN1bHQ7XG59XG5mdW5jdGlvbiBtZXJnZVdoaXRlc3BhY2VUb2tlbnModG9rZW5zKSB7XG4gIHJldHVybiB0b2tlbnMubWFwKChsaW5lKSA9PiB7XG4gICAgY29uc3QgbmV3TGluZSA9IFtdO1xuICAgIGxldCBjYXJyeU9uQ29udGVudCA9IFwiXCI7XG4gICAgbGV0IGZpcnN0T2Zmc2V0ID0gMDtcbiAgICBsaW5lLmZvckVhY2goKHRva2VuLCBpZHgpID0+IHtcbiAgICAgIGNvbnN0IGlzVW5kZXJsaW5lID0gdG9rZW4uZm9udFN0eWxlICYmIHRva2VuLmZvbnRTdHlsZSAmIEZvbnRTdHlsZS5VbmRlcmxpbmU7XG4gICAgICBjb25zdCBjb3VsZE1lcmdlID0gIWlzVW5kZXJsaW5lO1xuICAgICAgaWYgKGNvdWxkTWVyZ2UgJiYgdG9rZW4uY29udGVudC5tYXRjaCgvXlxccyskLykgJiYgbGluZVtpZHggKyAxXSkge1xuICAgICAgICBpZiAoIWZpcnN0T2Zmc2V0KVxuICAgICAgICAgIGZpcnN0T2Zmc2V0ID0gdG9rZW4ub2Zmc2V0O1xuICAgICAgICBjYXJyeU9uQ29udGVudCArPSB0b2tlbi5jb250ZW50O1xuICAgICAgfSBlbHNlIHtcbiAgICAgICAgaWYgKGNhcnJ5T25Db250ZW50KSB7XG4gICAgICAgICAgaWYgKGNvdWxkTWVyZ2UpIHtcbiAgICAgICAgICAgIG5ld0xpbmUucHVzaCh7XG4gICAgICAgICAgICAgIC4uLnRva2VuLFxuICAgICAgICAgICAgICBvZmZzZXQ6IGZpcnN0T2Zmc2V0LFxuICAgICAgICAgICAgICBjb250ZW50OiBjYXJyeU9uQ29udGVudCArIHRva2VuLmNvbnRlbnRcbiAgICAgICAgICAgIH0pO1xuICAgICAgICAgIH0gZWxzZSB7XG4gICAgICAgICAgICBuZXdMaW5lLnB1c2goXG4gICAgICAgICAgICAgIHtcbiAgICAgICAgICAgICAgICBjb250ZW50OiBjYXJyeU9uQ29udGVudCxcbiAgICAgICAgICAgICAgICBvZmZzZXQ6IGZpcnN0T2Zmc2V0XG4gICAgICAgICAgICAgIH0sXG4gICAgICAgICAgICAgIHRva2VuXG4gICAgICAgICAgICApO1xuICAgICAgICAgIH1cbiAgICAgICAgICBmaXJzdE9mZnNldCA9IDA7XG4gICAgICAgICAgY2FycnlPbkNvbnRlbnQgPSBcIlwiO1xuICAgICAgICB9IGVsc2Uge1xuICAgICAgICAgIG5ld0xpbmUucHVzaCh0b2tlbik7XG4gICAgICAgIH1cbiAgICAgIH1cbiAgICB9KTtcbiAgICByZXR1cm4gbmV3TGluZTtcbiAgfSk7XG59XG5mdW5jdGlvbiBzcGxpdFdoaXRlc3BhY2VUb2tlbnModG9rZW5zKSB7XG4gIHJldHVybiB0b2tlbnMubWFwKChsaW5lKSA9PiB7XG4gICAgcmV0dXJuIGxpbmUuZmxhdE1hcCgodG9rZW4pID0+IHtcbiAgICAgIGlmICh0b2tlbi5jb250ZW50Lm1hdGNoKC9eXFxzKyQvKSlcbiAgICAgICAgcmV0dXJuIHRva2VuO1xuICAgICAgY29uc3QgbWF0Y2ggPSB0b2tlbi5jb250ZW50Lm1hdGNoKC9eKFxccyopKC4qPykoXFxzKikkLyk7XG4gICAgICBpZiAoIW1hdGNoKVxuICAgICAgICByZXR1cm4gdG9rZW47XG4gICAgICBjb25zdCBbLCBsZWFkaW5nLCBjb250ZW50LCB0cmFpbGluZ10gPSBtYXRjaDtcbiAgICAgIGlmICghbGVhZGluZyAmJiAhdHJhaWxpbmcpXG4gICAgICAgIHJldHVybiB0b2tlbjtcbiAgICAgIGNvbnN0IGV4cGFuZGVkID0gW3tcbiAgICAgICAgLi4udG9rZW4sXG4gICAgICAgIG9mZnNldDogdG9rZW4ub2Zmc2V0ICsgbGVhZGluZy5sZW5ndGgsXG4gICAgICAgIGNvbnRlbnRcbiAgICAgIH1dO1xuICAgICAgaWYgKGxlYWRpbmcpIHtcbiAgICAgICAgZXhwYW5kZWQudW5zaGlmdCh7XG4gICAgICAgICAgY29udGVudDogbGVhZGluZyxcbiAgICAgICAgICBvZmZzZXQ6IHRva2VuLm9mZnNldFxuICAgICAgICB9KTtcbiAgICAgIH1cbiAgICAgIGlmICh0cmFpbGluZykge1xuICAgICAgICBleHBhbmRlZC5wdXNoKHtcbiAgICAgICAgICBjb250ZW50OiB0cmFpbGluZyxcbiAgICAgICAgICBvZmZzZXQ6IHRva2VuLm9mZnNldCArIGxlYWRpbmcubGVuZ3RoICsgY29udGVudC5sZW5ndGhcbiAgICAgICAgfSk7XG4gICAgICB9XG4gICAgICByZXR1cm4gZXhwYW5kZWQ7XG4gICAgfSk7XG4gIH0pO1xufVxuXG5mdW5jdGlvbiBjb2RlVG9IdG1sKGludGVybmFsLCBjb2RlLCBvcHRpb25zKSB7XG4gIGNvbnN0IGNvbnRleHQgPSB7XG4gICAgbWV0YToge30sXG4gICAgb3B0aW9ucyxcbiAgICBjb2RlVG9IYXN0OiAoX2NvZGUsIF9vcHRpb25zKSA9PiBjb2RlVG9IYXN0KGludGVybmFsLCBfY29kZSwgX29wdGlvbnMpLFxuICAgIGNvZGVUb1Rva2VuczogKF9jb2RlLCBfb3B0aW9ucykgPT4gY29kZVRvVG9rZW5zKGludGVybmFsLCBfY29kZSwgX29wdGlvbnMpXG4gIH07XG4gIGxldCByZXN1bHQgPSB0b0h0bWwoY29kZVRvSGFzdChpbnRlcm5hbCwgY29kZSwgb3B0aW9ucywgY29udGV4dCkpO1xuICBmb3IgKGNvbnN0IHRyYW5zZm9ybWVyIG9mIGdldFRyYW5zZm9ybWVycyhvcHRpb25zKSlcbiAgICByZXN1bHQgPSB0cmFuc2Zvcm1lci5wb3N0cHJvY2Vzcz8uY2FsbChjb250ZXh0LCByZXN1bHQsIG9wdGlvbnMpIHx8IHJlc3VsdDtcbiAgcmV0dXJuIHJlc3VsdDtcbn1cblxuY29uc3QgVlNDT0RFX0ZBTExCQUNLX0VESVRPUl9GRyA9IHsgbGlnaHQ6IFwiIzMzMzMzM1wiLCBkYXJrOiBcIiNiYmJiYmJcIiB9O1xuY29uc3QgVlNDT0RFX0ZBTExCQUNLX0VESVRPUl9CRyA9IHsgbGlnaHQ6IFwiI2ZmZmZmZVwiLCBkYXJrOiBcIiMxZTFlMWVcIiB9O1xuY29uc3QgUkVTT0xWRURfS0VZID0gXCJfX3NoaWtpX3Jlc29sdmVkXCI7XG5mdW5jdGlvbiBub3JtYWxpemVUaGVtZShyYXdUaGVtZSkge1xuICBpZiAocmF3VGhlbWU/LltSRVNPTFZFRF9LRVldKVxuICAgIHJldHVybiByYXdUaGVtZTtcbiAgY29uc3QgdGhlbWUgPSB7XG4gICAgLi4ucmF3VGhlbWVcbiAgfTtcbiAgaWYgKHRoZW1lLnRva2VuQ29sb3JzICYmICF0aGVtZS5zZXR0aW5ncykge1xuICAgIHRoZW1lLnNldHRpbmdzID0gdGhlbWUudG9rZW5Db2xvcnM7XG4gICAgZGVsZXRlIHRoZW1lLnRva2VuQ29sb3JzO1xuICB9XG4gIHRoZW1lLnR5cGUgfHw9IFwiZGFya1wiO1xuICB0aGVtZS5jb2xvclJlcGxhY2VtZW50cyA9IHsgLi4udGhlbWUuY29sb3JSZXBsYWNlbWVudHMgfTtcbiAgdGhlbWUuc2V0dGluZ3MgfHw9IFtdO1xuICBsZXQgeyBiZywgZmcgfSA9IHRoZW1lO1xuICBpZiAoIWJnIHx8ICFmZykge1xuICAgIGNvbnN0IGdsb2JhbFNldHRpbmcgPSB0aGVtZS5zZXR0aW5ncyA/IHRoZW1lLnNldHRpbmdzLmZpbmQoKHMpID0+ICFzLm5hbWUgJiYgIXMuc2NvcGUpIDogdm9pZCAwO1xuICAgIGlmIChnbG9iYWxTZXR0aW5nPy5zZXR0aW5ncz8uZm9yZWdyb3VuZClcbiAgICAgIGZnID0gZ2xvYmFsU2V0dGluZy5zZXR0aW5ncy5mb3JlZ3JvdW5kO1xuICAgIGlmIChnbG9iYWxTZXR0aW5nPy5zZXR0aW5ncz8uYmFja2dyb3VuZClcbiAgICAgIGJnID0gZ2xvYmFsU2V0dGluZy5zZXR0aW5ncy5iYWNrZ3JvdW5kO1xuICAgIGlmICghZmcgJiYgdGhlbWU/LmNvbG9ycz8uW1wiZWRpdG9yLmZvcmVncm91bmRcIl0pXG4gICAgICBmZyA9IHRoZW1lLmNvbG9yc1tcImVkaXRvci5mb3JlZ3JvdW5kXCJdO1xuICAgIGlmICghYmcgJiYgdGhlbWU/LmNvbG9ycz8uW1wiZWRpdG9yLmJhY2tncm91bmRcIl0pXG4gICAgICBiZyA9IHRoZW1lLmNvbG9yc1tcImVkaXRvci5iYWNrZ3JvdW5kXCJdO1xuICAgIGlmICghZmcpXG4gICAgICBmZyA9IHRoZW1lLnR5cGUgPT09IFwibGlnaHRcIiA/IFZTQ09ERV9GQUxMQkFDS19FRElUT1JfRkcubGlnaHQgOiBWU0NPREVfRkFMTEJBQ0tfRURJVE9SX0ZHLmRhcms7XG4gICAgaWYgKCFiZylcbiAgICAgIGJnID0gdGhlbWUudHlwZSA9PT0gXCJsaWdodFwiID8gVlNDT0RFX0ZBTExCQUNLX0VESVRPUl9CRy5saWdodCA6IFZTQ09ERV9GQUxMQkFDS19FRElUT1JfQkcuZGFyaztcbiAgICB0aGVtZS5mZyA9IGZnO1xuICAgIHRoZW1lLmJnID0gYmc7XG4gIH1cbiAgaWYgKCEodGhlbWUuc2V0dGluZ3NbMF0gJiYgdGhlbWUuc2V0dGluZ3NbMF0uc2V0dGluZ3MgJiYgIXRoZW1lLnNldHRpbmdzWzBdLnNjb3BlKSkge1xuICAgIHRoZW1lLnNldHRpbmdzLnVuc2hpZnQoe1xuICAgICAgc2V0dGluZ3M6IHtcbiAgICAgICAgZm9yZWdyb3VuZDogdGhlbWUuZmcsXG4gICAgICAgIGJhY2tncm91bmQ6IHRoZW1lLmJnXG4gICAgICB9XG4gICAgfSk7XG4gIH1cbiAgbGV0IHJlcGxhY2VtZW50Q291bnQgPSAwO1xuICBjb25zdCByZXBsYWNlbWVudE1hcCA9IC8qIEBfX1BVUkVfXyAqLyBuZXcgTWFwKCk7XG4gIGZ1bmN0aW9uIGdldFJlcGxhY2VtZW50Q29sb3IodmFsdWUpIHtcbiAgICBpZiAocmVwbGFjZW1lbnRNYXAuaGFzKHZhbHVlKSlcbiAgICAgIHJldHVybiByZXBsYWNlbWVudE1hcC5nZXQodmFsdWUpO1xuICAgIHJlcGxhY2VtZW50Q291bnQgKz0gMTtcbiAgICBjb25zdCBoZXggPSBgIyR7cmVwbGFjZW1lbnRDb3VudC50b1N0cmluZygxNikucGFkU3RhcnQoOCwgXCIwXCIpLnRvTG93ZXJDYXNlKCl9YDtcbiAgICBpZiAodGhlbWUuY29sb3JSZXBsYWNlbWVudHM/LltgIyR7aGV4fWBdKVxuICAgICAgcmV0dXJuIGdldFJlcGxhY2VtZW50Q29sb3IodmFsdWUpO1xuICAgIHJlcGxhY2VtZW50TWFwLnNldCh2YWx1ZSwgaGV4KTtcbiAgICByZXR1cm4gaGV4O1xuICB9XG4gIHRoZW1lLnNldHRpbmdzID0gdGhlbWUuc2V0dGluZ3MubWFwKChzZXR0aW5nKSA9PiB7XG4gICAgY29uc3QgcmVwbGFjZUZnID0gc2V0dGluZy5zZXR0aW5ncz8uZm9yZWdyb3VuZCAmJiAhc2V0dGluZy5zZXR0aW5ncy5mb3JlZ3JvdW5kLnN0YXJ0c1dpdGgoXCIjXCIpO1xuICAgIGNvbnN0IHJlcGxhY2VCZyA9IHNldHRpbmcuc2V0dGluZ3M/LmJhY2tncm91bmQgJiYgIXNldHRpbmcuc2V0dGluZ3MuYmFja2dyb3VuZC5zdGFydHNXaXRoKFwiI1wiKTtcbiAgICBpZiAoIXJlcGxhY2VGZyAmJiAhcmVwbGFjZUJnKVxuICAgICAgcmV0dXJuIHNldHRpbmc7XG4gICAgY29uc3QgY2xvbmUgPSB7XG4gICAgICAuLi5zZXR0aW5nLFxuICAgICAgc2V0dGluZ3M6IHtcbiAgICAgICAgLi4uc2V0dGluZy5zZXR0aW5nc1xuICAgICAgfVxuICAgIH07XG4gICAgaWYgKHJlcGxhY2VGZykge1xuICAgICAgY29uc3QgcmVwbGFjZW1lbnQgPSBnZXRSZXBsYWNlbWVudENvbG9yKHNldHRpbmcuc2V0dGluZ3MuZm9yZWdyb3VuZCk7XG4gICAgICB0aGVtZS5jb2xvclJlcGxhY2VtZW50c1tyZXBsYWNlbWVudF0gPSBzZXR0aW5nLnNldHRpbmdzLmZvcmVncm91bmQ7XG4gICAgICBjbG9uZS5zZXR0aW5ncy5mb3JlZ3JvdW5kID0gcmVwbGFjZW1lbnQ7XG4gICAgfVxuICAgIGlmIChyZXBsYWNlQmcpIHtcbiAgICAgIGNvbnN0IHJlcGxhY2VtZW50ID0gZ2V0UmVwbGFjZW1lbnRDb2xvcihzZXR0aW5nLnNldHRpbmdzLmJhY2tncm91bmQpO1xuICAgICAgdGhlbWUuY29sb3JSZXBsYWNlbWVudHNbcmVwbGFjZW1lbnRdID0gc2V0dGluZy5zZXR0aW5ncy5iYWNrZ3JvdW5kO1xuICAgICAgY2xvbmUuc2V0dGluZ3MuYmFja2dyb3VuZCA9IHJlcGxhY2VtZW50O1xuICAgIH1cbiAgICByZXR1cm4gY2xvbmU7XG4gIH0pO1xuICBmb3IgKGNvbnN0IGtleSBvZiBPYmplY3Qua2V5cyh0aGVtZS5jb2xvcnMgfHwge30pKSB7XG4gICAgaWYgKGtleSA9PT0gXCJlZGl0b3IuZm9yZWdyb3VuZFwiIHx8IGtleSA9PT0gXCJlZGl0b3IuYmFja2dyb3VuZFwiIHx8IGtleS5zdGFydHNXaXRoKFwidGVybWluYWwuYW5zaVwiKSkge1xuICAgICAgaWYgKCF0aGVtZS5jb2xvcnNba2V5XT8uc3RhcnRzV2l0aChcIiNcIikpIHtcbiAgICAgICAgY29uc3QgcmVwbGFjZW1lbnQgPSBnZXRSZXBsYWNlbWVudENvbG9yKHRoZW1lLmNvbG9yc1trZXldKTtcbiAgICAgICAgdGhlbWUuY29sb3JSZXBsYWNlbWVudHNbcmVwbGFjZW1lbnRdID0gdGhlbWUuY29sb3JzW2tleV07XG4gICAgICAgIHRoZW1lLmNvbG9yc1trZXldID0gcmVwbGFjZW1lbnQ7XG4gICAgICB9XG4gICAgfVxuICB9XG4gIE9iamVjdC5kZWZpbmVQcm9wZXJ0eSh0aGVtZSwgUkVTT0xWRURfS0VZLCB7XG4gICAgZW51bWVyYWJsZTogZmFsc2UsXG4gICAgd3JpdGFibGU6IGZhbHNlLFxuICAgIHZhbHVlOiB0cnVlXG4gIH0pO1xuICByZXR1cm4gdGhlbWU7XG59XG5cbmFzeW5jIGZ1bmN0aW9uIHJlc29sdmVMYW5ncyhsYW5ncykge1xuICByZXR1cm4gQXJyYXkuZnJvbShuZXcgU2V0KChhd2FpdCBQcm9taXNlLmFsbChcbiAgICBsYW5ncy5maWx0ZXIoKGwpID0+ICFpc1NwZWNpYWxMYW5nKGwpKS5tYXAoYXN5bmMgKGxhbmcpID0+IGF3YWl0IG5vcm1hbGl6ZUdldHRlcihsYW5nKS50aGVuKChyKSA9PiBBcnJheS5pc0FycmF5KHIpID8gciA6IFtyXSkpXG4gICkpLmZsYXQoKSkpO1xufVxuYXN5bmMgZnVuY3Rpb24gcmVzb2x2ZVRoZW1lcyh0aGVtZXMpIHtcbiAgY29uc3QgcmVzb2x2ZWQgPSBhd2FpdCBQcm9taXNlLmFsbChcbiAgICB0aGVtZXMubWFwKFxuICAgICAgYXN5bmMgKHRoZW1lKSA9PiBpc1NwZWNpYWxUaGVtZSh0aGVtZSkgPyBudWxsIDogbm9ybWFsaXplVGhlbWUoYXdhaXQgbm9ybWFsaXplR2V0dGVyKHRoZW1lKSlcbiAgICApXG4gICk7XG4gIHJldHVybiByZXNvbHZlZC5maWx0ZXIoKGkpID0+ICEhaSk7XG59XG5cbmNsYXNzIFJlZ2lzdHJ5IGV4dGVuZHMgUmVnaXN0cnkkMSB7XG4gIGNvbnN0cnVjdG9yKF9yZXNvbHZlciwgX3RoZW1lcywgX2xhbmdzLCBfYWxpYXMgPSB7fSkge1xuICAgIHN1cGVyKF9yZXNvbHZlcik7XG4gICAgdGhpcy5fcmVzb2x2ZXIgPSBfcmVzb2x2ZXI7XG4gICAgdGhpcy5fdGhlbWVzID0gX3RoZW1lcztcbiAgICB0aGlzLl9sYW5ncyA9IF9sYW5ncztcbiAgICB0aGlzLl9hbGlhcyA9IF9hbGlhcztcbiAgICB0aGlzLl90aGVtZXMubWFwKCh0KSA9PiB0aGlzLmxvYWRUaGVtZSh0KSk7XG4gICAgdGhpcy5sb2FkTGFuZ3VhZ2VzKHRoaXMuX2xhbmdzKTtcbiAgfVxuICBfcmVzb2x2ZWRUaGVtZXMgPSAvKiBAX19QVVJFX18gKi8gbmV3IE1hcCgpO1xuICBfcmVzb2x2ZWRHcmFtbWFycyA9IC8qIEBfX1BVUkVfXyAqLyBuZXcgTWFwKCk7XG4gIF9sYW5nTWFwID0gLyogQF9fUFVSRV9fICovIG5ldyBNYXAoKTtcbiAgX2xhbmdHcmFwaCA9IC8qIEBfX1BVUkVfXyAqLyBuZXcgTWFwKCk7XG4gIF90ZXh0bWF0ZVRoZW1lQ2FjaGUgPSAvKiBAX19QVVJFX18gKi8gbmV3IFdlYWtNYXAoKTtcbiAgX2xvYWRlZFRoZW1lc0NhY2hlID0gbnVsbDtcbiAgX2xvYWRlZExhbmd1YWdlc0NhY2hlID0gbnVsbDtcbiAgZ2V0VGhlbWUodGhlbWUpIHtcbiAgICBpZiAodHlwZW9mIHRoZW1lID09PSBcInN0cmluZ1wiKVxuICAgICAgcmV0dXJuIHRoaXMuX3Jlc29sdmVkVGhlbWVzLmdldCh0aGVtZSk7XG4gICAgZWxzZVxuICAgICAgcmV0dXJuIHRoaXMubG9hZFRoZW1lKHRoZW1lKTtcbiAgfVxuICBsb2FkVGhlbWUodGhlbWUpIHtcbiAgICBjb25zdCBfdGhlbWUgPSBub3JtYWxpemVUaGVtZSh0aGVtZSk7XG4gICAgaWYgKF90aGVtZS5uYW1lKSB7XG4gICAgICB0aGlzLl9yZXNvbHZlZFRoZW1lcy5zZXQoX3RoZW1lLm5hbWUsIF90aGVtZSk7XG4gICAgICB0aGlzLl9sb2FkZWRUaGVtZXNDYWNoZSA9IG51bGw7XG4gICAgfVxuICAgIHJldHVybiBfdGhlbWU7XG4gIH1cbiAgZ2V0TG9hZGVkVGhlbWVzKCkge1xuICAgIGlmICghdGhpcy5fbG9hZGVkVGhlbWVzQ2FjaGUpXG4gICAgICB0aGlzLl9sb2FkZWRUaGVtZXNDYWNoZSA9IFsuLi50aGlzLl9yZXNvbHZlZFRoZW1lcy5rZXlzKCldO1xuICAgIHJldHVybiB0aGlzLl9sb2FkZWRUaGVtZXNDYWNoZTtcbiAgfVxuICAvLyBPdmVycmlkZSBhbmQgcmUtaW1wbGVtZW50IHRoaXMgbWV0aG9kIHRvIGNhY2hlIHRoZSB0ZXh0bWF0ZSB0aGVtZXMgYXMgYFRleHRNYXRlVGhlbWUuY3JlYXRlRnJvbVJhd1RoZW1lYFxuICAvLyBpcyBleHBlbnNpdmUuIFRoZW1lcyBjYW4gc3dpdGNoIG9mdGVuIGVzcGVjaWFsbHkgZm9yIGR1YWwtdGhlbWUgc3VwcG9ydC5cbiAgLy9cbiAgLy8gVGhlIHBhcmVudCBjbGFzcyBhbHNvIGFjY2VwdHMgYGNvbG9yTWFwYCBhcyB0aGUgc2Vjb25kIHBhcmFtZXRlciwgYnV0IHNpbmNlIHdlIGRvbid0IHVzZSB0aGF0LFxuICAvLyB3ZSBvbWl0IGhlcmUgc28gaXQncyBlYXNpZXIgdG8gY2FjaGUgdGhlIHRoZW1lcy5cbiAgc2V0VGhlbWUodGhlbWUpIHtcbiAgICBsZXQgdGV4dG1hdGVUaGVtZSA9IHRoaXMuX3RleHRtYXRlVGhlbWVDYWNoZS5nZXQodGhlbWUpO1xuICAgIGlmICghdGV4dG1hdGVUaGVtZSkge1xuICAgICAgdGV4dG1hdGVUaGVtZSA9IFRoZW1lLmNyZWF0ZUZyb21SYXdUaGVtZSh0aGVtZSk7XG4gICAgICB0aGlzLl90ZXh0bWF0ZVRoZW1lQ2FjaGUuc2V0KHRoZW1lLCB0ZXh0bWF0ZVRoZW1lKTtcbiAgICB9XG4gICAgdGhpcy5fc3luY1JlZ2lzdHJ5LnNldFRoZW1lKHRleHRtYXRlVGhlbWUpO1xuICB9XG4gIGdldEdyYW1tYXIobmFtZSkge1xuICAgIGlmICh0aGlzLl9hbGlhc1tuYW1lXSkge1xuICAgICAgY29uc3QgcmVzb2x2ZWQgPSAvKiBAX19QVVJFX18gKi8gbmV3IFNldChbbmFtZV0pO1xuICAgICAgd2hpbGUgKHRoaXMuX2FsaWFzW25hbWVdKSB7XG4gICAgICAgIG5hbWUgPSB0aGlzLl9hbGlhc1tuYW1lXTtcbiAgICAgICAgaWYgKHJlc29sdmVkLmhhcyhuYW1lKSlcbiAgICAgICAgICB0aHJvdyBuZXcgU2hpa2lFcnJvcihgQ2lyY3VsYXIgYWxpYXMgXFxgJHtBcnJheS5mcm9tKHJlc29sdmVkKS5qb2luKFwiIC0+IFwiKX0gLT4gJHtuYW1lfVxcYGApO1xuICAgICAgICByZXNvbHZlZC5hZGQobmFtZSk7XG4gICAgICB9XG4gICAgfVxuICAgIHJldHVybiB0aGlzLl9yZXNvbHZlZEdyYW1tYXJzLmdldChuYW1lKTtcbiAgfVxuICBsb2FkTGFuZ3VhZ2UobGFuZykge1xuICAgIGlmICh0aGlzLmdldEdyYW1tYXIobGFuZy5uYW1lKSlcbiAgICAgIHJldHVybjtcbiAgICBjb25zdCBlbWJlZGRlZExhemlseUJ5ID0gbmV3IFNldChcbiAgICAgIFsuLi50aGlzLl9sYW5nTWFwLnZhbHVlcygpXS5maWx0ZXIoKGkpID0+IGkuZW1iZWRkZWRMYW5nc0xhenk/LmluY2x1ZGVzKGxhbmcubmFtZSkpXG4gICAgKTtcbiAgICB0aGlzLl9yZXNvbHZlci5hZGRMYW5ndWFnZShsYW5nKTtcbiAgICBjb25zdCBncmFtbWFyQ29uZmlnID0ge1xuICAgICAgYmFsYW5jZWRCcmFja2V0U2VsZWN0b3JzOiBsYW5nLmJhbGFuY2VkQnJhY2tldFNlbGVjdG9ycyB8fCBbXCIqXCJdLFxuICAgICAgdW5iYWxhbmNlZEJyYWNrZXRTZWxlY3RvcnM6IGxhbmcudW5iYWxhbmNlZEJyYWNrZXRTZWxlY3RvcnMgfHwgW11cbiAgICB9O1xuICAgIHRoaXMuX3N5bmNSZWdpc3RyeS5fcmF3R3JhbW1hcnMuc2V0KGxhbmcuc2NvcGVOYW1lLCBsYW5nKTtcbiAgICBjb25zdCBnID0gdGhpcy5sb2FkR3JhbW1hcldpdGhDb25maWd1cmF0aW9uKGxhbmcuc2NvcGVOYW1lLCAxLCBncmFtbWFyQ29uZmlnKTtcbiAgICBnLm5hbWUgPSBsYW5nLm5hbWU7XG4gICAgdGhpcy5fcmVzb2x2ZWRHcmFtbWFycy5zZXQobGFuZy5uYW1lLCBnKTtcbiAgICBpZiAobGFuZy5hbGlhc2VzKSB7XG4gICAgICBsYW5nLmFsaWFzZXMuZm9yRWFjaCgoYWxpYXMpID0+IHtcbiAgICAgICAgdGhpcy5fYWxpYXNbYWxpYXNdID0gbGFuZy5uYW1lO1xuICAgICAgfSk7XG4gICAgfVxuICAgIHRoaXMuX2xvYWRlZExhbmd1YWdlc0NhY2hlID0gbnVsbDtcbiAgICBpZiAoZW1iZWRkZWRMYXppbHlCeS5zaXplKSB7XG4gICAgICBmb3IgKGNvbnN0IGUgb2YgZW1iZWRkZWRMYXppbHlCeSkge1xuICAgICAgICB0aGlzLl9yZXNvbHZlZEdyYW1tYXJzLmRlbGV0ZShlLm5hbWUpO1xuICAgICAgICB0aGlzLl9sb2FkZWRMYW5ndWFnZXNDYWNoZSA9IG51bGw7XG4gICAgICAgIHRoaXMuX3N5bmNSZWdpc3RyeT8uX2luamVjdGlvbkdyYW1tYXJzPy5kZWxldGUoZS5zY29wZU5hbWUpO1xuICAgICAgICB0aGlzLl9zeW5jUmVnaXN0cnk/Ll9ncmFtbWFycz8uZGVsZXRlKGUuc2NvcGVOYW1lKTtcbiAgICAgICAgdGhpcy5sb2FkTGFuZ3VhZ2UodGhpcy5fbGFuZ01hcC5nZXQoZS5uYW1lKSk7XG4gICAgICB9XG4gICAgfVxuICB9XG4gIGRpc3Bvc2UoKSB7XG4gICAgc3VwZXIuZGlzcG9zZSgpO1xuICAgIHRoaXMuX3Jlc29sdmVkVGhlbWVzLmNsZWFyKCk7XG4gICAgdGhpcy5fcmVzb2x2ZWRHcmFtbWFycy5jbGVhcigpO1xuICAgIHRoaXMuX2xhbmdNYXAuY2xlYXIoKTtcbiAgICB0aGlzLl9sYW5nR3JhcGguY2xlYXIoKTtcbiAgICB0aGlzLl9sb2FkZWRUaGVtZXNDYWNoZSA9IG51bGw7XG4gIH1cbiAgbG9hZExhbmd1YWdlcyhsYW5ncykge1xuICAgIGZvciAoY29uc3QgbGFuZyBvZiBsYW5ncylcbiAgICAgIHRoaXMucmVzb2x2ZUVtYmVkZGVkTGFuZ3VhZ2VzKGxhbmcpO1xuICAgIGNvbnN0IGxhbmdzR3JhcGhBcnJheSA9IEFycmF5LmZyb20odGhpcy5fbGFuZ0dyYXBoLmVudHJpZXMoKSk7XG4gICAgY29uc3QgbWlzc2luZ0xhbmdzID0gbGFuZ3NHcmFwaEFycmF5LmZpbHRlcigoW18sIGxhbmddKSA9PiAhbGFuZyk7XG4gICAgaWYgKG1pc3NpbmdMYW5ncy5sZW5ndGgpIHtcbiAgICAgIGNvbnN0IGRlcGVuZGVudHMgPSBsYW5nc0dyYXBoQXJyYXkuZmlsdGVyKChbXywgbGFuZ10pID0+IGxhbmcgJiYgbGFuZy5lbWJlZGRlZExhbmdzPy5zb21lKChsKSA9PiBtaXNzaW5nTGFuZ3MubWFwKChbbmFtZV0pID0+IG5hbWUpLmluY2x1ZGVzKGwpKSkuZmlsdGVyKChsYW5nKSA9PiAhbWlzc2luZ0xhbmdzLmluY2x1ZGVzKGxhbmcpKTtcbiAgICAgIHRocm93IG5ldyBTaGlraUVycm9yKGBNaXNzaW5nIGxhbmd1YWdlcyAke21pc3NpbmdMYW5ncy5tYXAoKFtuYW1lXSkgPT4gYFxcYCR7bmFtZX1cXGBgKS5qb2luKFwiLCBcIil9LCByZXF1aXJlZCBieSAke2RlcGVuZGVudHMubWFwKChbbmFtZV0pID0+IGBcXGAke25hbWV9XFxgYCkuam9pbihcIiwgXCIpfWApO1xuICAgIH1cbiAgICBmb3IgKGNvbnN0IFtfLCBsYW5nXSBvZiBsYW5nc0dyYXBoQXJyYXkpXG4gICAgICB0aGlzLl9yZXNvbHZlci5hZGRMYW5ndWFnZShsYW5nKTtcbiAgICBmb3IgKGNvbnN0IFtfLCBsYW5nXSBvZiBsYW5nc0dyYXBoQXJyYXkpXG4gICAgICB0aGlzLmxvYWRMYW5ndWFnZShsYW5nKTtcbiAgfVxuICBnZXRMb2FkZWRMYW5ndWFnZXMoKSB7XG4gICAgaWYgKCF0aGlzLl9sb2FkZWRMYW5ndWFnZXNDYWNoZSkge1xuICAgICAgdGhpcy5fbG9hZGVkTGFuZ3VhZ2VzQ2FjaGUgPSBbXG4gICAgICAgIC4uLi8qIEBfX1BVUkVfXyAqLyBuZXcgU2V0KFsuLi50aGlzLl9yZXNvbHZlZEdyYW1tYXJzLmtleXMoKSwgLi4uT2JqZWN0LmtleXModGhpcy5fYWxpYXMpXSlcbiAgICAgIF07XG4gICAgfVxuICAgIHJldHVybiB0aGlzLl9sb2FkZWRMYW5ndWFnZXNDYWNoZTtcbiAgfVxuICByZXNvbHZlRW1iZWRkZWRMYW5ndWFnZXMobGFuZykge1xuICAgIHRoaXMuX2xhbmdNYXAuc2V0KGxhbmcubmFtZSwgbGFuZyk7XG4gICAgdGhpcy5fbGFuZ0dyYXBoLnNldChsYW5nLm5hbWUsIGxhbmcpO1xuICAgIGlmIChsYW5nLmVtYmVkZGVkTGFuZ3MpIHtcbiAgICAgIGZvciAoY29uc3QgZW1iZWRkZWRMYW5nIG9mIGxhbmcuZW1iZWRkZWRMYW5ncylcbiAgICAgICAgdGhpcy5fbGFuZ0dyYXBoLnNldChlbWJlZGRlZExhbmcsIHRoaXMuX2xhbmdNYXAuZ2V0KGVtYmVkZGVkTGFuZykpO1xuICAgIH1cbiAgfVxufVxuXG5jbGFzcyBSZXNvbHZlciB7XG4gIF9sYW5ncyA9IC8qIEBfX1BVUkVfXyAqLyBuZXcgTWFwKCk7XG4gIF9zY29wZVRvTGFuZyA9IC8qIEBfX1BVUkVfXyAqLyBuZXcgTWFwKCk7XG4gIF9pbmplY3Rpb25zID0gLyogQF9fUFVSRV9fICovIG5ldyBNYXAoKTtcbiAgX29uaWdMaWI7XG4gIGNvbnN0cnVjdG9yKGVuZ2luZSwgbGFuZ3MpIHtcbiAgICB0aGlzLl9vbmlnTGliID0ge1xuICAgICAgY3JlYXRlT25pZ1NjYW5uZXI6IChwYXR0ZXJucykgPT4gZW5naW5lLmNyZWF0ZVNjYW5uZXIocGF0dGVybnMpLFxuICAgICAgY3JlYXRlT25pZ1N0cmluZzogKHMpID0+IGVuZ2luZS5jcmVhdGVTdHJpbmcocylcbiAgICB9O1xuICAgIGxhbmdzLmZvckVhY2goKGkpID0+IHRoaXMuYWRkTGFuZ3VhZ2UoaSkpO1xuICB9XG4gIGdldCBvbmlnTGliKCkge1xuICAgIHJldHVybiB0aGlzLl9vbmlnTGliO1xuICB9XG4gIGdldExhbmdSZWdpc3RyYXRpb24obGFuZ0lkT3JBbGlhcykge1xuICAgIHJldHVybiB0aGlzLl9sYW5ncy5nZXQobGFuZ0lkT3JBbGlhcyk7XG4gIH1cbiAgbG9hZEdyYW1tYXIoc2NvcGVOYW1lKSB7XG4gICAgcmV0dXJuIHRoaXMuX3Njb3BlVG9MYW5nLmdldChzY29wZU5hbWUpO1xuICB9XG4gIGFkZExhbmd1YWdlKGwpIHtcbiAgICB0aGlzLl9sYW5ncy5zZXQobC5uYW1lLCBsKTtcbiAgICBpZiAobC5hbGlhc2VzKSB7XG4gICAgICBsLmFsaWFzZXMuZm9yRWFjaCgoYSkgPT4ge1xuICAgICAgICB0aGlzLl9sYW5ncy5zZXQoYSwgbCk7XG4gICAgICB9KTtcbiAgICB9XG4gICAgdGhpcy5fc2NvcGVUb0xhbmcuc2V0KGwuc2NvcGVOYW1lLCBsKTtcbiAgICBpZiAobC5pbmplY3RUbykge1xuICAgICAgbC5pbmplY3RUby5mb3JFYWNoKChpKSA9PiB7XG4gICAgICAgIGlmICghdGhpcy5faW5qZWN0aW9ucy5nZXQoaSkpXG4gICAgICAgICAgdGhpcy5faW5qZWN0aW9ucy5zZXQoaSwgW10pO1xuICAgICAgICB0aGlzLl9pbmplY3Rpb25zLmdldChpKS5wdXNoKGwuc2NvcGVOYW1lKTtcbiAgICAgIH0pO1xuICAgIH1cbiAgfVxuICBnZXRJbmplY3Rpb25zKHNjb3BlTmFtZSkge1xuICAgIGNvbnN0IHNjb3BlUGFydHMgPSBzY29wZU5hbWUuc3BsaXQoXCIuXCIpO1xuICAgIGxldCBpbmplY3Rpb25zID0gW107XG4gICAgZm9yIChsZXQgaSA9IDE7IGkgPD0gc2NvcGVQYXJ0cy5sZW5ndGg7IGkrKykge1xuICAgICAgY29uc3Qgc3ViU2NvcGVOYW1lID0gc2NvcGVQYXJ0cy5zbGljZSgwLCBpKS5qb2luKFwiLlwiKTtcbiAgICAgIGluamVjdGlvbnMgPSBbLi4uaW5qZWN0aW9ucywgLi4udGhpcy5faW5qZWN0aW9ucy5nZXQoc3ViU2NvcGVOYW1lKSB8fCBbXV07XG4gICAgfVxuICAgIHJldHVybiBpbmplY3Rpb25zO1xuICB9XG59XG5cbmxldCBpbnN0YW5jZXNDb3VudCA9IDA7XG5mdW5jdGlvbiBjcmVhdGVTaGlraUludGVybmFsU3luYyhvcHRpb25zKSB7XG4gIGluc3RhbmNlc0NvdW50ICs9IDE7XG4gIGlmIChvcHRpb25zLndhcm5pbmdzICE9PSBmYWxzZSAmJiBpbnN0YW5jZXNDb3VudCA+PSAxMCAmJiBpbnN0YW5jZXNDb3VudCAlIDEwID09PSAwKVxuICAgIGNvbnNvbGUud2FybihgW1NoaWtpXSAke2luc3RhbmNlc0NvdW50fSBpbnN0YW5jZXMgaGF2ZSBiZWVuIGNyZWF0ZWQuIFNoaWtpIGlzIHN1cHBvc2VkIHRvIGJlIHVzZWQgYXMgYSBzaW5nbGV0b24sIGNvbnNpZGVyIHJlZmFjdG9yaW5nIHlvdXIgY29kZSB0byBjYWNoZSB5b3VyIGhpZ2hsaWdodGVyIGluc3RhbmNlOyBPciBjYWxsIFxcYGhpZ2hsaWdodGVyLmRpc3Bvc2UoKVxcYCB0byByZWxlYXNlIHVudXNlZCBpbnN0YW5jZXMuYCk7XG4gIGxldCBpc0Rpc3Bvc2VkID0gZmFsc2U7XG4gIGlmICghb3B0aW9ucy5lbmdpbmUpXG4gICAgdGhyb3cgbmV3IFNoaWtpRXJyb3IoXCJgZW5naW5lYCBvcHRpb24gaXMgcmVxdWlyZWQgZm9yIHN5bmNocm9ub3VzIG1vZGVcIik7XG4gIGNvbnN0IGxhbmdzID0gKG9wdGlvbnMubGFuZ3MgfHwgW10pLmZsYXQoMSk7XG4gIGNvbnN0IHRoZW1lcyA9IChvcHRpb25zLnRoZW1lcyB8fCBbXSkuZmxhdCgxKS5tYXAobm9ybWFsaXplVGhlbWUpO1xuICBjb25zdCByZXNvbHZlciA9IG5ldyBSZXNvbHZlcihvcHRpb25zLmVuZ2luZSwgbGFuZ3MpO1xuICBjb25zdCBfcmVnaXN0cnkgPSBuZXcgUmVnaXN0cnkocmVzb2x2ZXIsIHRoZW1lcywgbGFuZ3MsIG9wdGlvbnMubGFuZ0FsaWFzKTtcbiAgbGV0IF9sYXN0VGhlbWU7XG4gIGZ1bmN0aW9uIGdldExhbmd1YWdlKG5hbWUpIHtcbiAgICBlbnN1cmVOb3REaXNwb3NlZCgpO1xuICAgIGNvbnN0IF9sYW5nID0gX3JlZ2lzdHJ5LmdldEdyYW1tYXIodHlwZW9mIG5hbWUgPT09IFwic3RyaW5nXCIgPyBuYW1lIDogbmFtZS5uYW1lKTtcbiAgICBpZiAoIV9sYW5nKVxuICAgICAgdGhyb3cgbmV3IFNoaWtpRXJyb3IoYExhbmd1YWdlIFxcYCR7bmFtZX1cXGAgbm90IGZvdW5kLCB5b3UgbWF5IG5lZWQgdG8gbG9hZCBpdCBmaXJzdGApO1xuICAgIHJldHVybiBfbGFuZztcbiAgfVxuICBmdW5jdGlvbiBnZXRUaGVtZShuYW1lKSB7XG4gICAgaWYgKG5hbWUgPT09IFwibm9uZVwiKVxuICAgICAgcmV0dXJuIHsgYmc6IFwiXCIsIGZnOiBcIlwiLCBuYW1lOiBcIm5vbmVcIiwgc2V0dGluZ3M6IFtdLCB0eXBlOiBcImRhcmtcIiB9O1xuICAgIGVuc3VyZU5vdERpc3Bvc2VkKCk7XG4gICAgY29uc3QgX3RoZW1lID0gX3JlZ2lzdHJ5LmdldFRoZW1lKG5hbWUpO1xuICAgIGlmICghX3RoZW1lKVxuICAgICAgdGhyb3cgbmV3IFNoaWtpRXJyb3IoYFRoZW1lIFxcYCR7bmFtZX1cXGAgbm90IGZvdW5kLCB5b3UgbWF5IG5lZWQgdG8gbG9hZCBpdCBmaXJzdGApO1xuICAgIHJldHVybiBfdGhlbWU7XG4gIH1cbiAgZnVuY3Rpb24gc2V0VGhlbWUobmFtZSkge1xuICAgIGVuc3VyZU5vdERpc3Bvc2VkKCk7XG4gICAgY29uc3QgdGhlbWUgPSBnZXRUaGVtZShuYW1lKTtcbiAgICBpZiAoX2xhc3RUaGVtZSAhPT0gbmFtZSkge1xuICAgICAgX3JlZ2lzdHJ5LnNldFRoZW1lKHRoZW1lKTtcbiAgICAgIF9sYXN0VGhlbWUgPSBuYW1lO1xuICAgIH1cbiAgICBjb25zdCBjb2xvck1hcCA9IF9yZWdpc3RyeS5nZXRDb2xvck1hcCgpO1xuICAgIHJldHVybiB7XG4gICAgICB0aGVtZSxcbiAgICAgIGNvbG9yTWFwXG4gICAgfTtcbiAgfVxuICBmdW5jdGlvbiBnZXRMb2FkZWRUaGVtZXMoKSB7XG4gICAgZW5zdXJlTm90RGlzcG9zZWQoKTtcbiAgICByZXR1cm4gX3JlZ2lzdHJ5LmdldExvYWRlZFRoZW1lcygpO1xuICB9XG4gIGZ1bmN0aW9uIGdldExvYWRlZExhbmd1YWdlcygpIHtcbiAgICBlbnN1cmVOb3REaXNwb3NlZCgpO1xuICAgIHJldHVybiBfcmVnaXN0cnkuZ2V0TG9hZGVkTGFuZ3VhZ2VzKCk7XG4gIH1cbiAgZnVuY3Rpb24gbG9hZExhbmd1YWdlU3luYyguLi5sYW5nczIpIHtcbiAgICBlbnN1cmVOb3REaXNwb3NlZCgpO1xuICAgIF9yZWdpc3RyeS5sb2FkTGFuZ3VhZ2VzKGxhbmdzMi5mbGF0KDEpKTtcbiAgfVxuICBhc3luYyBmdW5jdGlvbiBsb2FkTGFuZ3VhZ2UoLi4ubGFuZ3MyKSB7XG4gICAgcmV0dXJuIGxvYWRMYW5ndWFnZVN5bmMoYXdhaXQgcmVzb2x2ZUxhbmdzKGxhbmdzMikpO1xuICB9XG4gIGZ1bmN0aW9uIGxvYWRUaGVtZVN5bmMoLi4udGhlbWVzMikge1xuICAgIGVuc3VyZU5vdERpc3Bvc2VkKCk7XG4gICAgZm9yIChjb25zdCB0aGVtZSBvZiB0aGVtZXMyLmZsYXQoMSkpIHtcbiAgICAgIF9yZWdpc3RyeS5sb2FkVGhlbWUodGhlbWUpO1xuICAgIH1cbiAgfVxuICBhc3luYyBmdW5jdGlvbiBsb2FkVGhlbWUoLi4udGhlbWVzMikge1xuICAgIGVuc3VyZU5vdERpc3Bvc2VkKCk7XG4gICAgcmV0dXJuIGxvYWRUaGVtZVN5bmMoYXdhaXQgcmVzb2x2ZVRoZW1lcyh0aGVtZXMyKSk7XG4gIH1cbiAgZnVuY3Rpb24gZW5zdXJlTm90RGlzcG9zZWQoKSB7XG4gICAgaWYgKGlzRGlzcG9zZWQpXG4gICAgICB0aHJvdyBuZXcgU2hpa2lFcnJvcihcIlNoaWtpIGluc3RhbmNlIGhhcyBiZWVuIGRpc3Bvc2VkXCIpO1xuICB9XG4gIGZ1bmN0aW9uIGRpc3Bvc2UoKSB7XG4gICAgaWYgKGlzRGlzcG9zZWQpXG4gICAgICByZXR1cm47XG4gICAgaXNEaXNwb3NlZCA9IHRydWU7XG4gICAgX3JlZ2lzdHJ5LmRpc3Bvc2UoKTtcbiAgICBpbnN0YW5jZXNDb3VudCAtPSAxO1xuICB9XG4gIHJldHVybiB7XG4gICAgc2V0VGhlbWUsXG4gICAgZ2V0VGhlbWUsXG4gICAgZ2V0TGFuZ3VhZ2UsXG4gICAgZ2V0TG9hZGVkVGhlbWVzLFxuICAgIGdldExvYWRlZExhbmd1YWdlcyxcbiAgICBsb2FkTGFuZ3VhZ2UsXG4gICAgbG9hZExhbmd1YWdlU3luYyxcbiAgICBsb2FkVGhlbWUsXG4gICAgbG9hZFRoZW1lU3luYyxcbiAgICBkaXNwb3NlLFxuICAgIFtTeW1ib2wuZGlzcG9zZV06IGRpc3Bvc2VcbiAgfTtcbn1cblxuYXN5bmMgZnVuY3Rpb24gY3JlYXRlU2hpa2lJbnRlcm5hbChvcHRpb25zID0ge30pIHtcbiAgaWYgKG9wdGlvbnMubG9hZFdhc20pIHtcbiAgICB3YXJuRGVwcmVjYXRlZChcImBsb2FkV2FzbWAgb3B0aW9uIGlzIGRlcHJlY2F0ZWQuIFVzZSBgZW5naW5lOiBjcmVhdGVPbmlndXJ1bWFFbmdpbmUobG9hZFdhc20pYCBpbnN0ZWFkLlwiKTtcbiAgfVxuICBjb25zdCBbXG4gICAgdGhlbWVzLFxuICAgIGxhbmdzLFxuICAgIGVuZ2luZVxuICBdID0gYXdhaXQgUHJvbWlzZS5hbGwoW1xuICAgIHJlc29sdmVUaGVtZXMob3B0aW9ucy50aGVtZXMgfHwgW10pLFxuICAgIHJlc29sdmVMYW5ncyhvcHRpb25zLmxhbmdzIHx8IFtdKSxcbiAgICBvcHRpb25zLmVuZ2luZSB8fCBjcmVhdGVPbmlndXJ1bWFFbmdpbmUkMShvcHRpb25zLmxvYWRXYXNtIHx8IGdldERlZmF1bHRXYXNtTG9hZGVyKCkpXG4gIF0pO1xuICByZXR1cm4gY3JlYXRlU2hpa2lJbnRlcm5hbFN5bmMoe1xuICAgIC4uLm9wdGlvbnMsXG4gICAgbG9hZFdhc206IHZvaWQgMCxcbiAgICB0aGVtZXMsXG4gICAgbGFuZ3MsXG4gICAgZW5naW5lXG4gIH0pO1xufVxuZnVuY3Rpb24gZ2V0U2hpa2lJbnRlcm5hbChvcHRpb25zID0ge30pIHtcbiAgd2FybkRlcHJlY2F0ZWQoXCJgZ2V0U2hpa2lJbnRlcm5hbGAgaXMgZGVwcmVjYXRlZC4gVXNlIGBjcmVhdGVTaGlraUludGVybmFsYCBpbnN0ZWFkLlwiKTtcbiAgcmV0dXJuIGNyZWF0ZVNoaWtpSW50ZXJuYWwob3B0aW9ucyk7XG59XG5cbmFzeW5jIGZ1bmN0aW9uIGNyZWF0ZUhpZ2hsaWdodGVyQ29yZShvcHRpb25zID0ge30pIHtcbiAgY29uc3QgaW50ZXJuYWwgPSBhd2FpdCBjcmVhdGVTaGlraUludGVybmFsKG9wdGlvbnMpO1xuICByZXR1cm4ge1xuICAgIGdldExhc3RHcmFtbWFyU3RhdGU6ICguLi5hcmdzKSA9PiBnZXRMYXN0R3JhbW1hclN0YXRlKGludGVybmFsLCAuLi5hcmdzKSxcbiAgICBjb2RlVG9Ub2tlbnNCYXNlOiAoY29kZSwgb3B0aW9uczIpID0+IGNvZGVUb1Rva2Vuc0Jhc2UoaW50ZXJuYWwsIGNvZGUsIG9wdGlvbnMyKSxcbiAgICBjb2RlVG9Ub2tlbnNXaXRoVGhlbWVzOiAoY29kZSwgb3B0aW9uczIpID0+IGNvZGVUb1Rva2Vuc1dpdGhUaGVtZXMoaW50ZXJuYWwsIGNvZGUsIG9wdGlvbnMyKSxcbiAgICBjb2RlVG9Ub2tlbnM6IChjb2RlLCBvcHRpb25zMikgPT4gY29kZVRvVG9rZW5zKGludGVybmFsLCBjb2RlLCBvcHRpb25zMiksXG4gICAgY29kZVRvSGFzdDogKGNvZGUsIG9wdGlvbnMyKSA9PiBjb2RlVG9IYXN0KGludGVybmFsLCBjb2RlLCBvcHRpb25zMiksXG4gICAgY29kZVRvSHRtbDogKGNvZGUsIG9wdGlvbnMyKSA9PiBjb2RlVG9IdG1sKGludGVybmFsLCBjb2RlLCBvcHRpb25zMiksXG4gICAgLi4uaW50ZXJuYWwsXG4gICAgZ2V0SW50ZXJuYWxDb250ZXh0OiAoKSA9PiBpbnRlcm5hbFxuICB9O1xufVxuZnVuY3Rpb24gY3JlYXRlSGlnaGxpZ2h0ZXJDb3JlU3luYyhvcHRpb25zID0ge30pIHtcbiAgY29uc3QgaW50ZXJuYWwgPSBjcmVhdGVTaGlraUludGVybmFsU3luYyhvcHRpb25zKTtcbiAgcmV0dXJuIHtcbiAgICBnZXRMYXN0R3JhbW1hclN0YXRlOiAoLi4uYXJncykgPT4gZ2V0TGFzdEdyYW1tYXJTdGF0ZShpbnRlcm5hbCwgLi4uYXJncyksXG4gICAgY29kZVRvVG9rZW5zQmFzZTogKGNvZGUsIG9wdGlvbnMyKSA9PiBjb2RlVG9Ub2tlbnNCYXNlKGludGVybmFsLCBjb2RlLCBvcHRpb25zMiksXG4gICAgY29kZVRvVG9rZW5zV2l0aFRoZW1lczogKGNvZGUsIG9wdGlvbnMyKSA9PiBjb2RlVG9Ub2tlbnNXaXRoVGhlbWVzKGludGVybmFsLCBjb2RlLCBvcHRpb25zMiksXG4gICAgY29kZVRvVG9rZW5zOiAoY29kZSwgb3B0aW9uczIpID0+IGNvZGVUb1Rva2VucyhpbnRlcm5hbCwgY29kZSwgb3B0aW9uczIpLFxuICAgIGNvZGVUb0hhc3Q6IChjb2RlLCBvcHRpb25zMikgPT4gY29kZVRvSGFzdChpbnRlcm5hbCwgY29kZSwgb3B0aW9uczIpLFxuICAgIGNvZGVUb0h0bWw6IChjb2RlLCBvcHRpb25zMikgPT4gY29kZVRvSHRtbChpbnRlcm5hbCwgY29kZSwgb3B0aW9uczIpLFxuICAgIC4uLmludGVybmFsLFxuICAgIGdldEludGVybmFsQ29udGV4dDogKCkgPT4gaW50ZXJuYWxcbiAgfTtcbn1cbmZ1bmN0aW9uIG1ha2VTaW5nbGV0b25IaWdobGlnaHRlckNvcmUoY3JlYXRlSGlnaGxpZ2h0ZXIpIHtcbiAgbGV0IF9zaGlraTtcbiAgYXN5bmMgZnVuY3Rpb24gZ2V0U2luZ2xldG9uSGlnaGxpZ2h0ZXJDb3JlMihvcHRpb25zID0ge30pIHtcbiAgICBpZiAoIV9zaGlraSkge1xuICAgICAgX3NoaWtpID0gY3JlYXRlSGlnaGxpZ2h0ZXIoe1xuICAgICAgICAuLi5vcHRpb25zLFxuICAgICAgICB0aGVtZXM6IG9wdGlvbnMudGhlbWVzIHx8IFtdLFxuICAgICAgICBsYW5nczogb3B0aW9ucy5sYW5ncyB8fCBbXVxuICAgICAgfSk7XG4gICAgICByZXR1cm4gX3NoaWtpO1xuICAgIH0gZWxzZSB7XG4gICAgICBjb25zdCBzID0gYXdhaXQgX3NoaWtpO1xuICAgICAgYXdhaXQgUHJvbWlzZS5hbGwoW1xuICAgICAgICBzLmxvYWRUaGVtZSguLi5vcHRpb25zLnRoZW1lcyB8fCBbXSksXG4gICAgICAgIHMubG9hZExhbmd1YWdlKC4uLm9wdGlvbnMubGFuZ3MgfHwgW10pXG4gICAgICBdKTtcbiAgICAgIHJldHVybiBzO1xuICAgIH1cbiAgfVxuICByZXR1cm4gZ2V0U2luZ2xldG9uSGlnaGxpZ2h0ZXJDb3JlMjtcbn1cbmNvbnN0IGdldFNpbmdsZXRvbkhpZ2hsaWdodGVyQ29yZSA9IC8qIEBfX1BVUkVfXyAqLyBtYWtlU2luZ2xldG9uSGlnaGxpZ2h0ZXJDb3JlKGNyZWF0ZUhpZ2hsaWdodGVyQ29yZSk7XG5mdW5jdGlvbiBnZXRIaWdobGlnaHRlckNvcmUob3B0aW9ucyA9IHt9KSB7XG4gIHdhcm5EZXByZWNhdGVkKFwiYGdldEhpZ2hsaWdodGVyQ29yZWAgaXMgZGVwcmVjYXRlZC4gVXNlIGBjcmVhdGVIaWdobGlnaHRlckNvcmVgIG9yIGBnZXRTaW5nbGV0b25IaWdobGlnaHRlckNvcmVgIGluc3RlYWQuXCIpO1xuICByZXR1cm4gY3JlYXRlSGlnaGxpZ2h0ZXJDb3JlKG9wdGlvbnMpO1xufVxuXG5mdW5jdGlvbiBjcmVhdGVkQnVuZGxlZEhpZ2hsaWdodGVyKGFyZzEsIGFyZzIsIGFyZzMpIHtcbiAgbGV0IGJ1bmRsZWRMYW5ndWFnZXM7XG4gIGxldCBidW5kbGVkVGhlbWVzO1xuICBsZXQgZW5naW5lO1xuICBpZiAoYXJnMikge1xuICAgIHdhcm5EZXByZWNhdGVkKFwiYGNyZWF0ZWRCdW5kbGVkSGlnaGxpZ2h0ZXJgIHNpZ25hdHVyZSB3aXRoIGBidW5kbGVkTGFuZ3VhZ2VzYCBhbmQgYGJ1bmRsZWRUaGVtZXNgIGlzIGRlcHJlY2F0ZWQuIFVzZSB0aGUgb3B0aW9ucyBvYmplY3Qgc2lnbmF0dXJlIGluc3RlYWQuXCIpO1xuICAgIGJ1bmRsZWRMYW5ndWFnZXMgPSBhcmcxO1xuICAgIGJ1bmRsZWRUaGVtZXMgPSBhcmcyO1xuICAgIGVuZ2luZSA9ICgpID0+IGNyZWF0ZU9uaWd1cnVtYUVuZ2luZShhcmczKTtcbiAgfSBlbHNlIHtcbiAgICBjb25zdCBvcHRpb25zID0gYXJnMTtcbiAgICBidW5kbGVkTGFuZ3VhZ2VzID0gb3B0aW9ucy5sYW5ncztcbiAgICBidW5kbGVkVGhlbWVzID0gb3B0aW9ucy50aGVtZXM7XG4gICAgZW5naW5lID0gb3B0aW9ucy5lbmdpbmU7XG4gIH1cbiAgYXN5bmMgZnVuY3Rpb24gY3JlYXRlSGlnaGxpZ2h0ZXIob3B0aW9ucykge1xuICAgIGZ1bmN0aW9uIHJlc29sdmVMYW5nKGxhbmcpIHtcbiAgICAgIGlmICh0eXBlb2YgbGFuZyA9PT0gXCJzdHJpbmdcIikge1xuICAgICAgICBpZiAoaXNTcGVjaWFsTGFuZyhsYW5nKSlcbiAgICAgICAgICByZXR1cm4gW107XG4gICAgICAgIGNvbnN0IGJ1bmRsZSA9IGJ1bmRsZWRMYW5ndWFnZXNbbGFuZ107XG4gICAgICAgIGlmICghYnVuZGxlKVxuICAgICAgICAgIHRocm93IG5ldyBTaGlraUVycm9yJDEoYExhbmd1YWdlIFxcYCR7bGFuZ31cXGAgaXMgbm90IGluY2x1ZGVkIGluIHRoaXMgYnVuZGxlLiBZb3UgbWF5IHdhbnQgdG8gbG9hZCBpdCBmcm9tIGV4dGVybmFsIHNvdXJjZS5gKTtcbiAgICAgICAgcmV0dXJuIGJ1bmRsZTtcbiAgICAgIH1cbiAgICAgIHJldHVybiBsYW5nO1xuICAgIH1cbiAgICBmdW5jdGlvbiByZXNvbHZlVGhlbWUodGhlbWUpIHtcbiAgICAgIGlmIChpc1NwZWNpYWxUaGVtZSh0aGVtZSkpXG4gICAgICAgIHJldHVybiBcIm5vbmVcIjtcbiAgICAgIGlmICh0eXBlb2YgdGhlbWUgPT09IFwic3RyaW5nXCIpIHtcbiAgICAgICAgY29uc3QgYnVuZGxlID0gYnVuZGxlZFRoZW1lc1t0aGVtZV07XG4gICAgICAgIGlmICghYnVuZGxlKVxuICAgICAgICAgIHRocm93IG5ldyBTaGlraUVycm9yJDEoYFRoZW1lIFxcYCR7dGhlbWV9XFxgIGlzIG5vdCBpbmNsdWRlZCBpbiB0aGlzIGJ1bmRsZS4gWW91IG1heSB3YW50IHRvIGxvYWQgaXQgZnJvbSBleHRlcm5hbCBzb3VyY2UuYCk7XG4gICAgICAgIHJldHVybiBidW5kbGU7XG4gICAgICB9XG4gICAgICByZXR1cm4gdGhlbWU7XG4gICAgfVxuICAgIGNvbnN0IF90aGVtZXMgPSAob3B0aW9ucy50aGVtZXMgPz8gW10pLm1hcCgoaSkgPT4gcmVzb2x2ZVRoZW1lKGkpKTtcbiAgICBjb25zdCBsYW5ncyA9IChvcHRpb25zLmxhbmdzID8/IFtdKS5tYXAoKGkpID0+IHJlc29sdmVMYW5nKGkpKTtcbiAgICBjb25zdCBjb3JlID0gYXdhaXQgY3JlYXRlSGlnaGxpZ2h0ZXJDb3JlKHtcbiAgICAgIGVuZ2luZTogb3B0aW9ucy5lbmdpbmUgPz8gZW5naW5lKCksXG4gICAgICAuLi5vcHRpb25zLFxuICAgICAgdGhlbWVzOiBfdGhlbWVzLFxuICAgICAgbGFuZ3NcbiAgICB9KTtcbiAgICByZXR1cm4ge1xuICAgICAgLi4uY29yZSxcbiAgICAgIGxvYWRMYW5ndWFnZSguLi5sYW5nczIpIHtcbiAgICAgICAgcmV0dXJuIGNvcmUubG9hZExhbmd1YWdlKC4uLmxhbmdzMi5tYXAocmVzb2x2ZUxhbmcpKTtcbiAgICAgIH0sXG4gICAgICBsb2FkVGhlbWUoLi4udGhlbWVzKSB7XG4gICAgICAgIHJldHVybiBjb3JlLmxvYWRUaGVtZSguLi50aGVtZXMubWFwKHJlc29sdmVUaGVtZSkpO1xuICAgICAgfVxuICAgIH07XG4gIH1cbiAgcmV0dXJuIGNyZWF0ZUhpZ2hsaWdodGVyO1xufVxuZnVuY3Rpb24gbWFrZVNpbmdsZXRvbkhpZ2hsaWdodGVyKGNyZWF0ZUhpZ2hsaWdodGVyKSB7XG4gIGxldCBfc2hpa2k7XG4gIGFzeW5jIGZ1bmN0aW9uIGdldFNpbmdsZXRvbkhpZ2hsaWdodGVyKG9wdGlvbnMgPSB7fSkge1xuICAgIGlmICghX3NoaWtpKSB7XG4gICAgICBfc2hpa2kgPSBjcmVhdGVIaWdobGlnaHRlcih7XG4gICAgICAgIC4uLm9wdGlvbnMsXG4gICAgICAgIHRoZW1lczogb3B0aW9ucy50aGVtZXMgfHwgW10sXG4gICAgICAgIGxhbmdzOiBvcHRpb25zLmxhbmdzIHx8IFtdXG4gICAgICB9KTtcbiAgICAgIHJldHVybiBfc2hpa2k7XG4gICAgfSBlbHNlIHtcbiAgICAgIGNvbnN0IHMgPSBhd2FpdCBfc2hpa2k7XG4gICAgICBhd2FpdCBQcm9taXNlLmFsbChbXG4gICAgICAgIHMubG9hZFRoZW1lKC4uLm9wdGlvbnMudGhlbWVzIHx8IFtdKSxcbiAgICAgICAgcy5sb2FkTGFuZ3VhZ2UoLi4ub3B0aW9ucy5sYW5ncyB8fCBbXSlcbiAgICAgIF0pO1xuICAgICAgcmV0dXJuIHM7XG4gICAgfVxuICB9XG4gIHJldHVybiBnZXRTaW5nbGV0b25IaWdobGlnaHRlcjtcbn1cbmZ1bmN0aW9uIGNyZWF0ZVNpbmdsZXRvblNob3J0aGFuZHMoY3JlYXRlSGlnaGxpZ2h0ZXIpIHtcbiAgY29uc3QgZ2V0U2luZ2xldG9uSGlnaGxpZ2h0ZXIgPSBtYWtlU2luZ2xldG9uSGlnaGxpZ2h0ZXIoY3JlYXRlSGlnaGxpZ2h0ZXIpO1xuICByZXR1cm4ge1xuICAgIGdldFNpbmdsZXRvbkhpZ2hsaWdodGVyKG9wdGlvbnMpIHtcbiAgICAgIHJldHVybiBnZXRTaW5nbGV0b25IaWdobGlnaHRlcihvcHRpb25zKTtcbiAgICB9LFxuICAgIGFzeW5jIGNvZGVUb0h0bWwoY29kZSwgb3B0aW9ucykge1xuICAgICAgY29uc3Qgc2hpa2kgPSBhd2FpdCBnZXRTaW5nbGV0b25IaWdobGlnaHRlcih7XG4gICAgICAgIGxhbmdzOiBbb3B0aW9ucy5sYW5nXSxcbiAgICAgICAgdGhlbWVzOiBcInRoZW1lXCIgaW4gb3B0aW9ucyA/IFtvcHRpb25zLnRoZW1lXSA6IE9iamVjdC52YWx1ZXMob3B0aW9ucy50aGVtZXMpXG4gICAgICB9KTtcbiAgICAgIHJldHVybiBzaGlraS5jb2RlVG9IdG1sKGNvZGUsIG9wdGlvbnMpO1xuICAgIH0sXG4gICAgYXN5bmMgY29kZVRvSGFzdChjb2RlLCBvcHRpb25zKSB7XG4gICAgICBjb25zdCBzaGlraSA9IGF3YWl0IGdldFNpbmdsZXRvbkhpZ2hsaWdodGVyKHtcbiAgICAgICAgbGFuZ3M6IFtvcHRpb25zLmxhbmddLFxuICAgICAgICB0aGVtZXM6IFwidGhlbWVcIiBpbiBvcHRpb25zID8gW29wdGlvbnMudGhlbWVdIDogT2JqZWN0LnZhbHVlcyhvcHRpb25zLnRoZW1lcylcbiAgICAgIH0pO1xuICAgICAgcmV0dXJuIHNoaWtpLmNvZGVUb0hhc3QoY29kZSwgb3B0aW9ucyk7XG4gICAgfSxcbiAgICBhc3luYyBjb2RlVG9Ub2tlbnMoY29kZSwgb3B0aW9ucykge1xuICAgICAgY29uc3Qgc2hpa2kgPSBhd2FpdCBnZXRTaW5nbGV0b25IaWdobGlnaHRlcih7XG4gICAgICAgIGxhbmdzOiBbb3B0aW9ucy5sYW5nXSxcbiAgICAgICAgdGhlbWVzOiBcInRoZW1lXCIgaW4gb3B0aW9ucyA/IFtvcHRpb25zLnRoZW1lXSA6IE9iamVjdC52YWx1ZXMob3B0aW9ucy50aGVtZXMpXG4gICAgICB9KTtcbiAgICAgIHJldHVybiBzaGlraS5jb2RlVG9Ub2tlbnMoY29kZSwgb3B0aW9ucyk7XG4gICAgfSxcbiAgICBhc3luYyBjb2RlVG9Ub2tlbnNCYXNlKGNvZGUsIG9wdGlvbnMpIHtcbiAgICAgIGNvbnN0IHNoaWtpID0gYXdhaXQgZ2V0U2luZ2xldG9uSGlnaGxpZ2h0ZXIoe1xuICAgICAgICBsYW5nczogW29wdGlvbnMubGFuZ10sXG4gICAgICAgIHRoZW1lczogW29wdGlvbnMudGhlbWVdXG4gICAgICB9KTtcbiAgICAgIHJldHVybiBzaGlraS5jb2RlVG9Ub2tlbnNCYXNlKGNvZGUsIG9wdGlvbnMpO1xuICAgIH0sXG4gICAgYXN5bmMgY29kZVRvVG9rZW5zV2l0aFRoZW1lcyhjb2RlLCBvcHRpb25zKSB7XG4gICAgICBjb25zdCBzaGlraSA9IGF3YWl0IGdldFNpbmdsZXRvbkhpZ2hsaWdodGVyKHtcbiAgICAgICAgbGFuZ3M6IFtvcHRpb25zLmxhbmddLFxuICAgICAgICB0aGVtZXM6IE9iamVjdC52YWx1ZXMob3B0aW9ucy50aGVtZXMpLmZpbHRlcihCb29sZWFuKVxuICAgICAgfSk7XG4gICAgICByZXR1cm4gc2hpa2kuY29kZVRvVG9rZW5zV2l0aFRoZW1lcyhjb2RlLCBvcHRpb25zKTtcbiAgICB9LFxuICAgIGFzeW5jIGdldExhc3RHcmFtbWFyU3RhdGUoY29kZSwgb3B0aW9ucykge1xuICAgICAgY29uc3Qgc2hpa2kgPSBhd2FpdCBnZXRTaW5nbGV0b25IaWdobGlnaHRlcih7XG4gICAgICAgIGxhbmdzOiBbb3B0aW9ucy5sYW5nXSxcbiAgICAgICAgdGhlbWVzOiBbb3B0aW9ucy50aGVtZV1cbiAgICAgIH0pO1xuICAgICAgcmV0dXJuIHNoaWtpLmdldExhc3RHcmFtbWFyU3RhdGUoY29kZSwgb3B0aW9ucyk7XG4gICAgfVxuICB9O1xufVxuXG5mdW5jdGlvbiBjcmVhdGVKYXZhU2NyaXB0UmVnZXhFbmdpbmUob3B0aW9ucykge1xuICB3YXJuRGVwcmVjYXRlZChcImltcG9ydCBgY3JlYXRlSmF2YVNjcmlwdFJlZ2V4RW5naW5lYCBmcm9tIGBAc2hpa2lqcy9lbmdpbmUtamF2YXNjcmlwdGAgb3IgYHNoaWtpL2VuZ2luZS9qYXZhc2NyaXB0YCBpbnN0ZWFkXCIpO1xuICByZXR1cm4gY3JlYXRlSmF2YVNjcmlwdFJlZ2V4RW5naW5lJDEob3B0aW9ucyk7XG59XG5mdW5jdGlvbiBkZWZhdWx0SmF2YVNjcmlwdFJlZ2V4Q29uc3RydWN0b3IocGF0dGVybikge1xuICB3YXJuRGVwcmVjYXRlZChcImltcG9ydCBgZGVmYXVsdEphdmFTY3JpcHRSZWdleENvbnN0cnVjdG9yYCBmcm9tIGBAc2hpa2lqcy9lbmdpbmUtamF2YXNjcmlwdGAgb3IgYHNoaWtpL2VuZ2luZS9qYXZhc2NyaXB0YCBpbnN0ZWFkXCIpO1xuICByZXR1cm4gZGVmYXVsdEphdmFTY3JpcHRSZWdleENvbnN0cnVjdG9yJDEocGF0dGVybik7XG59XG5cbmZ1bmN0aW9uIGNyZWF0ZUNzc1ZhcmlhYmxlc1RoZW1lKG9wdGlvbnMgPSB7fSkge1xuICBjb25zdCB7XG4gICAgbmFtZSA9IFwiY3NzLXZhcmlhYmxlc1wiLFxuICAgIHZhcmlhYmxlUHJlZml4ID0gXCItLXNoaWtpLVwiLFxuICAgIGZvbnRTdHlsZSA9IHRydWVcbiAgfSA9IG9wdGlvbnM7XG4gIGNvbnN0IHZhcmlhYmxlID0gKG5hbWUyKSA9PiB7XG4gICAgaWYgKG9wdGlvbnMudmFyaWFibGVEZWZhdWx0cz8uW25hbWUyXSlcbiAgICAgIHJldHVybiBgdmFyKCR7dmFyaWFibGVQcmVmaXh9JHtuYW1lMn0sICR7b3B0aW9ucy52YXJpYWJsZURlZmF1bHRzW25hbWUyXX0pYDtcbiAgICByZXR1cm4gYHZhcigke3ZhcmlhYmxlUHJlZml4fSR7bmFtZTJ9KWA7XG4gIH07XG4gIGNvbnN0IHRoZW1lID0ge1xuICAgIG5hbWUsXG4gICAgdHlwZTogXCJkYXJrXCIsXG4gICAgY29sb3JzOiB7XG4gICAgICBcImVkaXRvci5mb3JlZ3JvdW5kXCI6IHZhcmlhYmxlKFwiZm9yZWdyb3VuZFwiKSxcbiAgICAgIFwiZWRpdG9yLmJhY2tncm91bmRcIjogdmFyaWFibGUoXCJiYWNrZ3JvdW5kXCIpLFxuICAgICAgXCJ0ZXJtaW5hbC5hbnNpQmxhY2tcIjogdmFyaWFibGUoXCJhbnNpLWJsYWNrXCIpLFxuICAgICAgXCJ0ZXJtaW5hbC5hbnNpUmVkXCI6IHZhcmlhYmxlKFwiYW5zaS1yZWRcIiksXG4gICAgICBcInRlcm1pbmFsLmFuc2lHcmVlblwiOiB2YXJpYWJsZShcImFuc2ktZ3JlZW5cIiksXG4gICAgICBcInRlcm1pbmFsLmFuc2lZZWxsb3dcIjogdmFyaWFibGUoXCJhbnNpLXllbGxvd1wiKSxcbiAgICAgIFwidGVybWluYWwuYW5zaUJsdWVcIjogdmFyaWFibGUoXCJhbnNpLWJsdWVcIiksXG4gICAgICBcInRlcm1pbmFsLmFuc2lNYWdlbnRhXCI6IHZhcmlhYmxlKFwiYW5zaS1tYWdlbnRhXCIpLFxuICAgICAgXCJ0ZXJtaW5hbC5hbnNpQ3lhblwiOiB2YXJpYWJsZShcImFuc2ktY3lhblwiKSxcbiAgICAgIFwidGVybWluYWwuYW5zaVdoaXRlXCI6IHZhcmlhYmxlKFwiYW5zaS13aGl0ZVwiKSxcbiAgICAgIFwidGVybWluYWwuYW5zaUJyaWdodEJsYWNrXCI6IHZhcmlhYmxlKFwiYW5zaS1icmlnaHQtYmxhY2tcIiksXG4gICAgICBcInRlcm1pbmFsLmFuc2lCcmlnaHRSZWRcIjogdmFyaWFibGUoXCJhbnNpLWJyaWdodC1yZWRcIiksXG4gICAgICBcInRlcm1pbmFsLmFuc2lCcmlnaHRHcmVlblwiOiB2YXJpYWJsZShcImFuc2ktYnJpZ2h0LWdyZWVuXCIpLFxuICAgICAgXCJ0ZXJtaW5hbC5hbnNpQnJpZ2h0WWVsbG93XCI6IHZhcmlhYmxlKFwiYW5zaS1icmlnaHQteWVsbG93XCIpLFxuICAgICAgXCJ0ZXJtaW5hbC5hbnNpQnJpZ2h0Qmx1ZVwiOiB2YXJpYWJsZShcImFuc2ktYnJpZ2h0LWJsdWVcIiksXG4gICAgICBcInRlcm1pbmFsLmFuc2lCcmlnaHRNYWdlbnRhXCI6IHZhcmlhYmxlKFwiYW5zaS1icmlnaHQtbWFnZW50YVwiKSxcbiAgICAgIFwidGVybWluYWwuYW5zaUJyaWdodEN5YW5cIjogdmFyaWFibGUoXCJhbnNpLWJyaWdodC1jeWFuXCIpLFxuICAgICAgXCJ0ZXJtaW5hbC5hbnNpQnJpZ2h0V2hpdGVcIjogdmFyaWFibGUoXCJhbnNpLWJyaWdodC13aGl0ZVwiKVxuICAgIH0sXG4gICAgdG9rZW5Db2xvcnM6IFtcbiAgICAgIHtcbiAgICAgICAgc2NvcGU6IFtcbiAgICAgICAgICBcImtleXdvcmQub3BlcmF0b3IuYWNjZXNzb3JcIixcbiAgICAgICAgICBcIm1ldGEuZ3JvdXAuYnJhY2VzLnJvdW5kLmZ1bmN0aW9uLmFyZ3VtZW50c1wiLFxuICAgICAgICAgIFwibWV0YS50ZW1wbGF0ZS5leHByZXNzaW9uXCIsXG4gICAgICAgICAgXCJtYXJrdXAuZmVuY2VkX2NvZGUgbWV0YS5lbWJlZGRlZC5ibG9ja1wiXG4gICAgICAgIF0sXG4gICAgICAgIHNldHRpbmdzOiB7XG4gICAgICAgICAgZm9yZWdyb3VuZDogdmFyaWFibGUoXCJmb3JlZ3JvdW5kXCIpXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgICB7XG4gICAgICAgIHNjb3BlOiBcImVtcGhhc2lzXCIsXG4gICAgICAgIHNldHRpbmdzOiB7XG4gICAgICAgICAgZm9udFN0eWxlOiBcIml0YWxpY1wiXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgICB7XG4gICAgICAgIHNjb3BlOiBbXCJzdHJvbmdcIiwgXCJtYXJrdXAuaGVhZGluZy5tYXJrZG93blwiLCBcIm1hcmt1cC5ib2xkLm1hcmtkb3duXCJdLFxuICAgICAgICBzZXR0aW5nczoge1xuICAgICAgICAgIGZvbnRTdHlsZTogXCJib2xkXCJcbiAgICAgICAgfVxuICAgICAgfSxcbiAgICAgIHtcbiAgICAgICAgc2NvcGU6IFtcIm1hcmt1cC5pdGFsaWMubWFya2Rvd25cIl0sXG4gICAgICAgIHNldHRpbmdzOiB7XG4gICAgICAgICAgZm9udFN0eWxlOiBcIml0YWxpY1wiXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgICB7XG4gICAgICAgIHNjb3BlOiBcIm1ldGEubGluay5pbmxpbmUubWFya2Rvd25cIixcbiAgICAgICAgc2V0dGluZ3M6IHtcbiAgICAgICAgICBmb250U3R5bGU6IFwidW5kZXJsaW5lXCIsXG4gICAgICAgICAgZm9yZWdyb3VuZDogdmFyaWFibGUoXCJ0b2tlbi1saW5rXCIpXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgICB7XG4gICAgICAgIHNjb3BlOiBbXCJzdHJpbmdcIiwgXCJtYXJrdXAuZmVuY2VkX2NvZGVcIiwgXCJtYXJrdXAuaW5saW5lXCJdLFxuICAgICAgICBzZXR0aW5nczoge1xuICAgICAgICAgIGZvcmVncm91bmQ6IHZhcmlhYmxlKFwidG9rZW4tc3RyaW5nXCIpXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgICB7XG4gICAgICAgIHNjb3BlOiBbXCJjb21tZW50XCIsIFwic3RyaW5nLnF1b3RlZC5kb2NzdHJpbmcubXVsdGlcIl0sXG4gICAgICAgIHNldHRpbmdzOiB7XG4gICAgICAgICAgZm9yZWdyb3VuZDogdmFyaWFibGUoXCJ0b2tlbi1jb21tZW50XCIpXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgICB7XG4gICAgICAgIHNjb3BlOiBbXG4gICAgICAgICAgXCJjb25zdGFudC5udW1lcmljXCIsXG4gICAgICAgICAgXCJjb25zdGFudC5sYW5ndWFnZVwiLFxuICAgICAgICAgIFwiY29uc3RhbnQub3RoZXIucGxhY2Vob2xkZXJcIixcbiAgICAgICAgICBcImNvbnN0YW50LmNoYXJhY3Rlci5mb3JtYXQucGxhY2Vob2xkZXJcIixcbiAgICAgICAgICBcInZhcmlhYmxlLmxhbmd1YWdlLnRoaXNcIixcbiAgICAgICAgICBcInZhcmlhYmxlLm90aGVyLm9iamVjdFwiLFxuICAgICAgICAgIFwidmFyaWFibGUub3RoZXIuY2xhc3NcIixcbiAgICAgICAgICBcInZhcmlhYmxlLm90aGVyLmNvbnN0YW50XCIsXG4gICAgICAgICAgXCJtZXRhLnByb3BlcnR5LW5hbWVcIixcbiAgICAgICAgICBcIm1ldGEucHJvcGVydHktdmFsdWVcIixcbiAgICAgICAgICBcInN1cHBvcnRcIlxuICAgICAgICBdLFxuICAgICAgICBzZXR0aW5nczoge1xuICAgICAgICAgIGZvcmVncm91bmQ6IHZhcmlhYmxlKFwidG9rZW4tY29uc3RhbnRcIilcbiAgICAgICAgfVxuICAgICAgfSxcbiAgICAgIHtcbiAgICAgICAgc2NvcGU6IFtcbiAgICAgICAgICBcImtleXdvcmRcIixcbiAgICAgICAgICBcInN0b3JhZ2UubW9kaWZpZXJcIixcbiAgICAgICAgICBcInN0b3JhZ2UudHlwZVwiLFxuICAgICAgICAgIFwic3RvcmFnZS5jb250cm9sLmNsb2p1cmVcIixcbiAgICAgICAgICBcImVudGl0eS5uYW1lLmZ1bmN0aW9uLmNsb2p1cmVcIixcbiAgICAgICAgICBcImVudGl0eS5uYW1lLnRhZy55YW1sXCIsXG4gICAgICAgICAgXCJzdXBwb3J0LmZ1bmN0aW9uLm5vZGVcIixcbiAgICAgICAgICBcInN1cHBvcnQudHlwZS5wcm9wZXJ0eS1uYW1lLmpzb25cIixcbiAgICAgICAgICBcInB1bmN0dWF0aW9uLnNlcGFyYXRvci5rZXktdmFsdWVcIixcbiAgICAgICAgICBcInB1bmN0dWF0aW9uLmRlZmluaXRpb24udGVtcGxhdGUtZXhwcmVzc2lvblwiXG4gICAgICAgIF0sXG4gICAgICAgIHNldHRpbmdzOiB7XG4gICAgICAgICAgZm9yZWdyb3VuZDogdmFyaWFibGUoXCJ0b2tlbi1rZXl3b3JkXCIpXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgICB7XG4gICAgICAgIHNjb3BlOiBcInZhcmlhYmxlLnBhcmFtZXRlci5mdW5jdGlvblwiLFxuICAgICAgICBzZXR0aW5nczoge1xuICAgICAgICAgIGZvcmVncm91bmQ6IHZhcmlhYmxlKFwidG9rZW4tcGFyYW1ldGVyXCIpXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgICB7XG4gICAgICAgIHNjb3BlOiBbXG4gICAgICAgICAgXCJzdXBwb3J0LmZ1bmN0aW9uXCIsXG4gICAgICAgICAgXCJlbnRpdHkubmFtZS50eXBlXCIsXG4gICAgICAgICAgXCJlbnRpdHkub3RoZXIuaW5oZXJpdGVkLWNsYXNzXCIsXG4gICAgICAgICAgXCJtZXRhLmZ1bmN0aW9uLWNhbGxcIixcbiAgICAgICAgICBcIm1ldGEuaW5zdGFuY2UuY29uc3RydWN0b3JcIixcbiAgICAgICAgICBcImVudGl0eS5vdGhlci5hdHRyaWJ1dGUtbmFtZVwiLFxuICAgICAgICAgIFwiZW50aXR5Lm5hbWUuZnVuY3Rpb25cIixcbiAgICAgICAgICBcImNvbnN0YW50LmtleXdvcmQuY2xvanVyZVwiXG4gICAgICAgIF0sXG4gICAgICAgIHNldHRpbmdzOiB7XG4gICAgICAgICAgZm9yZWdyb3VuZDogdmFyaWFibGUoXCJ0b2tlbi1mdW5jdGlvblwiKVxuICAgICAgICB9XG4gICAgICB9LFxuICAgICAge1xuICAgICAgICBzY29wZTogW1xuICAgICAgICAgIFwiZW50aXR5Lm5hbWUudGFnXCIsXG4gICAgICAgICAgXCJzdHJpbmcucXVvdGVkXCIsXG4gICAgICAgICAgXCJzdHJpbmcucmVnZXhwXCIsXG4gICAgICAgICAgXCJzdHJpbmcuaW50ZXJwb2xhdGVkXCIsXG4gICAgICAgICAgXCJzdHJpbmcudGVtcGxhdGVcIixcbiAgICAgICAgICBcInN0cmluZy51bnF1b3RlZC5wbGFpbi5vdXQueWFtbFwiLFxuICAgICAgICAgIFwia2V5d29yZC5vdGhlci50ZW1wbGF0ZVwiXG4gICAgICAgIF0sXG4gICAgICAgIHNldHRpbmdzOiB7XG4gICAgICAgICAgZm9yZWdyb3VuZDogdmFyaWFibGUoXCJ0b2tlbi1zdHJpbmctZXhwcmVzc2lvblwiKVxuICAgICAgICB9XG4gICAgICB9LFxuICAgICAge1xuICAgICAgICBzY29wZTogW1xuICAgICAgICAgIFwicHVuY3R1YXRpb24uZGVmaW5pdGlvbi5hcmd1bWVudHNcIixcbiAgICAgICAgICBcInB1bmN0dWF0aW9uLmRlZmluaXRpb24uZGljdFwiLFxuICAgICAgICAgIFwicHVuY3R1YXRpb24uc2VwYXJhdG9yXCIsXG4gICAgICAgICAgXCJtZXRhLmZ1bmN0aW9uLWNhbGwuYXJndW1lbnRzXCJcbiAgICAgICAgXSxcbiAgICAgICAgc2V0dGluZ3M6IHtcbiAgICAgICAgICBmb3JlZ3JvdW5kOiB2YXJpYWJsZShcInRva2VuLXB1bmN0dWF0aW9uXCIpXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgICB7XG4gICAgICAgIC8vIFtDdXN0b21dIE1hcmtkb3duIGxpbmtzXG4gICAgICAgIHNjb3BlOiBbXG4gICAgICAgICAgXCJtYXJrdXAudW5kZXJsaW5lLmxpbmtcIixcbiAgICAgICAgICBcInB1bmN0dWF0aW9uLmRlZmluaXRpb24ubWV0YWRhdGEubWFya2Rvd25cIlxuICAgICAgICBdLFxuICAgICAgICBzZXR0aW5nczoge1xuICAgICAgICAgIGZvcmVncm91bmQ6IHZhcmlhYmxlKFwidG9rZW4tbGlua1wiKVxuICAgICAgICB9XG4gICAgICB9LFxuICAgICAge1xuICAgICAgICAvLyBbQ3VzdG9tXSBNYXJrZG93biBsaXN0XG4gICAgICAgIHNjb3BlOiBbXCJiZWdpbm5pbmcucHVuY3R1YXRpb24uZGVmaW5pdGlvbi5saXN0Lm1hcmtkb3duXCJdLFxuICAgICAgICBzZXR0aW5nczoge1xuICAgICAgICAgIGZvcmVncm91bmQ6IHZhcmlhYmxlKFwidG9rZW4tc3RyaW5nXCIpXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgICB7XG4gICAgICAgIC8vIFtDdXN0b21dIE1hcmtkb3duIHB1bmN0dWF0aW9uIGRlZmluaXRpb24gYnJhY2tldHNcbiAgICAgICAgc2NvcGU6IFtcbiAgICAgICAgICBcInB1bmN0dWF0aW9uLmRlZmluaXRpb24uc3RyaW5nLmJlZ2luLm1hcmtkb3duXCIsXG4gICAgICAgICAgXCJwdW5jdHVhdGlvbi5kZWZpbml0aW9uLnN0cmluZy5lbmQubWFya2Rvd25cIixcbiAgICAgICAgICBcInN0cmluZy5vdGhlci5saW5rLnRpdGxlLm1hcmtkb3duXCIsXG4gICAgICAgICAgXCJzdHJpbmcub3RoZXIubGluay5kZXNjcmlwdGlvbi5tYXJrZG93blwiXG4gICAgICAgIF0sXG4gICAgICAgIHNldHRpbmdzOiB7XG4gICAgICAgICAgZm9yZWdyb3VuZDogdmFyaWFibGUoXCJ0b2tlbi1rZXl3b3JkXCIpXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgICB7XG4gICAgICAgIC8vIFtDdXN0b21dIERpZmZcbiAgICAgICAgc2NvcGU6IFtcbiAgICAgICAgICBcIm1hcmt1cC5pbnNlcnRlZFwiLFxuICAgICAgICAgIFwibWV0YS5kaWZmLmhlYWRlci50by1maWxlXCIsXG4gICAgICAgICAgXCJwdW5jdHVhdGlvbi5kZWZpbml0aW9uLmluc2VydGVkXCJcbiAgICAgICAgXSxcbiAgICAgICAgc2V0dGluZ3M6IHtcbiAgICAgICAgICBmb3JlZ3JvdW5kOiB2YXJpYWJsZShcInRva2VuLWluc2VydGVkXCIpXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgICB7XG4gICAgICAgIHNjb3BlOiBbXG4gICAgICAgICAgXCJtYXJrdXAuZGVsZXRlZFwiLFxuICAgICAgICAgIFwibWV0YS5kaWZmLmhlYWRlci5mcm9tLWZpbGVcIixcbiAgICAgICAgICBcInB1bmN0dWF0aW9uLmRlZmluaXRpb24uZGVsZXRlZFwiXG4gICAgICAgIF0sXG4gICAgICAgIHNldHRpbmdzOiB7XG4gICAgICAgICAgZm9yZWdyb3VuZDogdmFyaWFibGUoXCJ0b2tlbi1kZWxldGVkXCIpXG4gICAgICAgIH1cbiAgICAgIH0sXG4gICAgICB7XG4gICAgICAgIHNjb3BlOiBbXG4gICAgICAgICAgXCJtYXJrdXAuY2hhbmdlZFwiLFxuICAgICAgICAgIFwicHVuY3R1YXRpb24uZGVmaW5pdGlvbi5jaGFuZ2VkXCJcbiAgICAgICAgXSxcbiAgICAgICAgc2V0dGluZ3M6IHtcbiAgICAgICAgICBmb3JlZ3JvdW5kOiB2YXJpYWJsZShcInRva2VuLWNoYW5nZWRcIilcbiAgICAgICAgfVxuICAgICAgfVxuICAgIF1cbiAgfTtcbiAgaWYgKCFmb250U3R5bGUpIHtcbiAgICB0aGVtZS50b2tlbkNvbG9ycyA9IHRoZW1lLnRva2VuQ29sb3JzPy5tYXAoKHRva2VuQ29sb3IpID0+IHtcbiAgICAgIGlmICh0b2tlbkNvbG9yLnNldHRpbmdzPy5mb250U3R5bGUpXG4gICAgICAgIGRlbGV0ZSB0b2tlbkNvbG9yLnNldHRpbmdzLmZvbnRTdHlsZTtcbiAgICAgIHJldHVybiB0b2tlbkNvbG9yO1xuICAgIH0pO1xuICB9XG4gIHJldHVybiB0aGVtZTtcbn1cblxuZXhwb3J0IHsgYWRkQ2xhc3NUb0hhc3QsIGFwcGx5Q29sb3JSZXBsYWNlbWVudHMsIGNvZGVUb0hhc3QsIGNvZGVUb0h0bWwsIGNvZGVUb1Rva2VucywgY29kZVRvVG9rZW5zQmFzZSwgY29kZVRvVG9rZW5zV2l0aFRoZW1lcywgY3JlYXRlQ3NzVmFyaWFibGVzVGhlbWUsIGNyZWF0ZUhpZ2hsaWdodGVyQ29yZSwgY3JlYXRlSGlnaGxpZ2h0ZXJDb3JlU3luYywgY3JlYXRlSmF2YVNjcmlwdFJlZ2V4RW5naW5lLCBjcmVhdGVPbmlndXJ1bWFFbmdpbmUsIGNyZWF0ZVBvc2l0aW9uQ29udmVydGVyLCBjcmVhdGVTaGlraUludGVybmFsLCBjcmVhdGVTaGlraUludGVybmFsU3luYywgY3JlYXRlU2luZ2xldG9uU2hvcnRoYW5kcywgY3JlYXRlV2FzbU9uaWdFbmdpbmUsIGNyZWF0ZWRCdW5kbGVkSGlnaGxpZ2h0ZXIsIGRlZmF1bHRKYXZhU2NyaXB0UmVnZXhDb25zdHJ1Y3RvciwgZ2V0SGlnaGxpZ2h0ZXJDb3JlLCBnZXRTaGlraUludGVybmFsLCBnZXRTaW5nbGV0b25IaWdobGlnaHRlckNvcmUsIGdldFRva2VuU3R5bGVPYmplY3QsIGlzTm9uZVRoZW1lLCBpc1BsYWluTGFuZywgaXNTcGVjaWFsTGFuZywgaXNTcGVjaWFsVGhlbWUsIGxvYWRXYXNtLCBtYWtlU2luZ2xldG9uSGlnaGxpZ2h0ZXIsIG1ha2VTaW5nbGV0b25IaWdobGlnaHRlckNvcmUsIG5vcm1hbGl6ZUdldHRlciwgbm9ybWFsaXplVGhlbWUsIHJlc29sdmVDb2xvclJlcGxhY2VtZW50cywgc3BsaXRMaW5lcywgc3BsaXRUb2tlbiwgc3BsaXRUb2tlbnMsIHN0cmluZ2lmeVRva2VuU3R5bGUsIHRvQXJyYXksIHRva2VuaXplQW5zaVdpdGhUaGVtZSwgdG9rZW5pemVXaXRoVGhlbWUsIHRva2Vuc1RvSGFzdCwgdHJhbnNmb3JtZXJEZWNvcmF0aW9ucywgd2FybkRlcHJlY2F0ZWQgfTtcbiJdLCJuYW1lcyI6WyJTaGlraUVycm9yIiwiUmVnaXN0cnkiLCJTaGlraUVycm9yJDEiLCJjcmVhdGVPbmlndXJ1bWFFbmdpbmUkMSJdLCJtYXBwaW5ncyI6Ijs7bUJBQUEsTUFBTSxVQUFVLFNBQVMsS0FBSyxDQUFDO0FBQy9CLEVBQUUsV0FBVyxDQUFDLE9BQU8sRUFBRTtBQUN2QixJQUFJLEtBQUssQ0FBQyxPQUFPLENBQUM7QUFDbEIsSUFBSSxJQUFJLENBQUMsSUFBSSxHQUFHLFlBQVk7QUFDNUI7QUFDQTs7bUJDTEEsTUFBTSxVQUFVLFNBQVMsS0FBSyxDQUFDO0FBQy9CLEVBQUUsV0FBVyxDQUFDLE9BQU8sRUFBRTtBQUN2QixJQUFJLEtBQUssQ0FBQyxPQUFPLENBQUM7QUFDbEIsSUFBSSxJQUFJLENBQUMsSUFBSSxHQUFHLFlBQVk7QUFDNUI7QUFDQTs7QUFFQSxTQUFTLFVBQVUsR0FBRztBQUN0QixFQUFFLE9BQU8sVUFBVTtBQUNuQjtBQUNBLFNBQVMsbUJBQW1CLEdBQUc7QUFDL0IsRUFBRSxPQUFPLE9BQU8sV0FBVyxLQUFLLFdBQVcsR0FB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RTtBQUMzQyxFQUFFLE1BQU0sU0FBUyxHQUFHLE1BQU0sQ0FBQyxHQUFHLENBQUMsTUFBTSxFQUFFLENBQUM7QUFDeEMsRUFBRSxNQUFNLEtBQUssR0FBRyxNQUFNLENBQUMsTUFBTTtBQUM3QixFQUFFLEtBQUssSUFBSSxDQUFDLEdBQUcsQ0FBQyxFQUFFLENBQUMsR0FBRyxNQUFNLENBQUMsQ0FBQyxDQUFDLENBQUMsTUFBTSxFQUFFLENBQUMsRUFBRSxFQUFFO0FBQzdDLElBQUksTUFBTSxLQUFLLEdBQUcsTUFBTSxDQUFDLEdBQUcsQ0FBQyxDQUFDLENBQUMsS0FBSyxDQUFDLENBQUMsQ0FBQyxDQUFDLENBQUM7QUFDekMsSUFBSSxNQUFNLFFBQVEsR0FBRyxTQUFTLENBQUMsR0FBRyxDQUFDLE1BQU0sRUFBRSxDQUFDO0FBQzVDLElBQUksU0FBUyxDQUFDLE9BQU8sQ0FBQyxDQUFDLENBQUMsRUFBRSxFQUFFLEtBQUssQ0FBQyxDQUFDLElBQUksQ0FBQyxRQUFRLENBQUMsRUFBRSxDQUFDLENBQUMsQ0FBQztBQUN0RCxJQUFJLE1BQU0sT0FBTyxHQUFHLEtBQUssQ0FBQyxHQUFHLENBQUMsTUFBTSxDQUFDLENBQUM7QUFDdEMsSUFBSSxNQUFNLE9BQU8sR0FBRyxLQUFLLENBQUMsR0FBRyxDQUFDLENBQUMsQ0FBQyxLQUFLLENBQUMsQ0FBQyxDQUFDLENBQUMsQ0FBQztBQUMxQyxJQUFJLE9BQU8sT0FBTyxDQUFDLEtBQUssQ0FBQyxDQUFDLENBQUMsS0FBSyxDQUFDLENBQUMsRUFBRTtBQUNwQyxNQUFNLE1BQU0sU0FBUyxHQUFHLElBQUksQ0FBQyxHQUFHLENBQUMsR0FBRyxPQUFPLENBQUMsR0FBRyxDQUFDLENBQUMsQ0FBQyxLQUFLLENBQUMsQ0FBQyxPQUFPLENBQUMsTUFBTSxDQUFDLENBQUM7QUFDekUsTUFBTSxLQUFLLElBQUksQ0FBQyxHQUFHLENBQUMsRUFBRSxDQUFDLEdBQUcsS0FBSyxFQUFFLENBQUMsRUFBRSxFQUFFO0FBQ3RDLFFBQVEsTUFBTSxLQUFLLEdBQUcsT0FBTyxD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
