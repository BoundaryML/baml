/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
/*eslint-disable block-scoped-var, id-length, no-control-regex, no-magic-numbers, no-prototype-builtins, no-redeclare, no-shadow, no-var, sort-vars*/
"use strict";

var $protobuf = require("protobufjs/minimal");

// Common aliases
var $Reader = $protobuf.Reader, $Writer = $protobuf.Writer, $util = $protobuf.util;

// Exported root namespace
var $root = $protobuf.roots["default"] || ($protobuf.roots["default"] = {});

$root.baml_core = (function() {

    /**
     * Namespace baml_core.
     * @exports baml_core
     * @namespace
     */
    var baml_core = {};

    baml_core.cffi = (function() {

        /**
         * Namespace cffi.
         * @memberof baml_core
         * @namespace
         */
        var cffi = {};

        cffi.v1 = (function() {

            /**
             * Namespace v1.
             * @memberof baml_core.cffi
             * @namespace
             */
            var v1 = {};

            /**
             * BamlHandleType enum.
             * @name baml_core.cffi.v1.BamlHandleType
             * @enum {number}
             * @property {number} HANDLE_UNSPECIFIED=0 HANDLE_UNSPECIFIED value
             * @property {number} UNTAGGED_RUST_DATA=1 UNTAGGED_RUST_DATA value
             * @property {number} UNTAGGED_BEX_HEAP=2 UNTAGGED_BEX_HEAP value
             * @property {number} FUNCTION_REF=5 FUNCTION_REF value
             * @property {number} ADT_MEDIA_IMAGE=6 ADT_MEDIA_IMAGE value
             * @property {number} ADT_MEDIA_AUDIO=7 ADT_MEDIA_AUDIO value
             * @property {number} ADT_MEDIA_VIDEO=8 ADT_MEDIA_VIDEO value
             * @property {number} ADT_MEDIA_PDF=9 ADT_MEDIA_PDF value
             * @property {number} ADT_MEDIA_GENERIC=10 ADT_MEDIA_GENERIC value
             * @property {number} ADT_PROMPT_AST=11 ADT_PROMPT_AST value
             * @property {number} ADT_COLLECTOR=12 ADT_COLLECTOR value
             * @property {number} ADT_TYPE=13 ADT_TYPE value
             * @property {number} ADT_TAGGED_HEAP_HANDLE=14 ADT_TAGGED_HEAP_HANDLE value
             */
            v1.BamlHandleType = (function() {
                var valuesById = {}, values = Object.create(valuesById);
                values[valuesById[0] = "HANDLE_UNSPECIFIED"] = 0;
                values[valuesById[1] = "UNTAGGED_RUST_DATA"] = 1;
                values[valuesById[2] = "UNTAGGED_BEX_HEAP"] = 2;
                values[valuesById[5] = "FUNCTION_REF"] = 5;
                values[valuesById[6] = "ADT_MEDIA_IMAGE"] = 6;
                values[valuesById[7] = "ADT_MEDIA_AUDIO"] = 7;
                values[valuesById[8] = "ADT_MEDIA_VIDEO"] = 8;
                values[valuesById[9] = "ADT_MEDIA_PDF"] = 9;
                values[valuesById[10] = "ADT_MEDIA_GENERIC"] = 10;
                values[valuesById[11] = "ADT_PROMPT_AST"] = 11;
                values[valuesById[12] = "ADT_COLLECTOR"] = 12;
                values[valuesById[13] = "ADT_TYPE"] = 13;
                values[valuesById[14] = "ADT_TAGGED_HEAP_HANDLE"] = 14;
                return values;
            })();

            v1.BamlHandle = (function() {

                /**
                 * Properties of a BamlHandle.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlHandle
                 * @property {number|Long|null} [key] BamlHandle key
                 * @property {baml_core.cffi.v1.BamlHandleType|null} [handleType] BamlHandle handleType
                 */

                /**
                 * Constructs a new BamlHandle.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlHandle.
                 * @implements IBamlHandle
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlHandle=} [properties] Properties to set
                 */
                function BamlHandle(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlHandle key.
                 * @member {number|Long} key
                 * @memberof baml_core.cffi.v1.BamlHandle
                 * @instance
                 */
                BamlHandle.prototype.key = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * BamlHandle handleType.
                 * @member {baml_core.cffi.v1.BamlHandleType} handleType
                 * @memberof baml_core.cffi.v1.BamlHandle
                 * @instance
                 */
                BamlHandle.prototype.handleType = 0;

                /**
                 * Creates a new BamlHandle instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlHandle
                 * @static
                 * @param {baml_core.cffi.v1.IBamlHandle=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlHandle} BamlHandle instance
                 */
                BamlHandle.create = function create(properties) {
                    return new BamlHandle(properties);
                };

                /**
                 * Encodes the specified BamlHandle message. Does not implicitly {@link baml_core.cffi.v1.BamlHandle.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlHandle
                 * @static
                 * @param {baml_core.cffi.v1.IBamlHandle} message BamlHandle message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlHandle.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.key != null && Object.hasOwnProperty.call(message, "key"))
                        writer.uint32(/* id 1, wireType 0 =*/8).uint64(message.key);
                    if (message.handleType != null && Object.hasOwnProperty.call(message, "handleType"))
                        writer.uint32(/* id 2, wireType 0 =*/16).int32(message.handleType);
                    return writer;
                };

                /**
                 * Encodes the specified BamlHandle message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlHandle.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlHandle
                 * @static
                 * @param {baml_core.cffi.v1.IBamlHandle} message BamlHandle message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlHandle.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlHandle message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlHandle
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlHandle} BamlHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlHandle.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlHandle();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.key = reader.uint64();
                                break;
                            }
                        case 2: {
                                message.handleType = reader.int32();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlHandle message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlHandle
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlHandle} BamlHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlHandle.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlHandle message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlHandle
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlHandle.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.key != null && message.hasOwnProperty("key"))
                        if (!$util.isInteger(message.key) && !(message.key && $util.isInteger(message.key.low) && $util.isInteger(message.key.high)))
                            return "key: integer|Long expected";
                    if (message.handleType != null && message.hasOwnProperty("handleType"))
                        switch (message.handleType) {
                        default:
                            return "handleType: enum value expected";
                        case 0:
                        case 1:
                        case 2:
                        case 5:
                        case 6:
                        case 7:
                        case 8:
                        case 9:
                        case 10:
                        case 11:
                        case 12:
                        case 13:
                        case 14:
                            break;
                        }
                    return null;
                };

                /**
                 * Creates a BamlHandle message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlHandle
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlHandle} BamlHandle
                 */
                BamlHandle.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlHandle)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlHandle();
                    if (object.key != null)
                        if ($util.Long)
                            (message.key = $util.Long.fromValue(object.key)).unsigned = true;
                        else if (typeof object.key === "string")
                            message.key = parseInt(object.key, 10);
                        else if (typeof object.key === "number")
                            message.key = object.key;
                        else if (typeof object.key === "object")
                            message.key = new $util.LongBits(object.key.low >>> 0, object.key.high >>> 0).toNumber(true);
                    switch (object.handleType) {
                    default:
                        if (typeof object.handleType === "number") {
                            message.handleType = object.handleType;
                            break;
                        }
                        break;
                    case "HANDLE_UNSPECIFIED":
                    case 0:
                        message.handleType = 0;
                        break;
                    case "UNTAGGED_RUST_DATA":
                    case 1:
                        message.handleType = 1;
                        break;
                    case "UNTAGGED_BEX_HEAP":
                    case 2:
                        message.handleType = 2;
                        break;
                    case "FUNCTION_REF":
                    case 5:
                        message.handleType = 5;
                        break;
                    case "ADT_MEDIA_IMAGE":
                    case 6:
                        message.handleType = 6;
                        break;
                    case "ADT_MEDIA_AUDIO":
                    case 7:
                        message.handleType = 7;
                        break;
                    case "ADT_MEDIA_VIDEO":
                    case 8:
                        message.handleType = 8;
                        break;
                    case "ADT_MEDIA_PDF":
                    case 9:
                        message.handleType = 9;
                        break;
                    case "ADT_MEDIA_GENERIC":
                    case 10:
                        message.handleType = 10;
                        break;
                    case "ADT_PROMPT_AST":
                    case 11:
                        message.handleType = 11;
                        break;
                    case "ADT_COLLECTOR":
                    case 12:
                        message.handleType = 12;
                        break;
                    case "ADT_TYPE":
                    case 13:
                        message.handleType = 13;
                        break;
                    case "ADT_TAGGED_HEAP_HANDLE":
                    case 14:
                        message.handleType = 14;
                        break;
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlHandle message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlHandle
                 * @static
                 * @param {baml_core.cffi.v1.BamlHandle} message BamlHandle
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlHandle.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults) {
                        if ($util.Long) {
                            var long = new $util.Long(0, 0, true);
                            object.key = options.longs === String ? long.toString() : options.longs === Number ? long.toNumber() : long;
                        } else
                            object.key = options.longs === String ? "0" : 0;
                        object.handleType = options.enums === String ? "HANDLE_UNSPECIFIED" : 0;
                    }
                    if (message.key != null && message.hasOwnProperty("key"))
                        if (typeof message.key === "number")
                            object.key = options.longs === String ? String(message.key) : message.key;
                        else
                            object.key = options.longs === String ? $util.Long.prototype.toString.call(message.key) : options.longs === Number ? new $util.LongBits(message.key.low >>> 0, message.key.high >>> 0).toNumber(true) : message.key;
                    if (message.handleType != null && message.hasOwnProperty("handleType"))
                        object.handleType = options.enums === String ? $root.baml_core.cffi.v1.BamlHandleType[message.handleType] === undefined ? message.handleType : $root.baml_core.cffi.v1.BamlHandleType[message.handleType] : message.handleType;
                    return object;
                };

                /**
                 * Converts this BamlHandle to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlHandle
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlHandle.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlHandle
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlHandle
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlHandle.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlHandle";
                };

                return BamlHandle;
            })();

            v1.InboundValue = (function() {

                /**
                 * Properties of an InboundValue.
                 * @memberof baml_core.cffi.v1
                 * @interface IInboundValue
                 * @property {string|null} [stringValue] InboundValue stringValue
                 * @property {number|Long|null} [intValue] InboundValue intValue
                 * @property {number|null} [floatValue] InboundValue floatValue
                 * @property {boolean|null} [boolValue] InboundValue boolValue
                 * @property {baml_core.cffi.v1.IInboundListValue|null} [listValue] InboundValue listValue
                 * @property {baml_core.cffi.v1.IInboundMapValue|null} [mapValue] InboundValue mapValue
                 * @property {baml_core.cffi.v1.IInboundClassValue|null} [classValue] InboundValue classValue
                 * @property {baml_core.cffi.v1.IInboundEnumValue|null} [enumValue] InboundValue enumValue
                 * @property {baml_core.cffi.v1.IBamlHandle|null} [handle] InboundValue handle
                 * @property {Uint8Array|null} [uint8arrayValue] InboundValue uint8arrayValue
                 */

                /**
                 * Constructs a new InboundValue.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents an InboundValue.
                 * @implements IInboundValue
                 * @constructor
                 * @param {baml_core.cffi.v1.IInboundValue=} [properties] Properties to set
                 */
                function InboundValue(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * InboundValue stringValue.
                 * @member {string|null|undefined} stringValue
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.stringValue = null;

                /**
                 * InboundValue intValue.
                 * @member {number|Long|null|undefined} intValue
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.intValue = null;

                /**
                 * InboundValue floatValue.
                 * @member {number|null|undefined} floatValue
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.floatValue = null;

                /**
                 * InboundValue boolValue.
                 * @member {boolean|null|undefined} boolValue
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.boolValue = null;

                /**
                 * InboundValue listValue.
                 * @member {baml_core.cffi.v1.IInboundListValue|null|undefined} listValue
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.listValue = null;

                /**
                 * InboundValue mapValue.
                 * @member {baml_core.cffi.v1.IInboundMapValue|null|undefined} mapValue
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.mapValue = null;

                /**
                 * InboundValue classValue.
                 * @member {baml_core.cffi.v1.IInboundClassValue|null|undefined} classValue
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.classValue = null;

                /**
                 * InboundValue enumValue.
                 * @member {baml_core.cffi.v1.IInboundEnumValue|null|undefined} enumValue
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.enumValue = null;

                /**
                 * InboundValue handle.
                 * @member {baml_core.cffi.v1.IBamlHandle|null|undefined} handle
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.handle = null;

                /**
                 * InboundValue uint8arrayValue.
                 * @member {Uint8Array|null|undefined} uint8arrayValue
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.uint8arrayValue = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * InboundValue value.
                 * @member {"stringValue"|"intValue"|"floatValue"|"boolValue"|"listValue"|"mapValue"|"classValue"|"enumValue"|"handle"|"uint8arrayValue"|undefined} value
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @instance
                 */
                Object.defineProperty(InboundValue.prototype, "value", {
                    get: $util.oneOfGetter($oneOfFields = ["stringValue", "intValue", "floatValue", "boolValue", "listValue", "mapValue", "classValue", "enumValue", "handle", "uint8arrayValue"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new InboundValue instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundValue=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.InboundValue} InboundValue instance
                 */
                InboundValue.create = function create(properties) {
                    return new InboundValue(properties);
                };

                /**
                 * Encodes the specified InboundValue message. Does not implicitly {@link baml_core.cffi.v1.InboundValue.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundValue} message InboundValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundValue.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.stringValue != null && Object.hasOwnProperty.call(message, "stringValue"))
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.stringValue);
                    if (message.intValue != null && Object.hasOwnProperty.call(message, "intValue"))
                        writer.uint32(/* id 3, wireType 0 =*/24).int64(message.intValue);
                    if (message.floatValue != null && Object.hasOwnProperty.call(message, "floatValue"))
                        writer.uint32(/* id 4, wireType 1 =*/33).double(message.floatValue);
                    if (message.boolValue != null && Object.hasOwnProperty.call(message, "boolValue"))
                        writer.uint32(/* id 5, wireType 0 =*/40).bool(message.boolValue);
                    if (message.listValue != null && Object.hasOwnProperty.call(message, "listValue"))
                        $root.baml_core.cffi.v1.InboundListValue.encode(message.listValue, writer.uint32(/* id 6, wireType 2 =*/50).fork()).ldelim();
                    if (message.mapValue != null && Object.hasOwnProperty.call(message, "mapValue"))
                        $root.baml_core.cffi.v1.InboundMapValue.encode(message.mapValue, writer.uint32(/* id 7, wireType 2 =*/58).fork()).ldelim();
                    if (message.classValue != null && Object.hasOwnProperty.call(message, "classValue"))
                        $root.baml_core.cffi.v1.InboundClassValue.encode(message.classValue, writer.uint32(/* id 8, wireType 2 =*/66).fork()).ldelim();
                    if (message.enumValue != null && Object.hasOwnProperty.call(message, "enumValue"))
                        $root.baml_core.cffi.v1.InboundEnumValue.encode(message.enumValue, writer.uint32(/* id 9, wireType 2 =*/74).fork()).ldelim();
                    if (message.handle != null && Object.hasOwnProperty.call(message, "handle"))
                        $root.baml_core.cffi.v1.BamlHandle.encode(message.handle, writer.uint32(/* id 10, wireType 2 =*/82).fork()).ldelim();
                    if (message.uint8arrayValue != null && Object.hasOwnProperty.call(message, "uint8arrayValue"))
                        writer.uint32(/* id 11, wireType 2 =*/90).bytes(message.uint8arrayValue);
                    return writer;
                };

                /**
                 * Encodes the specified InboundValue message, length delimited. Does not implicitly {@link baml_core.cffi.v1.InboundValue.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundValue} message InboundValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundValue.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an InboundValue message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.InboundValue} InboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundValue.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.InboundValue();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 2: {
                                message.stringValue = reader.string();
                                break;
                            }
                        case 3: {
                                message.intValue = reader.int64();
                                break;
                            }
                        case 4: {
                                message.floatValue = reader.double();
                                break;
                            }
                        case 5: {
                                message.boolValue = reader.bool();
                                break;
                            }
                        case 6: {
                                message.listValue = $root.baml_core.cffi.v1.InboundListValue.decode(reader, reader.uint32());
                                break;
                            }
                        case 7: {
                                message.mapValue = $root.baml_core.cffi.v1.InboundMapValue.decode(reader, reader.uint32());
                                break;
                            }
                        case 8: {
                                message.classValue = $root.baml_core.cffi.v1.InboundClassValue.decode(reader, reader.uint32());
                                break;
                            }
                        case 9: {
                                message.enumValue = $root.baml_core.cffi.v1.InboundEnumValue.decode(reader, reader.uint32());
                                break;
                            }
                        case 10: {
                                message.handle = $root.baml_core.cffi.v1.BamlHandle.decode(reader, reader.uint32());
                                break;
                            }
                        case 11: {
                                message.uint8arrayValue = reader.bytes();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes an InboundValue message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.InboundValue} InboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundValue.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies an InboundValue message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                InboundValue.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    var properties = {};
                    if (message.stringValue != null && message.hasOwnProperty("stringValue")) {
                        properties.value = 1;
                        if (!$util.isString(message.stringValue))
                            return "stringValue: string expected";
                    }
                    if (message.intValue != null && message.hasOwnProperty("intValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        if (!$util.isInteger(message.intValue) && !(message.intValue && $util.isInteger(message.intValue.low) && $util.isInteger(message.intValue.high)))
                            return "intValue: integer|Long expected";
                    }
                    if (message.floatValue != null && message.hasOwnProperty("floatValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        if (typeof message.floatValue !== "number")
                            return "floatValue: number expected";
                    }
                    if (message.boolValue != null && message.hasOwnProperty("boolValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        if (typeof message.boolValue !== "boolean")
                            return "boolValue: boolean expected";
                    }
                    if (message.listValue != null && message.hasOwnProperty("listValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.InboundListValue.verify(message.listValue);
                            if (error)
                                return "listValue." + error;
                        }
                    }
                    if (message.mapValue != null && message.hasOwnProperty("mapValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.InboundMapValue.verify(message.mapValue);
                            if (error)
                                return "mapValue." + error;
                        }
                    }
                    if (message.classValue != null && message.hasOwnProperty("classValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.InboundClassValue.verify(message.classValue);
                            if (error)
                                return "classValue." + error;
                        }
                    }
                    if (message.enumValue != null && message.hasOwnProperty("enumValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.InboundEnumValue.verify(message.enumValue);
                            if (error)
                                return "enumValue." + error;
                        }
                    }
                    if (message.handle != null && message.hasOwnProperty("handle")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlHandle.verify(message.handle);
                            if (error)
                                return "handle." + error;
                        }
                    }
                    if (message.uint8arrayValue != null && message.hasOwnProperty("uint8arrayValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        if (!(message.uint8arrayValue && typeof message.uint8arrayValue.length === "number" || $util.isString(message.uint8arrayValue)))
                            return "uint8arrayValue: buffer expected";
                    }
                    return null;
                };

                /**
                 * Creates an InboundValue message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.InboundValue} InboundValue
                 */
                InboundValue.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.InboundValue)
                        return object;
                    var message = new $root.baml_core.cffi.v1.InboundValue();
                    if (object.stringValue != null)
                        message.stringValue = String(object.stringValue);
                    if (object.intValue != null)
                        if ($util.Long)
                            (message.intValue = $util.Long.fromValue(object.intValue)).unsigned = false;
                        else if (typeof object.intValue === "string")
                            message.intValue = parseInt(object.intValue, 10);
                        else if (typeof object.intValue === "number")
                            message.intValue = object.intValue;
                        else if (typeof object.intValue === "object")
                            message.intValue = new $util.LongBits(object.intValue.low >>> 0, object.intValue.high >>> 0).toNumber();
                    if (object.floatValue != null)
                        message.floatValue = Number(object.floatValue);
                    if (object.boolValue != null)
                        message.boolValue = Boolean(object.boolValue);
                    if (object.listValue != null) {
                        if (typeof object.listValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.InboundValue.listValue: object expected");
                        message.listValue = $root.baml_core.cffi.v1.InboundListValue.fromObject(object.listValue);
                    }
                    if (object.mapValue != null) {
                        if (typeof object.mapValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.InboundValue.mapValue: object expected");
                        message.mapValue = $root.baml_core.cffi.v1.InboundMapValue.fromObject(object.mapValue);
                    }
                    if (object.classValue != null) {
                        if (typeof object.classValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.InboundValue.classValue: object expected");
                        message.classValue = $root.baml_core.cffi.v1.InboundClassValue.fromObject(object.classValue);
                    }
                    if (object.enumValue != null) {
                        if (typeof object.enumValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.InboundValue.enumValue: object expected");
                        message.enumValue = $root.baml_core.cffi.v1.InboundEnumValue.fromObject(object.enumValue);
                    }
                    if (object.handle != null) {
                        if (typeof object.handle !== "object")
                            throw TypeError(".baml_core.cffi.v1.InboundValue.handle: object expected");
                        message.handle = $root.baml_core.cffi.v1.BamlHandle.fromObject(object.handle);
                    }
                    if (object.uint8arrayValue != null)
                        if (typeof object.uint8arrayValue === "string")
                            $util.base64.decode(object.uint8arrayValue, message.uint8arrayValue = $util.newBuffer($util.base64.length(object.uint8arrayValue)), 0);
                        else if (object.uint8arrayValue.length >= 0)
                            message.uint8arrayValue = object.uint8arrayValue;
                    return message;
                };

                /**
                 * Creates a plain object from an InboundValue message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @static
                 * @param {baml_core.cffi.v1.InboundValue} message InboundValue
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                InboundValue.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (message.stringValue != null && message.hasOwnProperty("stringValue")) {
                        object.stringValue = message.stringValue;
                        if (options.oneofs)
                            object.value = "stringValue";
                    }
                    if (message.intValue != null && message.hasOwnProperty("intValue")) {
                        if (typeof message.intValue === "number")
                            object.intValue = options.longs === String ? String(message.intValue) : message.intValue;
                        else
                            object.intValue = options.longs === String ? $util.Long.prototype.toString.call(message.intValue) : options.longs === Number ? new $util.LongBits(message.intValue.low >>> 0, message.intValue.high >>> 0).toNumber() : message.intValue;
                        if (options.oneofs)
                            object.value = "intValue";
                    }
                    if (message.floatValue != null && message.hasOwnProperty("floatValue")) {
                        object.floatValue = options.json && !isFinite(message.floatValue) ? String(message.floatValue) : message.floatValue;
                        if (options.oneofs)
                            object.value = "floatValue";
                    }
                    if (message.boolValue != null && message.hasOwnProperty("boolValue")) {
                        object.boolValue = message.boolValue;
                        if (options.oneofs)
                            object.value = "boolValue";
                    }
                    if (message.listValue != null && message.hasOwnProperty("listValue")) {
                        object.listValue = $root.baml_core.cffi.v1.InboundListValue.toObject(message.listValue, options);
                        if (options.oneofs)
                            object.value = "listValue";
                    }
                    if (message.mapValue != null && message.hasOwnProperty("mapValue")) {
                        object.mapValue = $root.baml_core.cffi.v1.InboundMapValue.toObject(message.mapValue, options);
                        if (options.oneofs)
                            object.value = "mapValue";
                    }
                    if (message.classValue != null && message.hasOwnProperty("classValue")) {
                        object.classValue = $root.baml_core.cffi.v1.InboundClassValue.toObject(message.classValue, options);
                        if (options.oneofs)
                            object.value = "classValue";
                    }
                    if (message.enumValue != null && message.hasOwnProperty("enumValue")) {
                        object.enumValue = $root.baml_core.cffi.v1.InboundEnumValue.toObject(message.enumValue, options);
                        if (options.oneofs)
                            object.value = "enumValue";
                    }
                    if (message.handle != null && message.hasOwnProperty("handle")) {
                        object.handle = $root.baml_core.cffi.v1.BamlHandle.toObject(message.handle, options);
                        if (options.oneofs)
                            object.value = "handle";
                    }
                    if (message.uint8arrayValue != null && message.hasOwnProperty("uint8arrayValue")) {
                        object.uint8arrayValue = options.bytes === String ? $util.base64.encode(message.uint8arrayValue, 0, message.uint8arrayValue.length) : options.bytes === Array ? Array.prototype.slice.call(message.uint8arrayValue) : message.uint8arrayValue;
                        if (options.oneofs)
                            object.value = "uint8arrayValue";
                    }
                    return object;
                };

                /**
                 * Converts this InboundValue to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                InboundValue.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for InboundValue
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.InboundValue
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                InboundValue.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.InboundValue";
                };

                return InboundValue;
            })();

            v1.InboundListValue = (function() {

                /**
                 * Properties of an InboundListValue.
                 * @memberof baml_core.cffi.v1
                 * @interface IInboundListValue
                 * @property {Array.<baml_core.cffi.v1.IInboundValue>|null} [values] InboundListValue values
                 */

                /**
                 * Constructs a new InboundListValue.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents an InboundListValue.
                 * @implements IInboundListValue
                 * @constructor
                 * @param {baml_core.cffi.v1.IInboundListValue=} [properties] Properties to set
                 */
                function InboundListValue(properties) {
                    this.values = [];
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * InboundListValue values.
                 * @member {Array.<baml_core.cffi.v1.IInboundValue>} values
                 * @memberof baml_core.cffi.v1.InboundListValue
                 * @instance
                 */
                InboundListValue.prototype.values = $util.emptyArray;

                /**
                 * Creates a new InboundListValue instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.InboundListValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundListValue=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.InboundListValue} InboundListValue instance
                 */
                InboundListValue.create = function create(properties) {
                    return new InboundListValue(properties);
                };

                /**
                 * Encodes the specified InboundListValue message. Does not implicitly {@link baml_core.cffi.v1.InboundListValue.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.InboundListValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundListValue} message InboundListValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundListValue.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.values != null && message.values.length)
                        for (var i = 0; i < message.values.length; ++i)
                            $root.baml_core.cffi.v1.InboundValue.encode(message.values[i], writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified InboundListValue message, length delimited. Does not implicitly {@link baml_core.cffi.v1.InboundListValue.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.InboundListValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundListValue} message InboundListValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundListValue.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an InboundListValue message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.InboundListValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.InboundListValue} InboundListValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundListValue.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.InboundListValue();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                if (!(message.values && message.values.length))
                                    message.values = [];
                                message.values.push($root.baml_core.cffi.v1.InboundValue.decode(reader, reader.uint32()));
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes an InboundListValue message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.InboundListValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.InboundListValue} InboundListValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundListValue.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies an InboundListValue message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.InboundListValue
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                InboundListValue.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.values != null && message.hasOwnProperty("values")) {
                        if (!Array.isArray(message.values))
                            return "values: array expected";
                        for (var i = 0; i < message.values.length; ++i) {
                            var error = $root.baml_core.cffi.v1.InboundValue.verify(message.values[i]);
                            if (error)
                                return "values." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates an InboundListValue message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.InboundListValue
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.InboundListValue} InboundListValue
                 */
                InboundListValue.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.InboundListValue)
                        return object;
                    var message = new $root.baml_core.cffi.v1.InboundListValue();
                    if (object.values) {
                        if (!Array.isArray(object.values))
                            throw TypeError(".baml_core.cffi.v1.InboundListValue.values: array expected");
                        message.values = [];
                        for (var i = 0; i < object.values.length; ++i) {
                            if (typeof object.values[i] !== "object")
                                throw TypeError(".baml_core.cffi.v1.InboundListValue.values: object expected");
                            message.values[i] = $root.baml_core.cffi.v1.InboundValue.fromObject(object.values[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from an InboundListValue message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.InboundListValue
                 * @static
                 * @param {baml_core.cffi.v1.InboundListValue} message InboundListValue
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                InboundListValue.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.arrays || options.defaults)
                        object.values = [];
                    if (message.values && message.values.length) {
                        object.values = [];
                        for (var j = 0; j < message.values.length; ++j)
                            object.values[j] = $root.baml_core.cffi.v1.InboundValue.toObject(message.values[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this InboundListValue to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.InboundListValue
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                InboundListValue.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for InboundListValue
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.InboundListValue
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                InboundListValue.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.InboundListValue";
                };

                return InboundListValue;
            })();

            v1.InboundMapValue = (function() {

                /**
                 * Properties of an InboundMapValue.
                 * @memberof baml_core.cffi.v1
                 * @interface IInboundMapValue
                 * @property {Array.<baml_core.cffi.v1.IInboundMapEntry>|null} [entries] InboundMapValue entries
                 */

                /**
                 * Constructs a new InboundMapValue.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents an InboundMapValue.
                 * @implements IInboundMapValue
                 * @constructor
                 * @param {baml_core.cffi.v1.IInboundMapValue=} [properties] Properties to set
                 */
                function InboundMapValue(properties) {
                    this.entries = [];
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * InboundMapValue entries.
                 * @member {Array.<baml_core.cffi.v1.IInboundMapEntry>} entries
                 * @memberof baml_core.cffi.v1.InboundMapValue
                 * @instance
                 */
                InboundMapValue.prototype.entries = $util.emptyArray;

                /**
                 * Creates a new InboundMapValue instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.InboundMapValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundMapValue=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.InboundMapValue} InboundMapValue instance
                 */
                InboundMapValue.create = function create(properties) {
                    return new InboundMapValue(properties);
                };

                /**
                 * Encodes the specified InboundMapValue message. Does not implicitly {@link baml_core.cffi.v1.InboundMapValue.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.InboundMapValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundMapValue} message InboundMapValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundMapValue.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.entries != null && message.entries.length)
                        for (var i = 0; i < message.entries.length; ++i)
                            $root.baml_core.cffi.v1.InboundMapEntry.encode(message.entries[i], writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified InboundMapValue message, length delimited. Does not implicitly {@link baml_core.cffi.v1.InboundMapValue.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.InboundMapValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundMapValue} message InboundMapValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundMapValue.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an InboundMapValue message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.InboundMapValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.InboundMapValue} InboundMapValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundMapValue.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.InboundMapValue();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                if (!(message.entries && message.entries.length))
                                    message.entries = [];
                                message.entries.push($root.baml_core.cffi.v1.InboundMapEntry.decode(reader, reader.uint32()));
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes an InboundMapValue message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.InboundMapValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.InboundMapValue} InboundMapValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundMapValue.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies an InboundMapValue message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.InboundMapValue
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                InboundMapValue.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.entries != null && message.hasOwnProperty("entries")) {
                        if (!Array.isArray(message.entries))
                            return "entries: array expected";
                        for (var i = 0; i < message.entries.length; ++i) {
                            var error = $root.baml_core.cffi.v1.InboundMapEntry.verify(message.entries[i]);
                            if (error)
                                return "entries." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates an InboundMapValue message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.InboundMapValue
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.InboundMapValue} InboundMapValue
                 */
                InboundMapValue.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.InboundMapValue)
                        return object;
                    var message = new $root.baml_core.cffi.v1.InboundMapValue();
                    if (object.entries) {
                        if (!Array.isArray(object.entries))
                            throw TypeError(".baml_core.cffi.v1.InboundMapValue.entries: array expected");
                        message.entries = [];
                        for (var i = 0; i < object.entries.length; ++i) {
                            if (typeof object.entries[i] !== "object")
                                throw TypeError(".baml_core.cffi.v1.InboundMapValue.entries: object expected");
                            message.entries[i] = $root.baml_core.cffi.v1.InboundMapEntry.fromObject(object.entries[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from an InboundMapValue message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.InboundMapValue
                 * @static
                 * @param {baml_core.cffi.v1.InboundMapValue} message InboundMapValue
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                InboundMapValue.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.arrays || options.defaults)
                        object.entries = [];
                    if (message.entries && message.entries.length) {
                        object.entries = [];
                        for (var j = 0; j < message.entries.length; ++j)
                            object.entries[j] = $root.baml_core.cffi.v1.InboundMapEntry.toObject(message.entries[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this InboundMapValue to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.InboundMapValue
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                InboundMapValue.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for InboundMapValue
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.InboundMapValue
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                InboundMapValue.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.InboundMapValue";
                };

                return InboundMapValue;
            })();

            v1.InboundMapEntry = (function() {

                /**
                 * Properties of an InboundMapEntry.
                 * @memberof baml_core.cffi.v1
                 * @interface IInboundMapEntry
                 * @property {string|null} [stringKey] InboundMapEntry stringKey
                 * @property {number|Long|null} [intKey] InboundMapEntry intKey
                 * @property {boolean|null} [boolKey] InboundMapEntry boolKey
                 * @property {baml_core.cffi.v1.IInboundEnumValue|null} [enumKey] InboundMapEntry enumKey
                 * @property {baml_core.cffi.v1.IInboundValue|null} [value] InboundMapEntry value
                 */

                /**
                 * Constructs a new InboundMapEntry.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents an InboundMapEntry.
                 * @implements IInboundMapEntry
                 * @constructor
                 * @param {baml_core.cffi.v1.IInboundMapEntry=} [properties] Properties to set
                 */
                function InboundMapEntry(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * InboundMapEntry stringKey.
                 * @member {string|null|undefined} stringKey
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @instance
                 */
                InboundMapEntry.prototype.stringKey = null;

                /**
                 * InboundMapEntry intKey.
                 * @member {number|Long|null|undefined} intKey
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @instance
                 */
                InboundMapEntry.prototype.intKey = null;

                /**
                 * InboundMapEntry boolKey.
                 * @member {boolean|null|undefined} boolKey
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @instance
                 */
                InboundMapEntry.prototype.boolKey = null;

                /**
                 * InboundMapEntry enumKey.
                 * @member {baml_core.cffi.v1.IInboundEnumValue|null|undefined} enumKey
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @instance
                 */
                InboundMapEntry.prototype.enumKey = null;

                /**
                 * InboundMapEntry value.
                 * @member {baml_core.cffi.v1.IInboundValue|null|undefined} value
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @instance
                 */
                InboundMapEntry.prototype.value = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * InboundMapEntry key.
                 * @member {"stringKey"|"intKey"|"boolKey"|"enumKey"|undefined} key
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @instance
                 */
                Object.defineProperty(InboundMapEntry.prototype, "key", {
                    get: $util.oneOfGetter($oneOfFields = ["stringKey", "intKey", "boolKey", "enumKey"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new InboundMapEntry instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @static
                 * @param {baml_core.cffi.v1.IInboundMapEntry=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.InboundMapEntry} InboundMapEntry instance
                 */
                InboundMapEntry.create = function create(properties) {
                    return new InboundMapEntry(properties);
                };

                /**
                 * Encodes the specified InboundMapEntry message. Does not implicitly {@link baml_core.cffi.v1.InboundMapEntry.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @static
                 * @param {baml_core.cffi.v1.IInboundMapEntry} message InboundMapEntry message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundMapEntry.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.stringKey != null && Object.hasOwnProperty.call(message, "stringKey"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.stringKey);
                    if (message.intKey != null && Object.hasOwnProperty.call(message, "intKey"))
                        writer.uint32(/* id 2, wireType 0 =*/16).int64(message.intKey);
                    if (message.boolKey != null && Object.hasOwnProperty.call(message, "boolKey"))
                        writer.uint32(/* id 3, wireType 0 =*/24).bool(message.boolKey);
                    if (message.enumKey != null && Object.hasOwnProperty.call(message, "enumKey"))
                        $root.baml_core.cffi.v1.InboundEnumValue.encode(message.enumKey, writer.uint32(/* id 5, wireType 2 =*/42).fork()).ldelim();
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        $root.baml_core.cffi.v1.InboundValue.encode(message.value, writer.uint32(/* id 6, wireType 2 =*/50).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified InboundMapEntry message, length delimited. Does not implicitly {@link baml_core.cffi.v1.InboundMapEntry.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @static
                 * @param {baml_core.cffi.v1.IInboundMapEntry} message InboundMapEntry message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundMapEntry.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an InboundMapEntry message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.InboundMapEntry} InboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundMapEntry.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.InboundMapEntry();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.stringKey = reader.string();
                                break;
                            }
                        case 2: {
                                message.intKey = reader.int64();
                                break;
                            }
                        case 3: {
                                message.boolKey = reader.bool();
                                break;
                            }
                        case 5: {
                                message.enumKey = $root.baml_core.cffi.v1.InboundEnumValue.decode(reader, reader.uint32());
                                break;
                            }
                        case 6: {
                                message.value = $root.baml_core.cffi.v1.InboundValue.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes an InboundMapEntry message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.InboundMapEntry} InboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundMapEntry.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies an InboundMapEntry message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                InboundMapEntry.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    var properties = {};
                    if (message.stringKey != null && message.hasOwnProperty("stringKey")) {
                        properties.key = 1;
                        if (!$util.isString(message.stringKey))
                            return "stringKey: string expected";
                    }
                    if (message.intKey != null && message.hasOwnProperty("intKey")) {
                        if (properties.key === 1)
                            return "key: multiple values";
                        properties.key = 1;
                        if (!$util.isInteger(message.intKey) && !(message.intKey && $util.isInteger(message.intKey.low) && $util.isInteger(message.intKey.high)))
                            return "intKey: integer|Long expected";
                    }
                    if (message.boolKey != null && message.hasOwnProperty("boolKey")) {
                        if (properties.key === 1)
                            return "key: multiple values";
                        properties.key = 1;
                        if (typeof message.boolKey !== "boolean")
                            return "boolKey: boolean expected";
                    }
                    if (message.enumKey != null && message.hasOwnProperty("enumKey")) {
                        if (properties.key === 1)
                            return "key: multiple values";
                        properties.key = 1;
                        {
                            var error = $root.baml_core.cffi.v1.InboundEnumValue.verify(message.enumKey);
                            if (error)
                                return "enumKey." + error;
                        }
                    }
                    if (message.value != null && message.hasOwnProperty("value")) {
                        var error = $root.baml_core.cffi.v1.InboundValue.verify(message.value);
                        if (error)
                            return "value." + error;
                    }
                    return null;
                };

                /**
                 * Creates an InboundMapEntry message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.InboundMapEntry} InboundMapEntry
                 */
                InboundMapEntry.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.InboundMapEntry)
                        return object;
                    var message = new $root.baml_core.cffi.v1.InboundMapEntry();
                    if (object.stringKey != null)
                        message.stringKey = String(object.stringKey);
                    if (object.intKey != null)
                        if ($util.Long)
                            (message.intKey = $util.Long.fromValue(object.intKey)).unsigned = false;
                        else if (typeof object.intKey === "string")
                            message.intKey = parseInt(object.intKey, 10);
                        else if (typeof object.intKey === "number")
                            message.intKey = object.intKey;
                        else if (typeof object.intKey === "object")
                            message.intKey = new $util.LongBits(object.intKey.low >>> 0, object.intKey.high >>> 0).toNumber();
                    if (object.boolKey != null)
                        message.boolKey = Boolean(object.boolKey);
                    if (object.enumKey != null) {
                        if (typeof object.enumKey !== "object")
                            throw TypeError(".baml_core.cffi.v1.InboundMapEntry.enumKey: object expected");
                        message.enumKey = $root.baml_core.cffi.v1.InboundEnumValue.fromObject(object.enumKey);
                    }
                    if (object.value != null) {
                        if (typeof object.value !== "object")
                            throw TypeError(".baml_core.cffi.v1.InboundMapEntry.value: object expected");
                        message.value = $root.baml_core.cffi.v1.InboundValue.fromObject(object.value);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from an InboundMapEntry message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @static
                 * @param {baml_core.cffi.v1.InboundMapEntry} message InboundMapEntry
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                InboundMapEntry.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.value = null;
                    if (message.stringKey != null && message.hasOwnProperty("stringKey")) {
                        object.stringKey = message.stringKey;
                        if (options.oneofs)
                            object.key = "stringKey";
                    }
                    if (message.intKey != null && message.hasOwnProperty("intKey")) {
                        if (typeof message.intKey === "number")
                            object.intKey = options.longs === String ? String(message.intKey) : message.intKey;
                        else
                            object.intKey = options.longs === String ? $util.Long.prototype.toString.call(message.intKey) : options.longs === Number ? new $util.LongBits(message.intKey.low >>> 0, message.intKey.high >>> 0).toNumber() : message.intKey;
                        if (options.oneofs)
                            object.key = "intKey";
                    }
                    if (message.boolKey != null && message.hasOwnProperty("boolKey")) {
                        object.boolKey = message.boolKey;
                        if (options.oneofs)
                            object.key = "boolKey";
                    }
                    if (message.enumKey != null && message.hasOwnProperty("enumKey")) {
                        object.enumKey = $root.baml_core.cffi.v1.InboundEnumValue.toObject(message.enumKey, options);
                        if (options.oneofs)
                            object.key = "enumKey";
                    }
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = $root.baml_core.cffi.v1.InboundValue.toObject(message.value, options);
                    return object;
                };

                /**
                 * Converts this InboundMapEntry to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                InboundMapEntry.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for InboundMapEntry
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.InboundMapEntry
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                InboundMapEntry.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.InboundMapEntry";
                };

                return InboundMapEntry;
            })();

            v1.InboundClassValue = (function() {

                /**
                 * Properties of an InboundClassValue.
                 * @memberof baml_core.cffi.v1
                 * @interface IInboundClassValue
                 * @property {string|null} [name] InboundClassValue name
                 * @property {Array.<baml_core.cffi.v1.IInboundMapEntry>|null} [fields] InboundClassValue fields
                 */

                /**
                 * Constructs a new InboundClassValue.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents an InboundClassValue.
                 * @implements IInboundClassValue
                 * @constructor
                 * @param {baml_core.cffi.v1.IInboundClassValue=} [properties] Properties to set
                 */
                function InboundClassValue(properties) {
                    this.fields = [];
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * InboundClassValue name.
                 * @member {string} name
                 * @memberof baml_core.cffi.v1.InboundClassValue
                 * @instance
                 */
                InboundClassValue.prototype.name = "";

                /**
                 * InboundClassValue fields.
                 * @member {Array.<baml_core.cffi.v1.IInboundMapEntry>} fields
                 * @memberof baml_core.cffi.v1.InboundClassValue
                 * @instance
                 */
                InboundClassValue.prototype.fields = $util.emptyArray;

                /**
                 * Creates a new InboundClassValue instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.InboundClassValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundClassValue=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.InboundClassValue} InboundClassValue instance
                 */
                InboundClassValue.create = function create(properties) {
                    return new InboundClassValue(properties);
                };

                /**
                 * Encodes the specified InboundClassValue message. Does not implicitly {@link baml_core.cffi.v1.InboundClassValue.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.InboundClassValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundClassValue} message InboundClassValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundClassValue.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.name);
                    if (message.fields != null && message.fields.length)
                        for (var i = 0; i < message.fields.length; ++i)
                            $root.baml_core.cffi.v1.InboundMapEntry.encode(message.fields[i], writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified InboundClassValue message, length delimited. Does not implicitly {@link baml_core.cffi.v1.InboundClassValue.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.InboundClassValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundClassValue} message InboundClassValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundClassValue.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an InboundClassValue message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.InboundClassValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.InboundClassValue} InboundClassValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundClassValue.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.InboundClassValue();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = reader.string();
                                break;
                            }
                        case 2: {
                                if (!(message.fields && message.fields.length))
                                    message.fields = [];
                                message.fields.push($root.baml_core.cffi.v1.InboundMapEntry.decode(reader, reader.uint32()));
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes an InboundClassValue message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.InboundClassValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.InboundClassValue} InboundClassValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundClassValue.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies an InboundClassValue message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.InboundClassValue
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                InboundClassValue.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name"))
                        if (!$util.isString(message.name))
                            return "name: string expected";
                    if (message.fields != null && message.hasOwnProperty("fields")) {
                        if (!Array.isArray(message.fields))
                            return "fields: array expected";
                        for (var i = 0; i < message.fields.length; ++i) {
                            var error = $root.baml_core.cffi.v1.InboundMapEntry.verify(message.fields[i]);
                            if (error)
                                return "fields." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates an InboundClassValue message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.InboundClassValue
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.InboundClassValue} InboundClassValue
                 */
                InboundClassValue.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.InboundClassValue)
                        return object;
                    var message = new $root.baml_core.cffi.v1.InboundClassValue();
                    if (object.name != null)
                        message.name = String(object.name);
                    if (object.fields) {
                        if (!Array.isArray(object.fields))
                            throw TypeError(".baml_core.cffi.v1.InboundClassValue.fields: array expected");
                        message.fields = [];
                        for (var i = 0; i < object.fields.length; ++i) {
                            if (typeof object.fields[i] !== "object")
                                throw TypeError(".baml_core.cffi.v1.InboundClassValue.fields: object expected");
                            message.fields[i] = $root.baml_core.cffi.v1.InboundMapEntry.fromObject(object.fields[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from an InboundClassValue message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.InboundClassValue
                 * @static
                 * @param {baml_core.cffi.v1.InboundClassValue} message InboundClassValue
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                InboundClassValue.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.arrays || options.defaults)
                        object.fields = [];
                    if (options.defaults)
                        object.name = "";
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = message.name;
                    if (message.fields && message.fields.length) {
                        object.fields = [];
                        for (var j = 0; j < message.fields.length; ++j)
                            object.fields[j] = $root.baml_core.cffi.v1.InboundMapEntry.toObject(message.fields[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this InboundClassValue to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.InboundClassValue
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                InboundClassValue.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for InboundClassValue
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.InboundClassValue
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                InboundClassValue.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.InboundClassValue";
                };

                return InboundClassValue;
            })();

            v1.InboundEnumValue = (function() {

                /**
                 * Properties of an InboundEnumValue.
                 * @memberof baml_core.cffi.v1
                 * @interface IInboundEnumValue
                 * @property {string|null} [name] InboundEnumValue name
                 * @property {string|null} [value] InboundEnumValue value
                 */

                /**
                 * Constructs a new InboundEnumValue.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents an InboundEnumValue.
                 * @implements IInboundEnumValue
                 * @constructor
                 * @param {baml_core.cffi.v1.IInboundEnumValue=} [properties] Properties to set
                 */
                function InboundEnumValue(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * InboundEnumValue name.
                 * @member {string} name
                 * @memberof baml_core.cffi.v1.InboundEnumValue
                 * @instance
                 */
                InboundEnumValue.prototype.name = "";

                /**
                 * InboundEnumValue value.
                 * @member {string} value
                 * @memberof baml_core.cffi.v1.InboundEnumValue
                 * @instance
                 */
                InboundEnumValue.prototype.value = "";

                /**
                 * Creates a new InboundEnumValue instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.InboundEnumValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundEnumValue=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.InboundEnumValue} InboundEnumValue instance
                 */
                InboundEnumValue.create = function create(properties) {
                    return new InboundEnumValue(properties);
                };

                /**
                 * Encodes the specified InboundEnumValue message. Does not implicitly {@link baml_core.cffi.v1.InboundEnumValue.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.InboundEnumValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundEnumValue} message InboundEnumValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundEnumValue.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.name);
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.value);
                    return writer;
                };

                /**
                 * Encodes the specified InboundEnumValue message, length delimited. Does not implicitly {@link baml_core.cffi.v1.InboundEnumValue.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.InboundEnumValue
                 * @static
                 * @param {baml_core.cffi.v1.IInboundEnumValue} message InboundEnumValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundEnumValue.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an InboundEnumValue message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.InboundEnumValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.InboundEnumValue} InboundEnumValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundEnumValue.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.InboundEnumValue();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = reader.string();
                                break;
                            }
                        case 2: {
                                message.value = reader.string();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes an InboundEnumValue message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.InboundEnumValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.InboundEnumValue} InboundEnumValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundEnumValue.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies an InboundEnumValue message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.InboundEnumValue
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                InboundEnumValue.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name"))
                        if (!$util.isString(message.name))
                            return "name: string expected";
                    if (message.value != null && message.hasOwnProperty("value"))
                        if (!$util.isString(message.value))
                            return "value: string expected";
                    return null;
                };

                /**
                 * Creates an InboundEnumValue message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.InboundEnumValue
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.InboundEnumValue} InboundEnumValue
                 */
                InboundEnumValue.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.InboundEnumValue)
                        return object;
                    var message = new $root.baml_core.cffi.v1.InboundEnumValue();
                    if (object.name != null)
                        message.name = String(object.name);
                    if (object.value != null)
                        message.value = String(object.value);
                    return message;
                };

                /**
                 * Creates a plain object from an InboundEnumValue message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.InboundEnumValue
                 * @static
                 * @param {baml_core.cffi.v1.InboundEnumValue} message InboundEnumValue
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                InboundEnumValue.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults) {
                        object.name = "";
                        object.value = "";
                    }
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = message.name;
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = message.value;
                    return object;
                };

                /**
                 * Converts this InboundEnumValue to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.InboundEnumValue
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                InboundEnumValue.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for InboundEnumValue
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.InboundEnumValue
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                InboundEnumValue.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.InboundEnumValue";
                };

                return InboundEnumValue;
            })();

            v1.CallFunctionArgs = (function() {

                /**
                 * Properties of a CallFunctionArgs.
                 * @memberof baml_core.cffi.v1
                 * @interface ICallFunctionArgs
                 * @property {Array.<baml_core.cffi.v1.IInboundMapEntry>|null} [kwargs] CallFunctionArgs kwargs
                 */

                /**
                 * Constructs a new CallFunctionArgs.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a CallFunctionArgs.
                 * @implements ICallFunctionArgs
                 * @constructor
                 * @param {baml_core.cffi.v1.ICallFunctionArgs=} [properties] Properties to set
                 */
                function CallFunctionArgs(properties) {
                    this.kwargs = [];
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * CallFunctionArgs kwargs.
                 * @member {Array.<baml_core.cffi.v1.IInboundMapEntry>} kwargs
                 * @memberof baml_core.cffi.v1.CallFunctionArgs
                 * @instance
                 */
                CallFunctionArgs.prototype.kwargs = $util.emptyArray;

                /**
                 * Creates a new CallFunctionArgs instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {baml_core.cffi.v1.ICallFunctionArgs=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.CallFunctionArgs} CallFunctionArgs instance
                 */
                CallFunctionArgs.create = function create(properties) {
                    return new CallFunctionArgs(properties);
                };

                /**
                 * Encodes the specified CallFunctionArgs message. Does not implicitly {@link baml_core.cffi.v1.CallFunctionArgs.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {baml_core.cffi.v1.ICallFunctionArgs} message CallFunctionArgs message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CallFunctionArgs.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.kwargs != null && message.kwargs.length)
                        for (var i = 0; i < message.kwargs.length; ++i)
                            $root.baml_core.cffi.v1.InboundMapEntry.encode(message.kwargs[i], writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified CallFunctionArgs message, length delimited. Does not implicitly {@link baml_core.cffi.v1.CallFunctionArgs.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {baml_core.cffi.v1.ICallFunctionArgs} message CallFunctionArgs message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CallFunctionArgs.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a CallFunctionArgs message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.CallFunctionArgs} CallFunctionArgs
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CallFunctionArgs.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.CallFunctionArgs();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                if (!(message.kwargs && message.kwargs.length))
                                    message.kwargs = [];
                                message.kwargs.push($root.baml_core.cffi.v1.InboundMapEntry.decode(reader, reader.uint32()));
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a CallFunctionArgs message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.CallFunctionArgs} CallFunctionArgs
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CallFunctionArgs.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a CallFunctionArgs message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                CallFunctionArgs.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.kwargs != null && message.hasOwnProperty("kwargs")) {
                        if (!Array.isArray(message.kwargs))
                            return "kwargs: array expected";
                        for (var i = 0; i < message.kwargs.length; ++i) {
                            var error = $root.baml_core.cffi.v1.InboundMapEntry.verify(message.kwargs[i]);
                            if (error)
                                return "kwargs." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a CallFunctionArgs message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.CallFunctionArgs} CallFunctionArgs
                 */
                CallFunctionArgs.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.CallFunctionArgs)
                        return object;
                    var message = new $root.baml_core.cffi.v1.CallFunctionArgs();
                    if (object.kwargs) {
                        if (!Array.isArray(object.kwargs))
                            throw TypeError(".baml_core.cffi.v1.CallFunctionArgs.kwargs: array expected");
                        message.kwargs = [];
                        for (var i = 0; i < object.kwargs.length; ++i) {
                            if (typeof object.kwargs[i] !== "object")
                                throw TypeError(".baml_core.cffi.v1.CallFunctionArgs.kwargs: object expected");
                            message.kwargs[i] = $root.baml_core.cffi.v1.InboundMapEntry.fromObject(object.kwargs[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a CallFunctionArgs message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {baml_core.cffi.v1.CallFunctionArgs} message CallFunctionArgs
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                CallFunctionArgs.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.arrays || options.defaults)
                        object.kwargs = [];
                    if (message.kwargs && message.kwargs.length) {
                        object.kwargs = [];
                        for (var j = 0; j < message.kwargs.length; ++j)
                            object.kwargs[j] = $root.baml_core.cffi.v1.InboundMapEntry.toObject(message.kwargs[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this CallFunctionArgs to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.CallFunctionArgs
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                CallFunctionArgs.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for CallFunctionArgs
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                CallFunctionArgs.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.CallFunctionArgs";
                };

                return CallFunctionArgs;
            })();

            v1.CallAck = (function() {

                /**
                 * Properties of a CallAck.
                 * @memberof baml_core.cffi.v1
                 * @interface ICallAck
                 * @property {string|null} [error] CallAck error
                 */

                /**
                 * Constructs a new CallAck.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a CallAck.
                 * @implements ICallAck
                 * @constructor
                 * @param {baml_core.cffi.v1.ICallAck=} [properties] Properties to set
                 */
                function CallAck(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * CallAck error.
                 * @member {string|null|undefined} error
                 * @memberof baml_core.cffi.v1.CallAck
                 * @instance
                 */
                CallAck.prototype.error = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * CallAck response.
                 * @member {"error"|undefined} response
                 * @memberof baml_core.cffi.v1.CallAck
                 * @instance
                 */
                Object.defineProperty(CallAck.prototype, "response", {
                    get: $util.oneOfGetter($oneOfFields = ["error"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new CallAck instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.CallAck
                 * @static
                 * @param {baml_core.cffi.v1.ICallAck=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.CallAck} CallAck instance
                 */
                CallAck.create = function create(properties) {
                    return new CallAck(properties);
                };

                /**
                 * Encodes the specified CallAck message. Does not implicitly {@link baml_core.cffi.v1.CallAck.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.CallAck
                 * @static
                 * @param {baml_core.cffi.v1.ICallAck} message CallAck message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CallAck.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.error != null && Object.hasOwnProperty.call(message, "error"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.error);
                    return writer;
                };

                /**
                 * Encodes the specified CallAck message, length delimited. Does not implicitly {@link baml_core.cffi.v1.CallAck.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.CallAck
                 * @static
                 * @param {baml_core.cffi.v1.ICallAck} message CallAck message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CallAck.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a CallAck message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.CallAck
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.CallAck} CallAck
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CallAck.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.CallAck();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.error = reader.string();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a CallAck message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.CallAck
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.CallAck} CallAck
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CallAck.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a CallAck message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.CallAck
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                CallAck.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    var properties = {};
                    if (message.error != null && message.hasOwnProperty("error")) {
                        properties.response = 1;
                        if (!$util.isString(message.error))
                            return "error: string expected";
                    }
                    return null;
                };

                /**
                 * Creates a CallAck message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.CallAck
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.CallAck} CallAck
                 */
                CallAck.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.CallAck)
                        return object;
                    var message = new $root.baml_core.cffi.v1.CallAck();
                    if (object.error != null)
                        message.error = String(object.error);
                    return message;
                };

                /**
                 * Creates a plain object from a CallAck message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.CallAck
                 * @static
                 * @param {baml_core.cffi.v1.CallAck} message CallAck
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                CallAck.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (message.error != null && message.hasOwnProperty("error")) {
                        object.error = message.error;
                        if (options.oneofs)
                            object.response = "error";
                    }
                    return object;
                };

                /**
                 * Converts this CallAck to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.CallAck
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                CallAck.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for CallAck
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.CallAck
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                CallAck.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.CallAck";
                };

                return CallAck;
            })();

            v1.BamlOutboundValue = (function() {

                /**
                 * Properties of a BamlOutboundValue.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlOutboundValue
                 * @property {baml_core.cffi.v1.IBamlValueNull|null} [nullValue] BamlOutboundValue nullValue
                 * @property {string|null} [stringValue] BamlOutboundValue stringValue
                 * @property {number|Long|null} [intValue] BamlOutboundValue intValue
                 * @property {number|null} [floatValue] BamlOutboundValue floatValue
                 * @property {boolean|null} [boolValue] BamlOutboundValue boolValue
                 * @property {baml_core.cffi.v1.IBamlValueClass|null} [classValue] BamlOutboundValue classValue
                 * @property {baml_core.cffi.v1.IBamlValueEnum|null} [enumValue] BamlOutboundValue enumValue
                 * @property {baml_core.cffi.v1.IBamlTyLiteral|null} [literalValue] BamlOutboundValue literalValue
                 * @property {baml_core.cffi.v1.IBamlValueList|null} [listValue] BamlOutboundValue listValue
                 * @property {baml_core.cffi.v1.IBamlValueMap|null} [mapValue] BamlOutboundValue mapValue
                 * @property {baml_core.cffi.v1.IBamlValueUnionVariant|null} [unionVariantValue] BamlOutboundValue unionVariantValue
                 * @property {baml_core.cffi.v1.IBamlOutboundHandle|null} [handleValue] BamlOutboundValue handleValue
                 * @property {baml_core.cffi.v1.IBamlValueMedia|null} [mediaValue] BamlOutboundValue mediaValue
                 * @property {baml_core.cffi.v1.IBamlValuePromptAst|null} [promptAstValue] BamlOutboundValue promptAstValue
                 * @property {Uint8Array|null} [uint8arrayValue] BamlOutboundValue uint8arrayValue
                 */

                /**
                 * Constructs a new BamlOutboundValue.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlOutboundValue.
                 * @implements IBamlOutboundValue
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlOutboundValue=} [properties] Properties to set
                 */
                function BamlOutboundValue(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlOutboundValue nullValue.
                 * @member {baml_core.cffi.v1.IBamlValueNull|null|undefined} nullValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.nullValue = null;

                /**
                 * BamlOutboundValue stringValue.
                 * @member {string|null|undefined} stringValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.stringValue = null;

                /**
                 * BamlOutboundValue intValue.
                 * @member {number|Long|null|undefined} intValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.intValue = null;

                /**
                 * BamlOutboundValue floatValue.
                 * @member {number|null|undefined} floatValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.floatValue = null;

                /**
                 * BamlOutboundValue boolValue.
                 * @member {boolean|null|undefined} boolValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.boolValue = null;

                /**
                 * BamlOutboundValue classValue.
                 * @member {baml_core.cffi.v1.IBamlValueClass|null|undefined} classValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.classValue = null;

                /**
                 * BamlOutboundValue enumValue.
                 * @member {baml_core.cffi.v1.IBamlValueEnum|null|undefined} enumValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.enumValue = null;

                /**
                 * BamlOutboundValue literalValue.
                 * @member {baml_core.cffi.v1.IBamlTyLiteral|null|undefined} literalValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.literalValue = null;

                /**
                 * BamlOutboundValue listValue.
                 * @member {baml_core.cffi.v1.IBamlValueList|null|undefined} listValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.listValue = null;

                /**
                 * BamlOutboundValue mapValue.
                 * @member {baml_core.cffi.v1.IBamlValueMap|null|undefined} mapValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.mapValue = null;

                /**
                 * BamlOutboundValue unionVariantValue.
                 * @member {baml_core.cffi.v1.IBamlValueUnionVariant|null|undefined} unionVariantValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.unionVariantValue = null;

                /**
                 * BamlOutboundValue handleValue.
                 * @member {baml_core.cffi.v1.IBamlOutboundHandle|null|undefined} handleValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.handleValue = null;

                /**
                 * BamlOutboundValue mediaValue.
                 * @member {baml_core.cffi.v1.IBamlValueMedia|null|undefined} mediaValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.mediaValue = null;

                /**
                 * BamlOutboundValue promptAstValue.
                 * @member {baml_core.cffi.v1.IBamlValuePromptAst|null|undefined} promptAstValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.promptAstValue = null;

                /**
                 * BamlOutboundValue uint8arrayValue.
                 * @member {Uint8Array|null|undefined} uint8arrayValue
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.uint8arrayValue = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * BamlOutboundValue value.
                 * @member {"nullValue"|"stringValue"|"intValue"|"floatValue"|"boolValue"|"classValue"|"enumValue"|"literalValue"|"listValue"|"mapValue"|"unionVariantValue"|"handleValue"|"mediaValue"|"promptAstValue"|"uint8arrayValue"|undefined} value
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                Object.defineProperty(BamlOutboundValue.prototype, "value", {
                    get: $util.oneOfGetter($oneOfFields = ["nullValue", "stringValue", "intValue", "floatValue", "boolValue", "classValue", "enumValue", "literalValue", "listValue", "mapValue", "unionVariantValue", "handleValue", "mediaValue", "promptAstValue", "uint8arrayValue"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new BamlOutboundValue instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {baml_core.cffi.v1.IBamlOutboundValue=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlOutboundValue} BamlOutboundValue instance
                 */
                BamlOutboundValue.create = function create(properties) {
                    return new BamlOutboundValue(properties);
                };

                /**
                 * Encodes the specified BamlOutboundValue message. Does not implicitly {@link baml_core.cffi.v1.BamlOutboundValue.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {baml_core.cffi.v1.IBamlOutboundValue} message BamlOutboundValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlOutboundValue.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.nullValue != null && Object.hasOwnProperty.call(message, "nullValue"))
                        $root.baml_core.cffi.v1.BamlValueNull.encode(message.nullValue, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.stringValue != null && Object.hasOwnProperty.call(message, "stringValue"))
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.stringValue);
                    if (message.intValue != null && Object.hasOwnProperty.call(message, "intValue"))
                        writer.uint32(/* id 4, wireType 0 =*/32).int64(message.intValue);
                    if (message.floatValue != null && Object.hasOwnProperty.call(message, "floatValue"))
                        writer.uint32(/* id 5, wireType 1 =*/41).double(message.floatValue);
                    if (message.boolValue != null && Object.hasOwnProperty.call(message, "boolValue"))
                        writer.uint32(/* id 6, wireType 0 =*/48).bool(message.boolValue);
                    if (message.classValue != null && Object.hasOwnProperty.call(message, "classValue"))
                        $root.baml_core.cffi.v1.BamlValueClass.encode(message.classValue, writer.uint32(/* id 7, wireType 2 =*/58).fork()).ldelim();
                    if (message.enumValue != null && Object.hasOwnProperty.call(message, "enumValue"))
                        $root.baml_core.cffi.v1.BamlValueEnum.encode(message.enumValue, writer.uint32(/* id 8, wireType 2 =*/66).fork()).ldelim();
                    if (message.literalValue != null && Object.hasOwnProperty.call(message, "literalValue"))
                        $root.baml_core.cffi.v1.BamlTyLiteral.encode(message.literalValue, writer.uint32(/* id 9, wireType 2 =*/74).fork()).ldelim();
                    if (message.listValue != null && Object.hasOwnProperty.call(message, "listValue"))
                        $root.baml_core.cffi.v1.BamlValueList.encode(message.listValue, writer.uint32(/* id 11, wireType 2 =*/90).fork()).ldelim();
                    if (message.mapValue != null && Object.hasOwnProperty.call(message, "mapValue"))
                        $root.baml_core.cffi.v1.BamlValueMap.encode(message.mapValue, writer.uint32(/* id 12, wireType 2 =*/98).fork()).ldelim();
                    if (message.unionVariantValue != null && Object.hasOwnProperty.call(message, "unionVariantValue"))
                        $root.baml_core.cffi.v1.BamlValueUnionVariant.encode(message.unionVariantValue, writer.uint32(/* id 13, wireType 2 =*/106).fork()).ldelim();
                    if (message.handleValue != null && Object.hasOwnProperty.call(message, "handleValue"))
                        $root.baml_core.cffi.v1.BamlOutboundHandle.encode(message.handleValue, writer.uint32(/* id 16, wireType 2 =*/130).fork()).ldelim();
                    if (message.mediaValue != null && Object.hasOwnProperty.call(message, "mediaValue"))
                        $root.baml_core.cffi.v1.BamlValueMedia.encode(message.mediaValue, writer.uint32(/* id 17, wireType 2 =*/138).fork()).ldelim();
                    if (message.promptAstValue != null && Object.hasOwnProperty.call(message, "promptAstValue"))
                        $root.baml_core.cffi.v1.BamlValuePromptAst.encode(message.promptAstValue, writer.uint32(/* id 18, wireType 2 =*/146).fork()).ldelim();
                    if (message.uint8arrayValue != null && Object.hasOwnProperty.call(message, "uint8arrayValue"))
                        writer.uint32(/* id 19, wireType 2 =*/154).bytes(message.uint8arrayValue);
                    return writer;
                };

                /**
                 * Encodes the specified BamlOutboundValue message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlOutboundValue.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {baml_core.cffi.v1.IBamlOutboundValue} message BamlOutboundValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlOutboundValue.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlOutboundValue message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlOutboundValue} BamlOutboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlOutboundValue.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlOutboundValue();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 2: {
                                message.nullValue = $root.baml_core.cffi.v1.BamlValueNull.decode(reader, reader.uint32());
                                break;
                            }
                        case 3: {
                                message.stringValue = reader.string();
                                break;
                            }
                        case 4: {
                                message.intValue = reader.int64();
                                break;
                            }
                        case 5: {
                                message.floatValue = reader.double();
                                break;
                            }
                        case 6: {
                                message.boolValue = reader.bool();
                                break;
                            }
                        case 7: {
                                message.classValue = $root.baml_core.cffi.v1.BamlValueClass.decode(reader, reader.uint32());
                                break;
                            }
                        case 8: {
                                message.enumValue = $root.baml_core.cffi.v1.BamlValueEnum.decode(reader, reader.uint32());
                                break;
                            }
                        case 9: {
                                message.literalValue = $root.baml_core.cffi.v1.BamlTyLiteral.decode(reader, reader.uint32());
                                break;
                            }
                        case 11: {
                                message.listValue = $root.baml_core.cffi.v1.BamlValueList.decode(reader, reader.uint32());
                                break;
                            }
                        case 12: {
                                message.mapValue = $root.baml_core.cffi.v1.BamlValueMap.decode(reader, reader.uint32());
                                break;
                            }
                        case 13: {
                                message.unionVariantValue = $root.baml_core.cffi.v1.BamlValueUnionVariant.decode(reader, reader.uint32());
                                break;
                            }
                        case 16: {
                                message.handleValue = $root.baml_core.cffi.v1.BamlOutboundHandle.decode(reader, reader.uint32());
                                break;
                            }
                        case 17: {
                                message.mediaValue = $root.baml_core.cffi.v1.BamlValueMedia.decode(reader, reader.uint32());
                                break;
                            }
                        case 18: {
                                message.promptAstValue = $root.baml_core.cffi.v1.BamlValuePromptAst.decode(reader, reader.uint32());
                                break;
                            }
                        case 19: {
                                message.uint8arrayValue = reader.bytes();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlOutboundValue message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlOutboundValue} BamlOutboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlOutboundValue.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlOutboundValue message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlOutboundValue.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    var properties = {};
                    if (message.nullValue != null && message.hasOwnProperty("nullValue")) {
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlValueNull.verify(message.nullValue);
                            if (error)
                                return "nullValue." + error;
                        }
                    }
                    if (message.stringValue != null && message.hasOwnProperty("stringValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        if (!$util.isString(message.stringValue))
                            return "stringValue: string expected";
                    }
                    if (message.intValue != null && message.hasOwnProperty("intValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        if (!$util.isInteger(message.intValue) && !(message.intValue && $util.isInteger(message.intValue.low) && $util.isInteger(message.intValue.high)))
                            return "intValue: integer|Long expected";
                    }
                    if (message.floatValue != null && message.hasOwnProperty("floatValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        if (typeof message.floatValue !== "number")
                            return "floatValue: number expected";
                    }
                    if (message.boolValue != null && message.hasOwnProperty("boolValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        if (typeof message.boolValue !== "boolean")
                            return "boolValue: boolean expected";
                    }
                    if (message.classValue != null && message.hasOwnProperty("classValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlValueClass.verify(message.classValue);
                            if (error)
                                return "classValue." + error;
                        }
                    }
                    if (message.enumValue != null && message.hasOwnProperty("enumValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlValueEnum.verify(message.enumValue);
                            if (error)
                                return "enumValue." + error;
                        }
                    }
                    if (message.literalValue != null && message.hasOwnProperty("literalValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyLiteral.verify(message.literalValue);
                            if (error)
                                return "literalValue." + error;
                        }
                    }
                    if (message.listValue != null && message.hasOwnProperty("listValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlValueList.verify(message.listValue);
                            if (error)
                                return "listValue." + error;
                        }
                    }
                    if (message.mapValue != null && message.hasOwnProperty("mapValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlValueMap.verify(message.mapValue);
                            if (error)
                                return "mapValue." + error;
                        }
                    }
                    if (message.unionVariantValue != null && message.hasOwnProperty("unionVariantValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlValueUnionVariant.verify(message.unionVariantValue);
                            if (error)
                                return "unionVariantValue." + error;
                        }
                    }
                    if (message.handleValue != null && message.hasOwnProperty("handleValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlOutboundHandle.verify(message.handleValue);
                            if (error)
                                return "handleValue." + error;
                        }
                    }
                    if (message.mediaValue != null && message.hasOwnProperty("mediaValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlValueMedia.verify(message.mediaValue);
                            if (error)
                                return "mediaValue." + error;
                        }
                    }
                    if (message.promptAstValue != null && message.hasOwnProperty("promptAstValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlValuePromptAst.verify(message.promptAstValue);
                            if (error)
                                return "promptAstValue." + error;
                        }
                    }
                    if (message.uint8arrayValue != null && message.hasOwnProperty("uint8arrayValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        if (!(message.uint8arrayValue && typeof message.uint8arrayValue.length === "number" || $util.isString(message.uint8arrayValue)))
                            return "uint8arrayValue: buffer expected";
                    }
                    return null;
                };

                /**
                 * Creates a BamlOutboundValue message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlOutboundValue} BamlOutboundValue
                 */
                BamlOutboundValue.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlOutboundValue)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlOutboundValue();
                    if (object.nullValue != null) {
                        if (typeof object.nullValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlOutboundValue.nullValue: object expected");
                        message.nullValue = $root.baml_core.cffi.v1.BamlValueNull.fromObject(object.nullValue);
                    }
                    if (object.stringValue != null)
                        message.stringValue = String(object.stringValue);
                    if (object.intValue != null)
                        if ($util.Long)
                            (message.intValue = $util.Long.fromValue(object.intValue)).unsigned = false;
                        else if (typeof object.intValue === "string")
                            message.intValue = parseInt(object.intValue, 10);
                        else if (typeof object.intValue === "number")
                            message.intValue = object.intValue;
                        else if (typeof object.intValue === "object")
                            message.intValue = new $util.LongBits(object.intValue.low >>> 0, object.intValue.high >>> 0).toNumber();
                    if (object.floatValue != null)
                        message.floatValue = Number(object.floatValue);
                    if (object.boolValue != null)
                        message.boolValue = Boolean(object.boolValue);
                    if (object.classValue != null) {
                        if (typeof object.classValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlOutboundValue.classValue: object expected");
                        message.classValue = $root.baml_core.cffi.v1.BamlValueClass.fromObject(object.classValue);
                    }
                    if (object.enumValue != null) {
                        if (typeof object.enumValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlOutboundValue.enumValue: object expected");
                        message.enumValue = $root.baml_core.cffi.v1.BamlValueEnum.fromObject(object.enumValue);
                    }
                    if (object.literalValue != null) {
                        if (typeof object.literalValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlOutboundValue.literalValue: object expected");
                        message.literalValue = $root.baml_core.cffi.v1.BamlTyLiteral.fromObject(object.literalValue);
                    }
                    if (object.listValue != null) {
                        if (typeof object.listValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlOutboundValue.listValue: object expected");
                        message.listValue = $root.baml_core.cffi.v1.BamlValueList.fromObject(object.listValue);
                    }
                    if (object.mapValue != null) {
                        if (typeof object.mapValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlOutboundValue.mapValue: object expected");
                        message.mapValue = $root.baml_core.cffi.v1.BamlValueMap.fromObject(object.mapValue);
                    }
                    if (object.unionVariantValue != null) {
                        if (typeof object.unionVariantValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlOutboundValue.unionVariantValue: object expected");
                        message.unionVariantValue = $root.baml_core.cffi.v1.BamlValueUnionVariant.fromObject(object.unionVariantValue);
                    }
                    if (object.handleValue != null) {
                        if (typeof object.handleValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlOutboundValue.handleValue: object expected");
                        message.handleValue = $root.baml_core.cffi.v1.BamlOutboundHandle.fromObject(object.handleValue);
                    }
                    if (object.mediaValue != null) {
                        if (typeof object.mediaValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlOutboundValue.mediaValue: object expected");
                        message.mediaValue = $root.baml_core.cffi.v1.BamlValueMedia.fromObject(object.mediaValue);
                    }
                    if (object.promptAstValue != null) {
                        if (typeof object.promptAstValue !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlOutboundValue.promptAstValue: object expected");
                        message.promptAstValue = $root.baml_core.cffi.v1.BamlValuePromptAst.fromObject(object.promptAstValue);
                    }
                    if (object.uint8arrayValue != null)
                        if (typeof object.uint8arrayValue === "string")
                            $util.base64.decode(object.uint8arrayValue, message.uint8arrayValue = $util.newBuffer($util.base64.length(object.uint8arrayValue)), 0);
                        else if (object.uint8arrayValue.length >= 0)
                            message.uint8arrayValue = object.uint8arrayValue;
                    return message;
                };

                /**
                 * Creates a plain object from a BamlOutboundValue message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {baml_core.cffi.v1.BamlOutboundValue} message BamlOutboundValue
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlOutboundValue.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (message.nullValue != null && message.hasOwnProperty("nullValue")) {
                        object.nullValue = $root.baml_core.cffi.v1.BamlValueNull.toObject(message.nullValue, options);
                        if (options.oneofs)
                            object.value = "nullValue";
                    }
                    if (message.stringValue != null && message.hasOwnProperty("stringValue")) {
                        object.stringValue = message.stringValue;
                        if (options.oneofs)
                            object.value = "stringValue";
                    }
                    if (message.intValue != null && message.hasOwnProperty("intValue")) {
                        if (typeof message.intValue === "number")
                            object.intValue = options.longs === String ? String(message.intValue) : message.intValue;
                        else
                            object.intValue = options.longs === String ? $util.Long.prototype.toString.call(message.intValue) : options.longs === Number ? new $util.LongBits(message.intValue.low >>> 0, message.intValue.high >>> 0).toNumber() : message.intValue;
                        if (options.oneofs)
                            object.value = "intValue";
                    }
                    if (message.floatValue != null && message.hasOwnProperty("floatValue")) {
                        object.floatValue = options.json && !isFinite(message.floatValue) ? String(message.floatValue) : message.floatValue;
                        if (options.oneofs)
                            object.value = "floatValue";
                    }
                    if (message.boolValue != null && message.hasOwnProperty("boolValue")) {
                        object.boolValue = message.boolValue;
                        if (options.oneofs)
                            object.value = "boolValue";
                    }
                    if (message.classValue != null && message.hasOwnProperty("classValue")) {
                        object.classValue = $root.baml_core.cffi.v1.BamlValueClass.toObject(message.classValue, options);
                        if (options.oneofs)
                            object.value = "classValue";
                    }
                    if (message.enumValue != null && message.hasOwnProperty("enumValue")) {
                        object.enumValue = $root.baml_core.cffi.v1.BamlValueEnum.toObject(message.enumValue, options);
                        if (options.oneofs)
                            object.value = "enumValue";
                    }
                    if (message.literalValue != null && message.hasOwnProperty("literalValue")) {
                        object.literalValue = $root.baml_core.cffi.v1.BamlTyLiteral.toObject(message.literalValue, options);
                        if (options.oneofs)
                            object.value = "literalValue";
                    }
                    if (message.listValue != null && message.hasOwnProperty("listValue")) {
                        object.listValue = $root.baml_core.cffi.v1.BamlValueList.toObject(message.listValue, options);
                        if (options.oneofs)
                            object.value = "listValue";
                    }
                    if (message.mapValue != null && message.hasOwnProperty("mapValue")) {
                        object.mapValue = $root.baml_core.cffi.v1.BamlValueMap.toObject(message.mapValue, options);
                        if (options.oneofs)
                            object.value = "mapValue";
                    }
                    if (message.unionVariantValue != null && message.hasOwnProperty("unionVariantValue")) {
                        object.unionVariantValue = $root.baml_core.cffi.v1.BamlValueUnionVariant.toObject(message.unionVariantValue, options);
                        if (options.oneofs)
                            object.value = "unionVariantValue";
                    }
                    if (message.handleValue != null && message.hasOwnProperty("handleValue")) {
                        object.handleValue = $root.baml_core.cffi.v1.BamlOutboundHandle.toObject(message.handleValue, options);
                        if (options.oneofs)
                            object.value = "handleValue";
                    }
                    if (message.mediaValue != null && message.hasOwnProperty("mediaValue")) {
                        object.mediaValue = $root.baml_core.cffi.v1.BamlValueMedia.toObject(message.mediaValue, options);
                        if (options.oneofs)
                            object.value = "mediaValue";
                    }
                    if (message.promptAstValue != null && message.hasOwnProperty("promptAstValue")) {
                        object.promptAstValue = $root.baml_core.cffi.v1.BamlValuePromptAst.toObject(message.promptAstValue, options);
                        if (options.oneofs)
                            object.value = "promptAstValue";
                    }
                    if (message.uint8arrayValue != null && message.hasOwnProperty("uint8arrayValue")) {
                        object.uint8arrayValue = options.bytes === String ? $util.base64.encode(message.uint8arrayValue, 0, message.uint8arrayValue.length) : options.bytes === Array ? Array.prototype.slice.call(message.uint8arrayValue) : message.uint8arrayValue;
                        if (options.oneofs)
                            object.value = "uint8arrayValue";
                    }
                    return object;
                };

                /**
                 * Converts this BamlOutboundValue to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlOutboundValue.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlOutboundValue
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlOutboundValue.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlOutboundValue";
                };

                return BamlOutboundValue;
            })();

            v1.BamlOutboundHandle = (function() {

                /**
                 * Properties of a BamlOutboundHandle.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlOutboundHandle
                 * @property {number|Long|null} [key] BamlOutboundHandle key
                 * @property {baml_core.cffi.v1.BamlHandleType|null} [handleType] BamlOutboundHandle handleType
                 * @property {baml_core.cffi.v1.IBamlTyName|null} [name] BamlOutboundHandle name
                 */

                /**
                 * Constructs a new BamlOutboundHandle.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlOutboundHandle.
                 * @implements IBamlOutboundHandle
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlOutboundHandle=} [properties] Properties to set
                 */
                function BamlOutboundHandle(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlOutboundHandle key.
                 * @member {number|Long} key
                 * @memberof baml_core.cffi.v1.BamlOutboundHandle
                 * @instance
                 */
                BamlOutboundHandle.prototype.key = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * BamlOutboundHandle handleType.
                 * @member {baml_core.cffi.v1.BamlHandleType} handleType
                 * @memberof baml_core.cffi.v1.BamlOutboundHandle
                 * @instance
                 */
                BamlOutboundHandle.prototype.handleType = 0;

                /**
                 * BamlOutboundHandle name.
                 * @member {baml_core.cffi.v1.IBamlTyName|null|undefined} name
                 * @memberof baml_core.cffi.v1.BamlOutboundHandle
                 * @instance
                 */
                BamlOutboundHandle.prototype.name = null;

                /**
                 * Creates a new BamlOutboundHandle instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlOutboundHandle
                 * @static
                 * @param {baml_core.cffi.v1.IBamlOutboundHandle=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlOutboundHandle} BamlOutboundHandle instance
                 */
                BamlOutboundHandle.create = function create(properties) {
                    return new BamlOutboundHandle(properties);
                };

                /**
                 * Encodes the specified BamlOutboundHandle message. Does not implicitly {@link baml_core.cffi.v1.BamlOutboundHandle.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlOutboundHandle
                 * @static
                 * @param {baml_core.cffi.v1.IBamlOutboundHandle} message BamlOutboundHandle message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlOutboundHandle.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.key != null && Object.hasOwnProperty.call(message, "key"))
                        writer.uint32(/* id 1, wireType 0 =*/8).uint64(message.key);
                    if (message.handleType != null && Object.hasOwnProperty.call(message, "handleType"))
                        writer.uint32(/* id 2, wireType 0 =*/16).int32(message.handleType);
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml_core.cffi.v1.BamlTyName.encode(message.name, writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlOutboundHandle message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlOutboundHandle.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlOutboundHandle
                 * @static
                 * @param {baml_core.cffi.v1.IBamlOutboundHandle} message BamlOutboundHandle message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlOutboundHandle.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlOutboundHandle message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlOutboundHandle
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlOutboundHandle} BamlOutboundHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlOutboundHandle.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlOutboundHandle();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.key = reader.uint64();
                                break;
                            }
                        case 2: {
                                message.handleType = reader.int32();
                                break;
                            }
                        case 3: {
                                message.name = $root.baml_core.cffi.v1.BamlTyName.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlOutboundHandle message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlOutboundHandle
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlOutboundHandle} BamlOutboundHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlOutboundHandle.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlOutboundHandle message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlOutboundHandle
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlOutboundHandle.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.key != null && message.hasOwnProperty("key"))
                        if (!$util.isInteger(message.key) && !(message.key && $util.isInteger(message.key.low) && $util.isInteger(message.key.high)))
                            return "key: integer|Long expected";
                    if (message.handleType != null && message.hasOwnProperty("handleType"))
                        switch (message.handleType) {
                        default:
                            return "handleType: enum value expected";
                        case 0:
                        case 1:
                        case 2:
                        case 5:
                        case 6:
                        case 7:
                        case 8:
                        case 9:
                        case 10:
                        case 11:
                        case 12:
                        case 13:
                        case 14:
                            break;
                        }
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml_core.cffi.v1.BamlTyName.verify(message.name);
                        if (error)
                            return "name." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlOutboundHandle message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlOutboundHandle
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlOutboundHandle} BamlOutboundHandle
                 */
                BamlOutboundHandle.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlOutboundHandle)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlOutboundHandle();
                    if (object.key != null)
                        if ($util.Long)
                            (message.key = $util.Long.fromValue(object.key)).unsigned = true;
                        else if (typeof object.key === "string")
                            message.key = parseInt(object.key, 10);
                        else if (typeof object.key === "number")
                            message.key = object.key;
                        else if (typeof object.key === "object")
                            message.key = new $util.LongBits(object.key.low >>> 0, object.key.high >>> 0).toNumber(true);
                    switch (object.handleType) {
                    default:
                        if (typeof object.handleType === "number") {
                            message.handleType = object.handleType;
                            break;
                        }
                        break;
                    case "HANDLE_UNSPECIFIED":
                    case 0:
                        message.handleType = 0;
                        break;
                    case "UNTAGGED_RUST_DATA":
                    case 1:
                        message.handleType = 1;
                        break;
                    case "UNTAGGED_BEX_HEAP":
                    case 2:
                        message.handleType = 2;
                        break;
                    case "FUNCTION_REF":
                    case 5:
                        message.handleType = 5;
                        break;
                    case "ADT_MEDIA_IMAGE":
                    case 6:
                        message.handleType = 6;
                        break;
                    case "ADT_MEDIA_AUDIO":
                    case 7:
                        message.handleType = 7;
                        break;
                    case "ADT_MEDIA_VIDEO":
                    case 8:
                        message.handleType = 8;
                        break;
                    case "ADT_MEDIA_PDF":
                    case 9:
                        message.handleType = 9;
                        break;
                    case "ADT_MEDIA_GENERIC":
                    case 10:
                        message.handleType = 10;
                        break;
                    case "ADT_PROMPT_AST":
                    case 11:
                        message.handleType = 11;
                        break;
                    case "ADT_COLLECTOR":
                    case 12:
                        message.handleType = 12;
                        break;
                    case "ADT_TYPE":
                    case 13:
                        message.handleType = 13;
                        break;
                    case "ADT_TAGGED_HEAP_HANDLE":
                    case 14:
                        message.handleType = 14;
                        break;
                    }
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlOutboundHandle.name: object expected");
                        message.name = $root.baml_core.cffi.v1.BamlTyName.fromObject(object.name);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlOutboundHandle message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlOutboundHandle
                 * @static
                 * @param {baml_core.cffi.v1.BamlOutboundHandle} message BamlOutboundHandle
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlOutboundHandle.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults) {
                        if ($util.Long) {
                            var long = new $util.Long(0, 0, true);
                            object.key = options.longs === String ? long.toString() : options.longs === Number ? long.toNumber() : long;
                        } else
                            object.key = options.longs === String ? "0" : 0;
                        object.handleType = options.enums === String ? "HANDLE_UNSPECIFIED" : 0;
                        object.name = null;
                    }
                    if (message.key != null && message.hasOwnProperty("key"))
                        if (typeof message.key === "number")
                            object.key = options.longs === String ? String(message.key) : message.key;
                        else
                            object.key = options.longs === String ? $util.Long.prototype.toString.call(message.key) : options.longs === Number ? new $util.LongBits(message.key.low >>> 0, message.key.high >>> 0).toNumber(true) : message.key;
                    if (message.handleType != null && message.hasOwnProperty("handleType"))
                        object.handleType = options.enums === String ? $root.baml_core.cffi.v1.BamlHandleType[message.handleType] === undefined ? message.handleType : $root.baml_core.cffi.v1.BamlHandleType[message.handleType] : message.handleType;
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = $root.baml_core.cffi.v1.BamlTyName.toObject(message.name, options);
                    return object;
                };

                /**
                 * Converts this BamlOutboundHandle to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlOutboundHandle
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlOutboundHandle.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlOutboundHandle
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlOutboundHandle
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlOutboundHandle.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlOutboundHandle";
                };

                return BamlOutboundHandle;
            })();

            v1.BamlTyName = (function() {

                /**
                 * Properties of a BamlTyName.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyName
                 * @property {string|null} [name] BamlTyName name
                 * @property {Array.<baml_core.cffi.v1.IBamlTyGenericArg>|null} [genericArgs] BamlTyName genericArgs
                 */

                /**
                 * Constructs a new BamlTyName.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyName.
                 * @implements IBamlTyName
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyName=} [properties] Properties to set
                 */
                function BamlTyName(properties) {
                    this.genericArgs = [];
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlTyName name.
                 * @member {string} name
                 * @memberof baml_core.cffi.v1.BamlTyName
                 * @instance
                 */
                BamlTyName.prototype.name = "";

                /**
                 * BamlTyName genericArgs.
                 * @member {Array.<baml_core.cffi.v1.IBamlTyGenericArg>} genericArgs
                 * @memberof baml_core.cffi.v1.BamlTyName
                 * @instance
                 */
                BamlTyName.prototype.genericArgs = $util.emptyArray;

                /**
                 * Creates a new BamlTyName instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyName
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyName=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyName} BamlTyName instance
                 */
                BamlTyName.create = function create(properties) {
                    return new BamlTyName(properties);
                };

                /**
                 * Encodes the specified BamlTyName message. Does not implicitly {@link baml_core.cffi.v1.BamlTyName.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyName
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyName} message BamlTyName message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyName.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.name);
                    if (message.genericArgs != null && message.genericArgs.length)
                        for (var i = 0; i < message.genericArgs.length; ++i)
                            $root.baml_core.cffi.v1.BamlTyGenericArg.encode(message.genericArgs[i], writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyName message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyName.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyName
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyName} message BamlTyName message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyName.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyName message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyName
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyName} BamlTyName
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyName.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyName();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 2: {
                                message.name = reader.string();
                                break;
                            }
                        case 3: {
                                if (!(message.genericArgs && message.genericArgs.length))
                                    message.genericArgs = [];
                                message.genericArgs.push($root.baml_core.cffi.v1.BamlTyGenericArg.decode(reader, reader.uint32()));
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyName message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyName
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyName} BamlTyName
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyName.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyName message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyName
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyName.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name"))
                        if (!$util.isString(message.name))
                            return "name: string expected";
                    if (message.genericArgs != null && message.hasOwnProperty("genericArgs")) {
                        if (!Array.isArray(message.genericArgs))
                            return "genericArgs: array expected";
                        for (var i = 0; i < message.genericArgs.length; ++i) {
                            var error = $root.baml_core.cffi.v1.BamlTyGenericArg.verify(message.genericArgs[i]);
                            if (error)
                                return "genericArgs." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlTyName message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyName
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyName} BamlTyName
                 */
                BamlTyName.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyName)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlTyName();
                    if (object.name != null)
                        message.name = String(object.name);
                    if (object.genericArgs) {
                        if (!Array.isArray(object.genericArgs))
                            throw TypeError(".baml_core.cffi.v1.BamlTyName.genericArgs: array expected");
                        message.genericArgs = [];
                        for (var i = 0; i < object.genericArgs.length; ++i) {
                            if (typeof object.genericArgs[i] !== "object")
                                throw TypeError(".baml_core.cffi.v1.BamlTyName.genericArgs: object expected");
                            message.genericArgs[i] = $root.baml_core.cffi.v1.BamlTyGenericArg.fromObject(object.genericArgs[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlTyName message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyName
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyName} message BamlTyName
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyName.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.arrays || options.defaults)
                        object.genericArgs = [];
                    if (options.defaults)
                        object.name = "";
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = message.name;
                    if (message.genericArgs && message.genericArgs.length) {
                        object.genericArgs = [];
                        for (var j = 0; j < message.genericArgs.length; ++j)
                            object.genericArgs[j] = $root.baml_core.cffi.v1.BamlTyGenericArg.toObject(message.genericArgs[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this BamlTyName to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyName
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyName.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyName
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyName
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyName.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyName";
                };

                return BamlTyName;
            })();

            v1.BamlTyGenericArg = (function() {

                /**
                 * Properties of a BamlTyGenericArg.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyGenericArg
                 * @property {string|null} [name] BamlTyGenericArg name
                 * @property {baml_core.cffi.v1.IBamlTy|null} [ty] BamlTyGenericArg ty
                 */

                /**
                 * Constructs a new BamlTyGenericArg.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyGenericArg.
                 * @implements IBamlTyGenericArg
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyGenericArg=} [properties] Properties to set
                 */
                function BamlTyGenericArg(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlTyGenericArg name.
                 * @member {string} name
                 * @memberof baml_core.cffi.v1.BamlTyGenericArg
                 * @instance
                 */
                BamlTyGenericArg.prototype.name = "";

                /**
                 * BamlTyGenericArg ty.
                 * @member {baml_core.cffi.v1.IBamlTy|null|undefined} ty
                 * @memberof baml_core.cffi.v1.BamlTyGenericArg
                 * @instance
                 */
                BamlTyGenericArg.prototype.ty = null;

                /**
                 * Creates a new BamlTyGenericArg instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyGenericArg
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyGenericArg=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyGenericArg} BamlTyGenericArg instance
                 */
                BamlTyGenericArg.create = function create(properties) {
                    return new BamlTyGenericArg(properties);
                };

                /**
                 * Encodes the specified BamlTyGenericArg message. Does not implicitly {@link baml_core.cffi.v1.BamlTyGenericArg.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyGenericArg
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyGenericArg} message BamlTyGenericArg message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyGenericArg.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.name);
                    if (message.ty != null && Object.hasOwnProperty.call(message, "ty"))
                        $root.baml_core.cffi.v1.BamlTy.encode(message.ty, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyGenericArg message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyGenericArg.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyGenericArg
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyGenericArg} message BamlTyGenericArg message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyGenericArg.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyGenericArg message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyGenericArg
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyGenericArg} BamlTyGenericArg
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyGenericArg.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyGenericArg();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = reader.string();
                                break;
                            }
                        case 2: {
                                message.ty = $root.baml_core.cffi.v1.BamlTy.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyGenericArg message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyGenericArg
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyGenericArg} BamlTyGenericArg
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyGenericArg.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyGenericArg message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyGenericArg
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyGenericArg.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name"))
                        if (!$util.isString(message.name))
                            return "name: string expected";
                    if (message.ty != null && message.hasOwnProperty("ty")) {
                        var error = $root.baml_core.cffi.v1.BamlTy.verify(message.ty);
                        if (error)
                            return "ty." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlTyGenericArg message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyGenericArg
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyGenericArg} BamlTyGenericArg
                 */
                BamlTyGenericArg.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyGenericArg)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlTyGenericArg();
                    if (object.name != null)
                        message.name = String(object.name);
                    if (object.ty != null) {
                        if (typeof object.ty !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTyGenericArg.ty: object expected");
                        message.ty = $root.baml_core.cffi.v1.BamlTy.fromObject(object.ty);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlTyGenericArg message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyGenericArg
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyGenericArg} message BamlTyGenericArg
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyGenericArg.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults) {
                        object.name = "";
                        object.ty = null;
                    }
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = message.name;
                    if (message.ty != null && message.hasOwnProperty("ty"))
                        object.ty = $root.baml_core.cffi.v1.BamlTy.toObject(message.ty, options);
                    return object;
                };

                /**
                 * Converts this BamlTyGenericArg to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyGenericArg
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyGenericArg.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyGenericArg
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyGenericArg
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyGenericArg.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyGenericArg";
                };

                return BamlTyGenericArg;
            })();

            v1.BamlValueNull = (function() {

                /**
                 * Properties of a BamlValueNull.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlValueNull
                 */

                /**
                 * Constructs a new BamlValueNull.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlValueNull.
                 * @implements IBamlValueNull
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlValueNull=} [properties] Properties to set
                 */
                function BamlValueNull(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlValueNull instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlValueNull
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueNull=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlValueNull} BamlValueNull instance
                 */
                BamlValueNull.create = function create(properties) {
                    return new BamlValueNull(properties);
                };

                /**
                 * Encodes the specified BamlValueNull message. Does not implicitly {@link baml_core.cffi.v1.BamlValueNull.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlValueNull
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueNull} message BamlValueNull message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueNull.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueNull message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueNull.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueNull
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueNull} message BamlValueNull message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueNull.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueNull message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlValueNull
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlValueNull} BamlValueNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueNull.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlValueNull();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlValueNull message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueNull
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlValueNull} BamlValueNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueNull.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValueNull message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlValueNull
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueNull.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlValueNull message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlValueNull
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlValueNull} BamlValueNull
                 */
                BamlValueNull.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlValueNull)
                        return object;
                    return new $root.baml_core.cffi.v1.BamlValueNull();
                };

                /**
                 * Creates a plain object from a BamlValueNull message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlValueNull
                 * @static
                 * @param {baml_core.cffi.v1.BamlValueNull} message BamlValueNull
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValueNull.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlValueNull to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlValueNull
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueNull.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueNull
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlValueNull
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueNull.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlValueNull";
                };

                return BamlValueNull;
            })();

            v1.BamlValueList = (function() {

                /**
                 * Properties of a BamlValueList.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlValueList
                 * @property {baml_core.cffi.v1.IBamlTy|null} [itemType] BamlValueList itemType
                 * @property {Array.<baml_core.cffi.v1.IBamlOutboundValue>|null} [items] BamlValueList items
                 */

                /**
                 * Constructs a new BamlValueList.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlValueList.
                 * @implements IBamlValueList
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlValueList=} [properties] Properties to set
                 */
                function BamlValueList(properties) {
                    this.items = [];
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValueList itemType.
                 * @member {baml_core.cffi.v1.IBamlTy|null|undefined} itemType
                 * @memberof baml_core.cffi.v1.BamlValueList
                 * @instance
                 */
                BamlValueList.prototype.itemType = null;

                /**
                 * BamlValueList items.
                 * @member {Array.<baml_core.cffi.v1.IBamlOutboundValue>} items
                 * @memberof baml_core.cffi.v1.BamlValueList
                 * @instance
                 */
                BamlValueList.prototype.items = $util.emptyArray;

                /**
                 * Creates a new BamlValueList instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlValueList
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueList=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlValueList} BamlValueList instance
                 */
                BamlValueList.create = function create(properties) {
                    return new BamlValueList(properties);
                };

                /**
                 * Encodes the specified BamlValueList message. Does not implicitly {@link baml_core.cffi.v1.BamlValueList.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlValueList
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueList} message BamlValueList message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueList.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.itemType != null && Object.hasOwnProperty.call(message, "itemType"))
                        $root.baml_core.cffi.v1.BamlTy.encode(message.itemType, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.items != null && message.items.length)
                        for (var i = 0; i < message.items.length; ++i)
                            $root.baml_core.cffi.v1.BamlOutboundValue.encode(message.items[i], writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueList message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueList.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueList
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueList} message BamlValueList message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueList.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueList message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlValueList
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlValueList} BamlValueList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueList.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlValueList();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.itemType = $root.baml_core.cffi.v1.BamlTy.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                if (!(message.items && message.items.length))
                                    message.items = [];
                                message.items.push($root.baml_core.cffi.v1.BamlOutboundValue.decode(reader, reader.uint32()));
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlValueList message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueList
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlValueList} BamlValueList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueList.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValueList message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlValueList
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueList.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.itemType != null && message.hasOwnProperty("itemType")) {
                        var error = $root.baml_core.cffi.v1.BamlTy.verify(message.itemType);
                        if (error)
                            return "itemType." + error;
                    }
                    if (message.items != null && message.hasOwnProperty("items")) {
                        if (!Array.isArray(message.items))
                            return "items: array expected";
                        for (var i = 0; i < message.items.length; ++i) {
                            var error = $root.baml_core.cffi.v1.BamlOutboundValue.verify(message.items[i]);
                            if (error)
                                return "items." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValueList message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlValueList
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlValueList} BamlValueList
                 */
                BamlValueList.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlValueList)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlValueList();
                    if (object.itemType != null) {
                        if (typeof object.itemType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValueList.itemType: object expected");
                        message.itemType = $root.baml_core.cffi.v1.BamlTy.fromObject(object.itemType);
                    }
                    if (object.items) {
                        if (!Array.isArray(object.items))
                            throw TypeError(".baml_core.cffi.v1.BamlValueList.items: array expected");
                        message.items = [];
                        for (var i = 0; i < object.items.length; ++i) {
                            if (typeof object.items[i] !== "object")
                                throw TypeError(".baml_core.cffi.v1.BamlValueList.items: object expected");
                            message.items[i] = $root.baml_core.cffi.v1.BamlOutboundValue.fromObject(object.items[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValueList message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlValueList
                 * @static
                 * @param {baml_core.cffi.v1.BamlValueList} message BamlValueList
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValueList.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.arrays || options.defaults)
                        object.items = [];
                    if (options.defaults)
                        object.itemType = null;
                    if (message.itemType != null && message.hasOwnProperty("itemType"))
                        object.itemType = $root.baml_core.cffi.v1.BamlTy.toObject(message.itemType, options);
                    if (message.items && message.items.length) {
                        object.items = [];
                        for (var j = 0; j < message.items.length; ++j)
                            object.items[j] = $root.baml_core.cffi.v1.BamlOutboundValue.toObject(message.items[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this BamlValueList to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlValueList
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueList.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueList
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlValueList
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueList.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlValueList";
                };

                return BamlValueList;
            })();

            v1.BamlOutboundMapEntry = (function() {

                /**
                 * Properties of a BamlOutboundMapEntry.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlOutboundMapEntry
                 * @property {string|null} [key] BamlOutboundMapEntry key
                 * @property {baml_core.cffi.v1.IBamlOutboundValue|null} [value] BamlOutboundMapEntry value
                 */

                /**
                 * Constructs a new BamlOutboundMapEntry.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlOutboundMapEntry.
                 * @implements IBamlOutboundMapEntry
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlOutboundMapEntry=} [properties] Properties to set
                 */
                function BamlOutboundMapEntry(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlOutboundMapEntry key.
                 * @member {string} key
                 * @memberof baml_core.cffi.v1.BamlOutboundMapEntry
                 * @instance
                 */
                BamlOutboundMapEntry.prototype.key = "";

                /**
                 * BamlOutboundMapEntry value.
                 * @member {baml_core.cffi.v1.IBamlOutboundValue|null|undefined} value
                 * @memberof baml_core.cffi.v1.BamlOutboundMapEntry
                 * @instance
                 */
                BamlOutboundMapEntry.prototype.value = null;

                /**
                 * Creates a new BamlOutboundMapEntry instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {baml_core.cffi.v1.IBamlOutboundMapEntry=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlOutboundMapEntry} BamlOutboundMapEntry instance
                 */
                BamlOutboundMapEntry.create = function create(properties) {
                    return new BamlOutboundMapEntry(properties);
                };

                /**
                 * Encodes the specified BamlOutboundMapEntry message. Does not implicitly {@link baml_core.cffi.v1.BamlOutboundMapEntry.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {baml_core.cffi.v1.IBamlOutboundMapEntry} message BamlOutboundMapEntry message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlOutboundMapEntry.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.key != null && Object.hasOwnProperty.call(message, "key"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.key);
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        $root.baml_core.cffi.v1.BamlOutboundValue.encode(message.value, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlOutboundMapEntry message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlOutboundMapEntry.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {baml_core.cffi.v1.IBamlOutboundMapEntry} message BamlOutboundMapEntry message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlOutboundMapEntry.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlOutboundMapEntry message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlOutboundMapEntry} BamlOutboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlOutboundMapEntry.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlOutboundMapEntry();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.key = reader.string();
                                break;
                            }
                        case 2: {
                                message.value = $root.baml_core.cffi.v1.BamlOutboundValue.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlOutboundMapEntry message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlOutboundMapEntry} BamlOutboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlOutboundMapEntry.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlOutboundMapEntry message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlOutboundMapEntry.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.key != null && message.hasOwnProperty("key"))
                        if (!$util.isString(message.key))
                            return "key: string expected";
                    if (message.value != null && message.hasOwnProperty("value")) {
                        var error = $root.baml_core.cffi.v1.BamlOutboundValue.verify(message.value);
                        if (error)
                            return "value." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlOutboundMapEntry message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlOutboundMapEntry} BamlOutboundMapEntry
                 */
                BamlOutboundMapEntry.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlOutboundMapEntry)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlOutboundMapEntry();
                    if (object.key != null)
                        message.key = String(object.key);
                    if (object.value != null) {
                        if (typeof object.value !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlOutboundMapEntry.value: object expected");
                        message.value = $root.baml_core.cffi.v1.BamlOutboundValue.fromObject(object.value);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlOutboundMapEntry message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {baml_core.cffi.v1.BamlOutboundMapEntry} message BamlOutboundMapEntry
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlOutboundMapEntry.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults) {
                        object.key = "";
                        object.value = null;
                    }
                    if (message.key != null && message.hasOwnProperty("key"))
                        object.key = message.key;
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = $root.baml_core.cffi.v1.BamlOutboundValue.toObject(message.value, options);
                    return object;
                };

                /**
                 * Converts this BamlOutboundMapEntry to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlOutboundMapEntry
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlOutboundMapEntry.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlOutboundMapEntry
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlOutboundMapEntry.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlOutboundMapEntry";
                };

                return BamlOutboundMapEntry;
            })();

            v1.BamlValueMap = (function() {

                /**
                 * Properties of a BamlValueMap.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlValueMap
                 * @property {baml_core.cffi.v1.IBamlTy|null} [keyType] BamlValueMap keyType
                 * @property {baml_core.cffi.v1.IBamlTy|null} [valueType] BamlValueMap valueType
                 * @property {Array.<baml_core.cffi.v1.IBamlOutboundMapEntry>|null} [entries] BamlValueMap entries
                 */

                /**
                 * Constructs a new BamlValueMap.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlValueMap.
                 * @implements IBamlValueMap
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlValueMap=} [properties] Properties to set
                 */
                function BamlValueMap(properties) {
                    this.entries = [];
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValueMap keyType.
                 * @member {baml_core.cffi.v1.IBamlTy|null|undefined} keyType
                 * @memberof baml_core.cffi.v1.BamlValueMap
                 * @instance
                 */
                BamlValueMap.prototype.keyType = null;

                /**
                 * BamlValueMap valueType.
                 * @member {baml_core.cffi.v1.IBamlTy|null|undefined} valueType
                 * @memberof baml_core.cffi.v1.BamlValueMap
                 * @instance
                 */
                BamlValueMap.prototype.valueType = null;

                /**
                 * BamlValueMap entries.
                 * @member {Array.<baml_core.cffi.v1.IBamlOutboundMapEntry>} entries
                 * @memberof baml_core.cffi.v1.BamlValueMap
                 * @instance
                 */
                BamlValueMap.prototype.entries = $util.emptyArray;

                /**
                 * Creates a new BamlValueMap instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlValueMap
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueMap=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlValueMap} BamlValueMap instance
                 */
                BamlValueMap.create = function create(properties) {
                    return new BamlValueMap(properties);
                };

                /**
                 * Encodes the specified BamlValueMap message. Does not implicitly {@link baml_core.cffi.v1.BamlValueMap.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlValueMap
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueMap} message BamlValueMap message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueMap.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.keyType != null && Object.hasOwnProperty.call(message, "keyType"))
                        $root.baml_core.cffi.v1.BamlTy.encode(message.keyType, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.valueType != null && Object.hasOwnProperty.call(message, "valueType"))
                        $root.baml_core.cffi.v1.BamlTy.encode(message.valueType, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.entries != null && message.entries.length)
                        for (var i = 0; i < message.entries.length; ++i)
                            $root.baml_core.cffi.v1.BamlOutboundMapEntry.encode(message.entries[i], writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueMap message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueMap.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueMap
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueMap} message BamlValueMap message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueMap.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueMap message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlValueMap
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlValueMap} BamlValueMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueMap.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlValueMap();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.keyType = $root.baml_core.cffi.v1.BamlTy.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.valueType = $root.baml_core.cffi.v1.BamlTy.decode(reader, reader.uint32());
                                break;
                            }
                        case 3: {
                                if (!(message.entries && message.entries.length))
                                    message.entries = [];
                                message.entries.push($root.baml_core.cffi.v1.BamlOutboundMapEntry.decode(reader, reader.uint32()));
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlValueMap message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueMap
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlValueMap} BamlValueMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueMap.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValueMap message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlValueMap
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueMap.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.keyType != null && message.hasOwnProperty("keyType")) {
                        var error = $root.baml_core.cffi.v1.BamlTy.verify(message.keyType);
                        if (error)
                            return "keyType." + error;
                    }
                    if (message.valueType != null && message.hasOwnProperty("valueType")) {
                        var error = $root.baml_core.cffi.v1.BamlTy.verify(message.valueType);
                        if (error)
                            return "valueType." + error;
                    }
                    if (message.entries != null && message.hasOwnProperty("entries")) {
                        if (!Array.isArray(message.entries))
                            return "entries: array expected";
                        for (var i = 0; i < message.entries.length; ++i) {
                            var error = $root.baml_core.cffi.v1.BamlOutboundMapEntry.verify(message.entries[i]);
                            if (error)
                                return "entries." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValueMap message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlValueMap
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlValueMap} BamlValueMap
                 */
                BamlValueMap.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlValueMap)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlValueMap();
                    if (object.keyType != null) {
                        if (typeof object.keyType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValueMap.keyType: object expected");
                        message.keyType = $root.baml_core.cffi.v1.BamlTy.fromObject(object.keyType);
                    }
                    if (object.valueType != null) {
                        if (typeof object.valueType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValueMap.valueType: object expected");
                        message.valueType = $root.baml_core.cffi.v1.BamlTy.fromObject(object.valueType);
                    }
                    if (object.entries) {
                        if (!Array.isArray(object.entries))
                            throw TypeError(".baml_core.cffi.v1.BamlValueMap.entries: array expected");
                        message.entries = [];
                        for (var i = 0; i < object.entries.length; ++i) {
                            if (typeof object.entries[i] !== "object")
                                throw TypeError(".baml_core.cffi.v1.BamlValueMap.entries: object expected");
                            message.entries[i] = $root.baml_core.cffi.v1.BamlOutboundMapEntry.fromObject(object.entries[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValueMap message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlValueMap
                 * @static
                 * @param {baml_core.cffi.v1.BamlValueMap} message BamlValueMap
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValueMap.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.arrays || options.defaults)
                        object.entries = [];
                    if (options.defaults) {
                        object.keyType = null;
                        object.valueType = null;
                    }
                    if (message.keyType != null && message.hasOwnProperty("keyType"))
                        object.keyType = $root.baml_core.cffi.v1.BamlTy.toObject(message.keyType, options);
                    if (message.valueType != null && message.hasOwnProperty("valueType"))
                        object.valueType = $root.baml_core.cffi.v1.BamlTy.toObject(message.valueType, options);
                    if (message.entries && message.entries.length) {
                        object.entries = [];
                        for (var j = 0; j < message.entries.length; ++j)
                            object.entries[j] = $root.baml_core.cffi.v1.BamlOutboundMapEntry.toObject(message.entries[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this BamlValueMap to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlValueMap
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueMap.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueMap
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlValueMap
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueMap.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlValueMap";
                };

                return BamlValueMap;
            })();

            v1.BamlValueClass = (function() {

                /**
                 * Properties of a BamlValueClass.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlValueClass
                 * @property {baml_core.cffi.v1.IBamlTyName|null} [name] BamlValueClass name
                 * @property {Array.<baml_core.cffi.v1.IBamlOutboundMapEntry>|null} [fields] BamlValueClass fields
                 */

                /**
                 * Constructs a new BamlValueClass.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlValueClass.
                 * @implements IBamlValueClass
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlValueClass=} [properties] Properties to set
                 */
                function BamlValueClass(properties) {
                    this.fields = [];
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValueClass name.
                 * @member {baml_core.cffi.v1.IBamlTyName|null|undefined} name
                 * @memberof baml_core.cffi.v1.BamlValueClass
                 * @instance
                 */
                BamlValueClass.prototype.name = null;

                /**
                 * BamlValueClass fields.
                 * @member {Array.<baml_core.cffi.v1.IBamlOutboundMapEntry>} fields
                 * @memberof baml_core.cffi.v1.BamlValueClass
                 * @instance
                 */
                BamlValueClass.prototype.fields = $util.emptyArray;

                /**
                 * Creates a new BamlValueClass instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlValueClass
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueClass=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlValueClass} BamlValueClass instance
                 */
                BamlValueClass.create = function create(properties) {
                    return new BamlValueClass(properties);
                };

                /**
                 * Encodes the specified BamlValueClass message. Does not implicitly {@link baml_core.cffi.v1.BamlValueClass.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlValueClass
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueClass} message BamlValueClass message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueClass.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml_core.cffi.v1.BamlTyName.encode(message.name, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.fields != null && message.fields.length)
                        for (var i = 0; i < message.fields.length; ++i)
                            $root.baml_core.cffi.v1.BamlOutboundMapEntry.encode(message.fields[i], writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueClass message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueClass.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueClass
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueClass} message BamlValueClass message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueClass.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueClass message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlValueClass
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlValueClass} BamlValueClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueClass.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlValueClass();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = $root.baml_core.cffi.v1.BamlTyName.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                if (!(message.fields && message.fields.length))
                                    message.fields = [];
                                message.fields.push($root.baml_core.cffi.v1.BamlOutboundMapEntry.decode(reader, reader.uint32()));
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlValueClass message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueClass
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlValueClass} BamlValueClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueClass.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValueClass message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlValueClass
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueClass.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml_core.cffi.v1.BamlTyName.verify(message.name);
                        if (error)
                            return "name." + error;
                    }
                    if (message.fields != null && message.hasOwnProperty("fields")) {
                        if (!Array.isArray(message.fields))
                            return "fields: array expected";
                        for (var i = 0; i < message.fields.length; ++i) {
                            var error = $root.baml_core.cffi.v1.BamlOutboundMapEntry.verify(message.fields[i]);
                            if (error)
                                return "fields." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValueClass message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlValueClass
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlValueClass} BamlValueClass
                 */
                BamlValueClass.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlValueClass)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlValueClass();
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValueClass.name: object expected");
                        message.name = $root.baml_core.cffi.v1.BamlTyName.fromObject(object.name);
                    }
                    if (object.fields) {
                        if (!Array.isArray(object.fields))
                            throw TypeError(".baml_core.cffi.v1.BamlValueClass.fields: array expected");
                        message.fields = [];
                        for (var i = 0; i < object.fields.length; ++i) {
                            if (typeof object.fields[i] !== "object")
                                throw TypeError(".baml_core.cffi.v1.BamlValueClass.fields: object expected");
                            message.fields[i] = $root.baml_core.cffi.v1.BamlOutboundMapEntry.fromObject(object.fields[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValueClass message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlValueClass
                 * @static
                 * @param {baml_core.cffi.v1.BamlValueClass} message BamlValueClass
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValueClass.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.arrays || options.defaults)
                        object.fields = [];
                    if (options.defaults)
                        object.name = null;
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = $root.baml_core.cffi.v1.BamlTyName.toObject(message.name, options);
                    if (message.fields && message.fields.length) {
                        object.fields = [];
                        for (var j = 0; j < message.fields.length; ++j)
                            object.fields[j] = $root.baml_core.cffi.v1.BamlOutboundMapEntry.toObject(message.fields[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this BamlValueClass to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlValueClass
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueClass.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueClass
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlValueClass
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueClass.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlValueClass";
                };

                return BamlValueClass;
            })();

            v1.BamlValueEnum = (function() {

                /**
                 * Properties of a BamlValueEnum.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlValueEnum
                 * @property {baml_core.cffi.v1.IBamlTyName|null} [name] BamlValueEnum name
                 * @property {string|null} [value] BamlValueEnum value
                 * @property {boolean|null} [isDynamic] BamlValueEnum isDynamic
                 */

                /**
                 * Constructs a new BamlValueEnum.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlValueEnum.
                 * @implements IBamlValueEnum
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlValueEnum=} [properties] Properties to set
                 */
                function BamlValueEnum(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValueEnum name.
                 * @member {baml_core.cffi.v1.IBamlTyName|null|undefined} name
                 * @memberof baml_core.cffi.v1.BamlValueEnum
                 * @instance
                 */
                BamlValueEnum.prototype.name = null;

                /**
                 * BamlValueEnum value.
                 * @member {string} value
                 * @memberof baml_core.cffi.v1.BamlValueEnum
                 * @instance
                 */
                BamlValueEnum.prototype.value = "";

                /**
                 * BamlValueEnum isDynamic.
                 * @member {boolean} isDynamic
                 * @memberof baml_core.cffi.v1.BamlValueEnum
                 * @instance
                 */
                BamlValueEnum.prototype.isDynamic = false;

                /**
                 * Creates a new BamlValueEnum instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlValueEnum
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueEnum=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlValueEnum} BamlValueEnum instance
                 */
                BamlValueEnum.create = function create(properties) {
                    return new BamlValueEnum(properties);
                };

                /**
                 * Encodes the specified BamlValueEnum message. Does not implicitly {@link baml_core.cffi.v1.BamlValueEnum.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlValueEnum
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueEnum} message BamlValueEnum message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueEnum.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml_core.cffi.v1.BamlTyName.encode(message.name, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.value);
                    if (message.isDynamic != null && Object.hasOwnProperty.call(message, "isDynamic"))
                        writer.uint32(/* id 3, wireType 0 =*/24).bool(message.isDynamic);
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueEnum message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueEnum.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueEnum
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueEnum} message BamlValueEnum message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueEnum.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueEnum message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlValueEnum
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlValueEnum} BamlValueEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueEnum.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlValueEnum();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = $root.baml_core.cffi.v1.BamlTyName.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.value = reader.string();
                                break;
                            }
                        case 3: {
                                message.isDynamic = reader.bool();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlValueEnum message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueEnum
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlValueEnum} BamlValueEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueEnum.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValueEnum message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlValueEnum
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueEnum.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml_core.cffi.v1.BamlTyName.verify(message.name);
                        if (error)
                            return "name." + error;
                    }
                    if (message.value != null && message.hasOwnProperty("value"))
                        if (!$util.isString(message.value))
                            return "value: string expected";
                    if (message.isDynamic != null && message.hasOwnProperty("isDynamic"))
                        if (typeof message.isDynamic !== "boolean")
                            return "isDynamic: boolean expected";
                    return null;
                };

                /**
                 * Creates a BamlValueEnum message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlValueEnum
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlValueEnum} BamlValueEnum
                 */
                BamlValueEnum.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlValueEnum)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlValueEnum();
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValueEnum.name: object expected");
                        message.name = $root.baml_core.cffi.v1.BamlTyName.fromObject(object.name);
                    }
                    if (object.value != null)
                        message.value = String(object.value);
                    if (object.isDynamic != null)
                        message.isDynamic = Boolean(object.isDynamic);
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValueEnum message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlValueEnum
                 * @static
                 * @param {baml_core.cffi.v1.BamlValueEnum} message BamlValueEnum
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValueEnum.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults) {
                        object.name = null;
                        object.value = "";
                        object.isDynamic = false;
                    }
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = $root.baml_core.cffi.v1.BamlTyName.toObject(message.name, options);
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = message.value;
                    if (message.isDynamic != null && message.hasOwnProperty("isDynamic"))
                        object.isDynamic = message.isDynamic;
                    return object;
                };

                /**
                 * Converts this BamlValueEnum to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlValueEnum
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueEnum.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueEnum
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlValueEnum
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueEnum.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlValueEnum";
                };

                return BamlValueEnum;
            })();

            v1.BamlValueUnionVariant = (function() {

                /**
                 * Properties of a BamlValueUnionVariant.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlValueUnionVariant
                 * @property {baml_core.cffi.v1.IBamlTyName|null} [name] BamlValueUnionVariant name
                 * @property {boolean|null} [isOptional] BamlValueUnionVariant isOptional
                 * @property {boolean|null} [isSinglePattern] BamlValueUnionVariant isSinglePattern
                 * @property {baml_core.cffi.v1.IBamlTy|null} [selfType] BamlValueUnionVariant selfType
                 * @property {string|null} [valueOptionName] BamlValueUnionVariant valueOptionName
                 * @property {baml_core.cffi.v1.IBamlOutboundValue|null} [value] BamlValueUnionVariant value
                 */

                /**
                 * Constructs a new BamlValueUnionVariant.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlValueUnionVariant.
                 * @implements IBamlValueUnionVariant
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlValueUnionVariant=} [properties] Properties to set
                 */
                function BamlValueUnionVariant(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValueUnionVariant name.
                 * @member {baml_core.cffi.v1.IBamlTyName|null|undefined} name
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @instance
                 */
                BamlValueUnionVariant.prototype.name = null;

                /**
                 * BamlValueUnionVariant isOptional.
                 * @member {boolean} isOptional
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @instance
                 */
                BamlValueUnionVariant.prototype.isOptional = false;

                /**
                 * BamlValueUnionVariant isSinglePattern.
                 * @member {boolean} isSinglePattern
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @instance
                 */
                BamlValueUnionVariant.prototype.isSinglePattern = false;

                /**
                 * BamlValueUnionVariant selfType.
                 * @member {baml_core.cffi.v1.IBamlTy|null|undefined} selfType
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @instance
                 */
                BamlValueUnionVariant.prototype.selfType = null;

                /**
                 * BamlValueUnionVariant valueOptionName.
                 * @member {string} valueOptionName
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @instance
                 */
                BamlValueUnionVariant.prototype.valueOptionName = "";

                /**
                 * BamlValueUnionVariant value.
                 * @member {baml_core.cffi.v1.IBamlOutboundValue|null|undefined} value
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @instance
                 */
                BamlValueUnionVariant.prototype.value = null;

                /**
                 * Creates a new BamlValueUnionVariant instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueUnionVariant=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlValueUnionVariant} BamlValueUnionVariant instance
                 */
                BamlValueUnionVariant.create = function create(properties) {
                    return new BamlValueUnionVariant(properties);
                };

                /**
                 * Encodes the specified BamlValueUnionVariant message. Does not implicitly {@link baml_core.cffi.v1.BamlValueUnionVariant.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueUnionVariant} message BamlValueUnionVariant message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueUnionVariant.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml_core.cffi.v1.BamlTyName.encode(message.name, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.isOptional != null && Object.hasOwnProperty.call(message, "isOptional"))
                        writer.uint32(/* id 2, wireType 0 =*/16).bool(message.isOptional);
                    if (message.isSinglePattern != null && Object.hasOwnProperty.call(message, "isSinglePattern"))
                        writer.uint32(/* id 3, wireType 0 =*/24).bool(message.isSinglePattern);
                    if (message.selfType != null && Object.hasOwnProperty.call(message, "selfType"))
                        $root.baml_core.cffi.v1.BamlTy.encode(message.selfType, writer.uint32(/* id 4, wireType 2 =*/34).fork()).ldelim();
                    if (message.valueOptionName != null && Object.hasOwnProperty.call(message, "valueOptionName"))
                        writer.uint32(/* id 5, wireType 2 =*/42).string(message.valueOptionName);
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        $root.baml_core.cffi.v1.BamlOutboundValue.encode(message.value, writer.uint32(/* id 6, wireType 2 =*/50).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueUnionVariant message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueUnionVariant.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueUnionVariant} message BamlValueUnionVariant message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueUnionVariant.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueUnionVariant message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlValueUnionVariant} BamlValueUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueUnionVariant.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlValueUnionVariant();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = $root.baml_core.cffi.v1.BamlTyName.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.isOptional = reader.bool();
                                break;
                            }
                        case 3: {
                                message.isSinglePattern = reader.bool();
                                break;
                            }
                        case 4: {
                                message.selfType = $root.baml_core.cffi.v1.BamlTy.decode(reader, reader.uint32());
                                break;
                            }
                        case 5: {
                                message.valueOptionName = reader.string();
                                break;
                            }
                        case 6: {
                                message.value = $root.baml_core.cffi.v1.BamlOutboundValue.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlValueUnionVariant message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlValueUnionVariant} BamlValueUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueUnionVariant.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValueUnionVariant message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueUnionVariant.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml_core.cffi.v1.BamlTyName.verify(message.name);
                        if (error)
                            return "name." + error;
                    }
                    if (message.isOptional != null && message.hasOwnProperty("isOptional"))
                        if (typeof message.isOptional !== "boolean")
                            return "isOptional: boolean expected";
                    if (message.isSinglePattern != null && message.hasOwnProperty("isSinglePattern"))
                        if (typeof message.isSinglePattern !== "boolean")
                            return "isSinglePattern: boolean expected";
                    if (message.selfType != null && message.hasOwnProperty("selfType")) {
                        var error = $root.baml_core.cffi.v1.BamlTy.verify(message.selfType);
                        if (error)
                            return "selfType." + error;
                    }
                    if (message.valueOptionName != null && message.hasOwnProperty("valueOptionName"))
                        if (!$util.isString(message.valueOptionName))
                            return "valueOptionName: string expected";
                    if (message.value != null && message.hasOwnProperty("value")) {
                        var error = $root.baml_core.cffi.v1.BamlOutboundValue.verify(message.value);
                        if (error)
                            return "value." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlValueUnionVariant message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlValueUnionVariant} BamlValueUnionVariant
                 */
                BamlValueUnionVariant.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlValueUnionVariant)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlValueUnionVariant();
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValueUnionVariant.name: object expected");
                        message.name = $root.baml_core.cffi.v1.BamlTyName.fromObject(object.name);
                    }
                    if (object.isOptional != null)
                        message.isOptional = Boolean(object.isOptional);
                    if (object.isSinglePattern != null)
                        message.isSinglePattern = Boolean(object.isSinglePattern);
                    if (object.selfType != null) {
                        if (typeof object.selfType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValueUnionVariant.selfType: object expected");
                        message.selfType = $root.baml_core.cffi.v1.BamlTy.fromObject(object.selfType);
                    }
                    if (object.valueOptionName != null)
                        message.valueOptionName = String(object.valueOptionName);
                    if (object.value != null) {
                        if (typeof object.value !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValueUnionVariant.value: object expected");
                        message.value = $root.baml_core.cffi.v1.BamlOutboundValue.fromObject(object.value);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValueUnionVariant message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {baml_core.cffi.v1.BamlValueUnionVariant} message BamlValueUnionVariant
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValueUnionVariant.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults) {
                        object.name = null;
                        object.isOptional = false;
                        object.isSinglePattern = false;
                        object.selfType = null;
                        object.valueOptionName = "";
                        object.value = null;
                    }
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = $root.baml_core.cffi.v1.BamlTyName.toObject(message.name, options);
                    if (message.isOptional != null && message.hasOwnProperty("isOptional"))
                        object.isOptional = message.isOptional;
                    if (message.isSinglePattern != null && message.hasOwnProperty("isSinglePattern"))
                        object.isSinglePattern = message.isSinglePattern;
                    if (message.selfType != null && message.hasOwnProperty("selfType"))
                        object.selfType = $root.baml_core.cffi.v1.BamlTy.toObject(message.selfType, options);
                    if (message.valueOptionName != null && message.hasOwnProperty("valueOptionName"))
                        object.valueOptionName = message.valueOptionName;
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = $root.baml_core.cffi.v1.BamlOutboundValue.toObject(message.value, options);
                    return object;
                };

                /**
                 * Converts this BamlValueUnionVariant to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueUnionVariant.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueUnionVariant
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueUnionVariant.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlValueUnionVariant";
                };

                return BamlValueUnionVariant;
            })();

            /**
             * MediaTypeEnum enum.
             * @name baml_core.cffi.v1.MediaTypeEnum
             * @enum {number}
             * @property {number} MEDIA_TYPE_UNSPECIFIED=0 MEDIA_TYPE_UNSPECIFIED value
             * @property {number} IMAGE=1 IMAGE value
             * @property {number} AUDIO=2 AUDIO value
             * @property {number} PDF=3 PDF value
             * @property {number} VIDEO=4 VIDEO value
             * @property {number} OTHER=5 OTHER value
             */
            v1.MediaTypeEnum = (function() {
                var valuesById = {}, values = Object.create(valuesById);
                values[valuesById[0] = "MEDIA_TYPE_UNSPECIFIED"] = 0;
                values[valuesById[1] = "IMAGE"] = 1;
                values[valuesById[2] = "AUDIO"] = 2;
                values[valuesById[3] = "PDF"] = 3;
                values[valuesById[4] = "VIDEO"] = 4;
                values[valuesById[5] = "OTHER"] = 5;
                return values;
            })();

            v1.BamlValueMedia = (function() {

                /**
                 * Properties of a BamlValueMedia.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlValueMedia
                 * @property {baml_core.cffi.v1.MediaTypeEnum|null} [media] BamlValueMedia media
                 * @property {string|null} [mimeType] BamlValueMedia mimeType
                 * @property {string|null} [url] BamlValueMedia url
                 * @property {string|null} [base64] BamlValueMedia base64
                 * @property {string|null} [file] BamlValueMedia file
                 */

                /**
                 * Constructs a new BamlValueMedia.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlValueMedia.
                 * @implements IBamlValueMedia
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlValueMedia=} [properties] Properties to set
                 */
                function BamlValueMedia(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValueMedia media.
                 * @member {baml_core.cffi.v1.MediaTypeEnum} media
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @instance
                 */
                BamlValueMedia.prototype.media = 0;

                /**
                 * BamlValueMedia mimeType.
                 * @member {string|null|undefined} mimeType
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @instance
                 */
                BamlValueMedia.prototype.mimeType = null;

                /**
                 * BamlValueMedia url.
                 * @member {string|null|undefined} url
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @instance
                 */
                BamlValueMedia.prototype.url = null;

                /**
                 * BamlValueMedia base64.
                 * @member {string|null|undefined} base64
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @instance
                 */
                BamlValueMedia.prototype.base64 = null;

                /**
                 * BamlValueMedia file.
                 * @member {string|null|undefined} file
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @instance
                 */
                BamlValueMedia.prototype.file = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                // Virtual OneOf for proto3 optional field
                Object.defineProperty(BamlValueMedia.prototype, "_mimeType", {
                    get: $util.oneOfGetter($oneOfFields = ["mimeType"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * BamlValueMedia value.
                 * @member {"url"|"base64"|"file"|undefined} value
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @instance
                 */
                Object.defineProperty(BamlValueMedia.prototype, "value", {
                    get: $util.oneOfGetter($oneOfFields = ["url", "base64", "file"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new BamlValueMedia instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueMedia=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlValueMedia} BamlValueMedia instance
                 */
                BamlValueMedia.create = function create(properties) {
                    return new BamlValueMedia(properties);
                };

                /**
                 * Encodes the specified BamlValueMedia message. Does not implicitly {@link baml_core.cffi.v1.BamlValueMedia.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueMedia} message BamlValueMedia message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueMedia.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.media != null && Object.hasOwnProperty.call(message, "media"))
                        writer.uint32(/* id 1, wireType 0 =*/8).int32(message.media);
                    if (message.mimeType != null && Object.hasOwnProperty.call(message, "mimeType"))
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.mimeType);
                    if (message.url != null && Object.hasOwnProperty.call(message, "url"))
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.url);
                    if (message.base64 != null && Object.hasOwnProperty.call(message, "base64"))
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.base64);
                    if (message.file != null && Object.hasOwnProperty.call(message, "file"))
                        writer.uint32(/* id 5, wireType 2 =*/42).string(message.file);
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueMedia message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueMedia.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValueMedia} message BamlValueMedia message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueMedia.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueMedia message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlValueMedia} BamlValueMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueMedia.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlValueMedia();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.media = reader.int32();
                                break;
                            }
                        case 2: {
                                message.mimeType = reader.string();
                                break;
                            }
                        case 3: {
                                message.url = reader.string();
                                break;
                            }
                        case 4: {
                                message.base64 = reader.string();
                                break;
                            }
                        case 5: {
                                message.file = reader.string();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlValueMedia message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlValueMedia} BamlValueMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueMedia.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValueMedia message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueMedia.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    var properties = {};
                    if (message.media != null && message.hasOwnProperty("media"))
                        switch (message.media) {
                        default:
                            return "media: enum value expected";
                        case 0:
                        case 1:
                        case 2:
                        case 3:
                        case 4:
                        case 5:
                            break;
                        }
                    if (message.mimeType != null && message.hasOwnProperty("mimeType")) {
                        properties._mimeType = 1;
                        if (!$util.isString(message.mimeType))
                            return "mimeType: string expected";
                    }
                    if (message.url != null && message.hasOwnProperty("url")) {
                        properties.value = 1;
                        if (!$util.isString(message.url))
                            return "url: string expected";
                    }
                    if (message.base64 != null && message.hasOwnProperty("base64")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        if (!$util.isString(message.base64))
                            return "base64: string expected";
                    }
                    if (message.file != null && message.hasOwnProperty("file")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        if (!$util.isString(message.file))
                            return "file: string expected";
                    }
                    return null;
                };

                /**
                 * Creates a BamlValueMedia message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlValueMedia} BamlValueMedia
                 */
                BamlValueMedia.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlValueMedia)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlValueMedia();
                    switch (object.media) {
                    default:
                        if (typeof object.media === "number") {
                            message.media = object.media;
                            break;
                        }
                        break;
                    case "MEDIA_TYPE_UNSPECIFIED":
                    case 0:
                        message.media = 0;
                        break;
                    case "IMAGE":
                    case 1:
                        message.media = 1;
                        break;
                    case "AUDIO":
                    case 2:
                        message.media = 2;
                        break;
                    case "PDF":
                    case 3:
                        message.media = 3;
                        break;
                    case "VIDEO":
                    case 4:
                        message.media = 4;
                        break;
                    case "OTHER":
                    case 5:
                        message.media = 5;
                        break;
                    }
                    if (object.mimeType != null)
                        message.mimeType = String(object.mimeType);
                    if (object.url != null)
                        message.url = String(object.url);
                    if (object.base64 != null)
                        message.base64 = String(object.base64);
                    if (object.file != null)
                        message.file = String(object.file);
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValueMedia message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @static
                 * @param {baml_core.cffi.v1.BamlValueMedia} message BamlValueMedia
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValueMedia.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.media = options.enums === String ? "MEDIA_TYPE_UNSPECIFIED" : 0;
                    if (message.media != null && message.hasOwnProperty("media"))
                        object.media = options.enums === String ? $root.baml_core.cffi.v1.MediaTypeEnum[message.media] === undefined ? message.media : $root.baml_core.cffi.v1.MediaTypeEnum[message.media] : message.media;
                    if (message.mimeType != null && message.hasOwnProperty("mimeType")) {
                        object.mimeType = message.mimeType;
                        if (options.oneofs)
                            object._mimeType = "mimeType";
                    }
                    if (message.url != null && message.hasOwnProperty("url")) {
                        object.url = message.url;
                        if (options.oneofs)
                            object.value = "url";
                    }
                    if (message.base64 != null && message.hasOwnProperty("base64")) {
                        object.base64 = message.base64;
                        if (options.oneofs)
                            object.value = "base64";
                    }
                    if (message.file != null && message.hasOwnProperty("file")) {
                        object.file = message.file;
                        if (options.oneofs)
                            object.value = "file";
                    }
                    return object;
                };

                /**
                 * Converts this BamlValueMedia to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueMedia.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueMedia
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlValueMedia
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueMedia.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlValueMedia";
                };

                return BamlValueMedia;
            })();

            v1.BamlValuePromptAst = (function() {

                /**
                 * Properties of a BamlValuePromptAst.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlValuePromptAst
                 * @property {baml_core.cffi.v1.IBamlValuePromptAstSimple|null} [simple] BamlValuePromptAst simple
                 * @property {baml_core.cffi.v1.IBamlValuePromptAstMessage|null} [message] BamlValuePromptAst message
                 * @property {baml_core.cffi.v1.IBamlValuePromptAstMultiple|null} [multiple] BamlValuePromptAst multiple
                 */

                /**
                 * Constructs a new BamlValuePromptAst.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlValuePromptAst.
                 * @implements IBamlValuePromptAst
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlValuePromptAst=} [properties] Properties to set
                 */
                function BamlValuePromptAst(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValuePromptAst simple.
                 * @member {baml_core.cffi.v1.IBamlValuePromptAstSimple|null|undefined} simple
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @instance
                 */
                BamlValuePromptAst.prototype.simple = null;

                /**
                 * BamlValuePromptAst message.
                 * @member {baml_core.cffi.v1.IBamlValuePromptAstMessage|null|undefined} message
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @instance
                 */
                BamlValuePromptAst.prototype.message = null;

                /**
                 * BamlValuePromptAst multiple.
                 * @member {baml_core.cffi.v1.IBamlValuePromptAstMultiple|null|undefined} multiple
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @instance
                 */
                BamlValuePromptAst.prototype.multiple = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * BamlValuePromptAst value.
                 * @member {"simple"|"message"|"multiple"|undefined} value
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @instance
                 */
                Object.defineProperty(BamlValuePromptAst.prototype, "value", {
                    get: $util.oneOfGetter($oneOfFields = ["simple", "message", "multiple"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new BamlValuePromptAst instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAst=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlValuePromptAst} BamlValuePromptAst instance
                 */
                BamlValuePromptAst.create = function create(properties) {
                    return new BamlValuePromptAst(properties);
                };

                /**
                 * Encodes the specified BamlValuePromptAst message. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAst.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAst} message BamlValuePromptAst message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAst.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.simple != null && Object.hasOwnProperty.call(message, "simple"))
                        $root.baml_core.cffi.v1.BamlValuePromptAstSimple.encode(message.simple, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.message != null && Object.hasOwnProperty.call(message, "message"))
                        $root.baml_core.cffi.v1.BamlValuePromptAstMessage.encode(message.message, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.multiple != null && Object.hasOwnProperty.call(message, "multiple"))
                        $root.baml_core.cffi.v1.BamlValuePromptAstMultiple.encode(message.multiple, writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValuePromptAst message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAst.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAst} message BamlValuePromptAst message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAst.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValuePromptAst message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlValuePromptAst} BamlValuePromptAst
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAst.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlValuePromptAst();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.simple = $root.baml_core.cffi.v1.BamlValuePromptAstSimple.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.message = $root.baml_core.cffi.v1.BamlValuePromptAstMessage.decode(reader, reader.uint32());
                                break;
                            }
                        case 3: {
                                message.multiple = $root.baml_core.cffi.v1.BamlValuePromptAstMultiple.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlValuePromptAst message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlValuePromptAst} BamlValuePromptAst
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAst.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValuePromptAst message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValuePromptAst.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    var properties = {};
                    if (message.simple != null && message.hasOwnProperty("simple")) {
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlValuePromptAstSimple.verify(message.simple);
                            if (error)
                                return "simple." + error;
                        }
                    }
                    if (message.message != null && message.hasOwnProperty("message")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlValuePromptAstMessage.verify(message.message);
                            if (error)
                                return "message." + error;
                        }
                    }
                    if (message.multiple != null && message.hasOwnProperty("multiple")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlValuePromptAstMultiple.verify(message.multiple);
                            if (error)
                                return "multiple." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValuePromptAst message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlValuePromptAst} BamlValuePromptAst
                 */
                BamlValuePromptAst.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlValuePromptAst)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlValuePromptAst();
                    if (object.simple != null) {
                        if (typeof object.simple !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValuePromptAst.simple: object expected");
                        message.simple = $root.baml_core.cffi.v1.BamlValuePromptAstSimple.fromObject(object.simple);
                    }
                    if (object.message != null) {
                        if (typeof object.message !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValuePromptAst.message: object expected");
                        message.message = $root.baml_core.cffi.v1.BamlValuePromptAstMessage.fromObject(object.message);
                    }
                    if (object.multiple != null) {
                        if (typeof object.multiple !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValuePromptAst.multiple: object expected");
                        message.multiple = $root.baml_core.cffi.v1.BamlValuePromptAstMultiple.fromObject(object.multiple);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValuePromptAst message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {baml_core.cffi.v1.BamlValuePromptAst} message BamlValuePromptAst
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValuePromptAst.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (message.simple != null && message.hasOwnProperty("simple")) {
                        object.simple = $root.baml_core.cffi.v1.BamlValuePromptAstSimple.toObject(message.simple, options);
                        if (options.oneofs)
                            object.value = "simple";
                    }
                    if (message.message != null && message.hasOwnProperty("message")) {
                        object.message = $root.baml_core.cffi.v1.BamlValuePromptAstMessage.toObject(message.message, options);
                        if (options.oneofs)
                            object.value = "message";
                    }
                    if (message.multiple != null && message.hasOwnProperty("multiple")) {
                        object.multiple = $root.baml_core.cffi.v1.BamlValuePromptAstMultiple.toObject(message.multiple, options);
                        if (options.oneofs)
                            object.value = "multiple";
                    }
                    return object;
                };

                /**
                 * Converts this BamlValuePromptAst to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValuePromptAst.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValuePromptAst
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValuePromptAst.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlValuePromptAst";
                };

                return BamlValuePromptAst;
            })();

            v1.BamlValuePromptAstMessage = (function() {

                /**
                 * Properties of a BamlValuePromptAstMessage.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlValuePromptAstMessage
                 * @property {string|null} [role] BamlValuePromptAstMessage role
                 * @property {baml_core.cffi.v1.IBamlValuePromptAstSimple|null} [content] BamlValuePromptAstMessage content
                 * @property {string|null} [metadataAsJson] BamlValuePromptAstMessage metadataAsJson
                 */

                /**
                 * Constructs a new BamlValuePromptAstMessage.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlValuePromptAstMessage.
                 * @implements IBamlValuePromptAstMessage
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstMessage=} [properties] Properties to set
                 */
                function BamlValuePromptAstMessage(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValuePromptAstMessage role.
                 * @member {string} role
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMessage
                 * @instance
                 */
                BamlValuePromptAstMessage.prototype.role = "";

                /**
                 * BamlValuePromptAstMessage content.
                 * @member {baml_core.cffi.v1.IBamlValuePromptAstSimple|null|undefined} content
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMessage
                 * @instance
                 */
                BamlValuePromptAstMessage.prototype.content = null;

                /**
                 * BamlValuePromptAstMessage metadataAsJson.
                 * @member {string} metadataAsJson
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMessage
                 * @instance
                 */
                BamlValuePromptAstMessage.prototype.metadataAsJson = "";

                /**
                 * Creates a new BamlValuePromptAstMessage instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstMessage=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstMessage} BamlValuePromptAstMessage instance
                 */
                BamlValuePromptAstMessage.create = function create(properties) {
                    return new BamlValuePromptAstMessage(properties);
                };

                /**
                 * Encodes the specified BamlValuePromptAstMessage message. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstMessage.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstMessage} message BamlValuePromptAstMessage message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstMessage.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.role != null && Object.hasOwnProperty.call(message, "role"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.role);
                    if (message.content != null && Object.hasOwnProperty.call(message, "content"))
                        $root.baml_core.cffi.v1.BamlValuePromptAstSimple.encode(message.content, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.metadataAsJson != null && Object.hasOwnProperty.call(message, "metadataAsJson"))
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.metadataAsJson);
                    return writer;
                };

                /**
                 * Encodes the specified BamlValuePromptAstMessage message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstMessage.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstMessage} message BamlValuePromptAstMessage message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstMessage.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValuePromptAstMessage message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstMessage} BamlValuePromptAstMessage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAstMessage.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlValuePromptAstMessage();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.role = reader.string();
                                break;
                            }
                        case 2: {
                                message.content = $root.baml_core.cffi.v1.BamlValuePromptAstSimple.decode(reader, reader.uint32());
                                break;
                            }
                        case 3: {
                                message.metadataAsJson = reader.string();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlValuePromptAstMessage message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstMessage} BamlValuePromptAstMessage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAstMessage.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValuePromptAstMessage message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValuePromptAstMessage.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.role != null && message.hasOwnProperty("role"))
                        if (!$util.isString(message.role))
                            return "role: string expected";
                    if (message.content != null && message.hasOwnProperty("content")) {
                        var error = $root.baml_core.cffi.v1.BamlValuePromptAstSimple.verify(message.content);
                        if (error)
                            return "content." + error;
                    }
                    if (message.metadataAsJson != null && message.hasOwnProperty("metadataAsJson"))
                        if (!$util.isString(message.metadataAsJson))
                            return "metadataAsJson: string expected";
                    return null;
                };

                /**
                 * Creates a BamlValuePromptAstMessage message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstMessage} BamlValuePromptAstMessage
                 */
                BamlValuePromptAstMessage.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlValuePromptAstMessage)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlValuePromptAstMessage();
                    if (object.role != null)
                        message.role = String(object.role);
                    if (object.content != null) {
                        if (typeof object.content !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValuePromptAstMessage.content: object expected");
                        message.content = $root.baml_core.cffi.v1.BamlValuePromptAstSimple.fromObject(object.content);
                    }
                    if (object.metadataAsJson != null)
                        message.metadataAsJson = String(object.metadataAsJson);
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValuePromptAstMessage message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {baml_core.cffi.v1.BamlValuePromptAstMessage} message BamlValuePromptAstMessage
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValuePromptAstMessage.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults) {
                        object.role = "";
                        object.content = null;
                        object.metadataAsJson = "";
                    }
                    if (message.role != null && message.hasOwnProperty("role"))
                        object.role = message.role;
                    if (message.content != null && message.hasOwnProperty("content"))
                        object.content = $root.baml_core.cffi.v1.BamlValuePromptAstSimple.toObject(message.content, options);
                    if (message.metadataAsJson != null && message.hasOwnProperty("metadataAsJson"))
                        object.metadataAsJson = message.metadataAsJson;
                    return object;
                };

                /**
                 * Converts this BamlValuePromptAstMessage to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMessage
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValuePromptAstMessage.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValuePromptAstMessage
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValuePromptAstMessage.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlValuePromptAstMessage";
                };

                return BamlValuePromptAstMessage;
            })();

            v1.BamlValuePromptAstMultiple = (function() {

                /**
                 * Properties of a BamlValuePromptAstMultiple.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlValuePromptAstMultiple
                 * @property {Array.<baml_core.cffi.v1.IBamlValuePromptAst>|null} [items] BamlValuePromptAstMultiple items
                 */

                /**
                 * Constructs a new BamlValuePromptAstMultiple.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlValuePromptAstMultiple.
                 * @implements IBamlValuePromptAstMultiple
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstMultiple=} [properties] Properties to set
                 */
                function BamlValuePromptAstMultiple(properties) {
                    this.items = [];
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValuePromptAstMultiple items.
                 * @member {Array.<baml_core.cffi.v1.IBamlValuePromptAst>} items
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMultiple
                 * @instance
                 */
                BamlValuePromptAstMultiple.prototype.items = $util.emptyArray;

                /**
                 * Creates a new BamlValuePromptAstMultiple instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstMultiple=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstMultiple} BamlValuePromptAstMultiple instance
                 */
                BamlValuePromptAstMultiple.create = function create(properties) {
                    return new BamlValuePromptAstMultiple(properties);
                };

                /**
                 * Encodes the specified BamlValuePromptAstMultiple message. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstMultiple.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstMultiple} message BamlValuePromptAstMultiple message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstMultiple.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.items != null && message.items.length)
                        for (var i = 0; i < message.items.length; ++i)
                            $root.baml_core.cffi.v1.BamlValuePromptAst.encode(message.items[i], writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValuePromptAstMultiple message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstMultiple.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstMultiple} message BamlValuePromptAstMultiple message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstMultiple.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValuePromptAstMultiple message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstMultiple} BamlValuePromptAstMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAstMultiple.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlValuePromptAstMultiple();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                if (!(message.items && message.items.length))
                                    message.items = [];
                                message.items.push($root.baml_core.cffi.v1.BamlValuePromptAst.decode(reader, reader.uint32()));
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlValuePromptAstMultiple message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstMultiple} BamlValuePromptAstMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAstMultiple.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValuePromptAstMultiple message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValuePromptAstMultiple.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.items != null && message.hasOwnProperty("items")) {
                        if (!Array.isArray(message.items))
                            return "items: array expected";
                        for (var i = 0; i < message.items.length; ++i) {
                            var error = $root.baml_core.cffi.v1.BamlValuePromptAst.verify(message.items[i]);
                            if (error)
                                return "items." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValuePromptAstMultiple message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstMultiple} BamlValuePromptAstMultiple
                 */
                BamlValuePromptAstMultiple.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlValuePromptAstMultiple)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlValuePromptAstMultiple();
                    if (object.items) {
                        if (!Array.isArray(object.items))
                            throw TypeError(".baml_core.cffi.v1.BamlValuePromptAstMultiple.items: array expected");
                        message.items = [];
                        for (var i = 0; i < object.items.length; ++i) {
                            if (typeof object.items[i] !== "object")
                                throw TypeError(".baml_core.cffi.v1.BamlValuePromptAstMultiple.items: object expected");
                            message.items[i] = $root.baml_core.cffi.v1.BamlValuePromptAst.fromObject(object.items[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValuePromptAstMultiple message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {baml_core.cffi.v1.BamlValuePromptAstMultiple} message BamlValuePromptAstMultiple
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValuePromptAstMultiple.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.arrays || options.defaults)
                        object.items = [];
                    if (message.items && message.items.length) {
                        object.items = [];
                        for (var j = 0; j < message.items.length; ++j)
                            object.items[j] = $root.baml_core.cffi.v1.BamlValuePromptAst.toObject(message.items[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this BamlValuePromptAstMultiple to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMultiple
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValuePromptAstMultiple.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValuePromptAstMultiple
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValuePromptAstMultiple.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlValuePromptAstMultiple";
                };

                return BamlValuePromptAstMultiple;
            })();

            v1.BamlValuePromptAstSimple = (function() {

                /**
                 * Properties of a BamlValuePromptAstSimple.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlValuePromptAstSimple
                 * @property {string|null} [string] BamlValuePromptAstSimple string
                 * @property {baml_core.cffi.v1.IBamlValueMedia|null} [media] BamlValuePromptAstSimple media
                 * @property {baml_core.cffi.v1.IBamlValuePromptAstSimpleMultiple|null} [multiple] BamlValuePromptAstSimple multiple
                 */

                /**
                 * Constructs a new BamlValuePromptAstSimple.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlValuePromptAstSimple.
                 * @implements IBamlValuePromptAstSimple
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstSimple=} [properties] Properties to set
                 */
                function BamlValuePromptAstSimple(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValuePromptAstSimple string.
                 * @member {string|null|undefined} string
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @instance
                 */
                BamlValuePromptAstSimple.prototype.string = null;

                /**
                 * BamlValuePromptAstSimple media.
                 * @member {baml_core.cffi.v1.IBamlValueMedia|null|undefined} media
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @instance
                 */
                BamlValuePromptAstSimple.prototype.media = null;

                /**
                 * BamlValuePromptAstSimple multiple.
                 * @member {baml_core.cffi.v1.IBamlValuePromptAstSimpleMultiple|null|undefined} multiple
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @instance
                 */
                BamlValuePromptAstSimple.prototype.multiple = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * BamlValuePromptAstSimple value.
                 * @member {"string"|"media"|"multiple"|undefined} value
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @instance
                 */
                Object.defineProperty(BamlValuePromptAstSimple.prototype, "value", {
                    get: $util.oneOfGetter($oneOfFields = ["string", "media", "multiple"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new BamlValuePromptAstSimple instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstSimple=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstSimple} BamlValuePromptAstSimple instance
                 */
                BamlValuePromptAstSimple.create = function create(properties) {
                    return new BamlValuePromptAstSimple(properties);
                };

                /**
                 * Encodes the specified BamlValuePromptAstSimple message. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstSimple.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstSimple} message BamlValuePromptAstSimple message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstSimple.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.string != null && Object.hasOwnProperty.call(message, "string"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.string);
                    if (message.media != null && Object.hasOwnProperty.call(message, "media"))
                        $root.baml_core.cffi.v1.BamlValueMedia.encode(message.media, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.multiple != null && Object.hasOwnProperty.call(message, "multiple"))
                        $root.baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple.encode(message.multiple, writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValuePromptAstSimple message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstSimple.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstSimple} message BamlValuePromptAstSimple message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstSimple.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValuePromptAstSimple message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstSimple} BamlValuePromptAstSimple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAstSimple.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlValuePromptAstSimple();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.string = reader.string();
                                break;
                            }
                        case 2: {
                                message.media = $root.baml_core.cffi.v1.BamlValueMedia.decode(reader, reader.uint32());
                                break;
                            }
                        case 3: {
                                message.multiple = $root.baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlValuePromptAstSimple message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstSimple} BamlValuePromptAstSimple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAstSimple.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValuePromptAstSimple message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValuePromptAstSimple.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    var properties = {};
                    if (message.string != null && message.hasOwnProperty("string")) {
                        properties.value = 1;
                        if (!$util.isString(message.string))
                            return "string: string expected";
                    }
                    if (message.media != null && message.hasOwnProperty("media")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlValueMedia.verify(message.media);
                            if (error)
                                return "media." + error;
                        }
                    }
                    if (message.multiple != null && message.hasOwnProperty("multiple")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple.verify(message.multiple);
                            if (error)
                                return "multiple." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValuePromptAstSimple message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstSimple} BamlValuePromptAstSimple
                 */
                BamlValuePromptAstSimple.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlValuePromptAstSimple)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlValuePromptAstSimple();
                    if (object.string != null)
                        message.string = String(object.string);
                    if (object.media != null) {
                        if (typeof object.media !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValuePromptAstSimple.media: object expected");
                        message.media = $root.baml_core.cffi.v1.BamlValueMedia.fromObject(object.media);
                    }
                    if (object.multiple != null) {
                        if (typeof object.multiple !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlValuePromptAstSimple.multiple: object expected");
                        message.multiple = $root.baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple.fromObject(object.multiple);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValuePromptAstSimple message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {baml_core.cffi.v1.BamlValuePromptAstSimple} message BamlValuePromptAstSimple
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValuePromptAstSimple.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (message.string != null && message.hasOwnProperty("string")) {
                        object.string = message.string;
                        if (options.oneofs)
                            object.value = "string";
                    }
                    if (message.media != null && message.hasOwnProperty("media")) {
                        object.media = $root.baml_core.cffi.v1.BamlValueMedia.toObject(message.media, options);
                        if (options.oneofs)
                            object.value = "media";
                    }
                    if (message.multiple != null && message.hasOwnProperty("multiple")) {
                        object.multiple = $root.baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple.toObject(message.multiple, options);
                        if (options.oneofs)
                            object.value = "multiple";
                    }
                    return object;
                };

                /**
                 * Converts this BamlValuePromptAstSimple to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValuePromptAstSimple.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValuePromptAstSimple
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValuePromptAstSimple.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlValuePromptAstSimple";
                };

                return BamlValuePromptAstSimple;
            })();

            v1.BamlValuePromptAstSimpleMultiple = (function() {

                /**
                 * Properties of a BamlValuePromptAstSimpleMultiple.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlValuePromptAstSimpleMultiple
                 * @property {Array.<baml_core.cffi.v1.IBamlValuePromptAstSimple>|null} [items] BamlValuePromptAstSimpleMultiple items
                 */

                /**
                 * Constructs a new BamlValuePromptAstSimpleMultiple.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlValuePromptAstSimpleMultiple.
                 * @implements IBamlValuePromptAstSimpleMultiple
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstSimpleMultiple=} [properties] Properties to set
                 */
                function BamlValuePromptAstSimpleMultiple(properties) {
                    this.items = [];
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValuePromptAstSimpleMultiple items.
                 * @member {Array.<baml_core.cffi.v1.IBamlValuePromptAstSimple>} items
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @instance
                 */
                BamlValuePromptAstSimpleMultiple.prototype.items = $util.emptyArray;

                /**
                 * Creates a new BamlValuePromptAstSimpleMultiple instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstSimpleMultiple=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple} BamlValuePromptAstSimpleMultiple instance
                 */
                BamlValuePromptAstSimpleMultiple.create = function create(properties) {
                    return new BamlValuePromptAstSimpleMultiple(properties);
                };

                /**
                 * Encodes the specified BamlValuePromptAstSimpleMultiple message. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstSimpleMultiple} message BamlValuePromptAstSimpleMultiple message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstSimpleMultiple.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.items != null && message.items.length)
                        for (var i = 0; i < message.items.length; ++i)
                            $root.baml_core.cffi.v1.BamlValuePromptAstSimple.encode(message.items[i], writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValuePromptAstSimpleMultiple message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {baml_core.cffi.v1.IBamlValuePromptAstSimpleMultiple} message BamlValuePromptAstSimpleMultiple message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstSimpleMultiple.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValuePromptAstSimpleMultiple message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple} BamlValuePromptAstSimpleMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAstSimpleMultiple.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                if (!(message.items && message.items.length))
                                    message.items = [];
                                message.items.push($root.baml_core.cffi.v1.BamlValuePromptAstSimple.decode(reader, reader.uint32()));
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlValuePromptAstSimpleMultiple message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple} BamlValuePromptAstSimpleMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAstSimpleMultiple.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValuePromptAstSimpleMultiple message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValuePromptAstSimpleMultiple.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.items != null && message.hasOwnProperty("items")) {
                        if (!Array.isArray(message.items))
                            return "items: array expected";
                        for (var i = 0; i < message.items.length; ++i) {
                            var error = $root.baml_core.cffi.v1.BamlValuePromptAstSimple.verify(message.items[i]);
                            if (error)
                                return "items." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValuePromptAstSimpleMultiple message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple} BamlValuePromptAstSimpleMultiple
                 */
                BamlValuePromptAstSimpleMultiple.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple();
                    if (object.items) {
                        if (!Array.isArray(object.items))
                            throw TypeError(".baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple.items: array expected");
                        message.items = [];
                        for (var i = 0; i < object.items.length; ++i) {
                            if (typeof object.items[i] !== "object")
                                throw TypeError(".baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple.items: object expected");
                            message.items[i] = $root.baml_core.cffi.v1.BamlValuePromptAstSimple.fromObject(object.items[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValuePromptAstSimpleMultiple message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple} message BamlValuePromptAstSimpleMultiple
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValuePromptAstSimpleMultiple.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.arrays || options.defaults)
                        object.items = [];
                    if (message.items && message.items.length) {
                        object.items = [];
                        for (var j = 0; j < message.items.length; ++j)
                            object.items[j] = $root.baml_core.cffi.v1.BamlValuePromptAstSimple.toObject(message.items[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this BamlValuePromptAstSimpleMultiple to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValuePromptAstSimpleMultiple.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValuePromptAstSimpleMultiple
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValuePromptAstSimpleMultiple.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple";
                };

                return BamlValuePromptAstSimpleMultiple;
            })();

            v1.BamlTy = (function() {

                /**
                 * Properties of a BamlTy.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTy
                 * @property {baml_core.cffi.v1.IBamlTyString|null} [stringType] BamlTy stringType
                 * @property {baml_core.cffi.v1.IBamlTyInt|null} [intType] BamlTy intType
                 * @property {baml_core.cffi.v1.IBamlTyFloat|null} [floatType] BamlTy floatType
                 * @property {baml_core.cffi.v1.IBamlTyBool|null} [boolType] BamlTy boolType
                 * @property {baml_core.cffi.v1.IBamlTyNull|null} [nullType] BamlTy nullType
                 * @property {baml_core.cffi.v1.IBamlTyLiteral|null} [literalType] BamlTy literalType
                 * @property {baml_core.cffi.v1.IBamlTyMedia|null} [mediaType] BamlTy mediaType
                 * @property {baml_core.cffi.v1.IBamlTyEnum|null} [enumType] BamlTy enumType
                 * @property {baml_core.cffi.v1.IBamlTyClass|null} [classType] BamlTy classType
                 * @property {baml_core.cffi.v1.IBamlTyTypeAlias|null} [typeAliasType] BamlTy typeAliasType
                 * @property {baml_core.cffi.v1.IBamlTyList|null} [listType] BamlTy listType
                 * @property {baml_core.cffi.v1.IBamlTyMap|null} [mapType] BamlTy mapType
                 * @property {baml_core.cffi.v1.IBamlTyUnionVariant|null} [unionVariantType] BamlTy unionVariantType
                 * @property {baml_core.cffi.v1.IBamlTyOptional|null} [optionalType] BamlTy optionalType
                 * @property {baml_core.cffi.v1.IBamlTyAny|null} [anyType] BamlTy anyType
                 * @property {baml_core.cffi.v1.IBamlTyUint8Array|null} [uint8arrayType] BamlTy uint8arrayType
                 * @property {baml_core.cffi.v1.IBamlTyUnknown|null} [unknownType] BamlTy unknownType
                 */

                /**
                 * Constructs a new BamlTy.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTy.
                 * @implements IBamlTy
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTy=} [properties] Properties to set
                 */
                function BamlTy(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlTy stringType.
                 * @member {baml_core.cffi.v1.IBamlTyString|null|undefined} stringType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.stringType = null;

                /**
                 * BamlTy intType.
                 * @member {baml_core.cffi.v1.IBamlTyInt|null|undefined} intType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.intType = null;

                /**
                 * BamlTy floatType.
                 * @member {baml_core.cffi.v1.IBamlTyFloat|null|undefined} floatType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.floatType = null;

                /**
                 * BamlTy boolType.
                 * @member {baml_core.cffi.v1.IBamlTyBool|null|undefined} boolType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.boolType = null;

                /**
                 * BamlTy nullType.
                 * @member {baml_core.cffi.v1.IBamlTyNull|null|undefined} nullType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.nullType = null;

                /**
                 * BamlTy literalType.
                 * @member {baml_core.cffi.v1.IBamlTyLiteral|null|undefined} literalType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.literalType = null;

                /**
                 * BamlTy mediaType.
                 * @member {baml_core.cffi.v1.IBamlTyMedia|null|undefined} mediaType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.mediaType = null;

                /**
                 * BamlTy enumType.
                 * @member {baml_core.cffi.v1.IBamlTyEnum|null|undefined} enumType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.enumType = null;

                /**
                 * BamlTy classType.
                 * @member {baml_core.cffi.v1.IBamlTyClass|null|undefined} classType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.classType = null;

                /**
                 * BamlTy typeAliasType.
                 * @member {baml_core.cffi.v1.IBamlTyTypeAlias|null|undefined} typeAliasType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.typeAliasType = null;

                /**
                 * BamlTy listType.
                 * @member {baml_core.cffi.v1.IBamlTyList|null|undefined} listType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.listType = null;

                /**
                 * BamlTy mapType.
                 * @member {baml_core.cffi.v1.IBamlTyMap|null|undefined} mapType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.mapType = null;

                /**
                 * BamlTy unionVariantType.
                 * @member {baml_core.cffi.v1.IBamlTyUnionVariant|null|undefined} unionVariantType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.unionVariantType = null;

                /**
                 * BamlTy optionalType.
                 * @member {baml_core.cffi.v1.IBamlTyOptional|null|undefined} optionalType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.optionalType = null;

                /**
                 * BamlTy anyType.
                 * @member {baml_core.cffi.v1.IBamlTyAny|null|undefined} anyType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.anyType = null;

                /**
                 * BamlTy uint8arrayType.
                 * @member {baml_core.cffi.v1.IBamlTyUint8Array|null|undefined} uint8arrayType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.uint8arrayType = null;

                /**
                 * BamlTy unknownType.
                 * @member {baml_core.cffi.v1.IBamlTyUnknown|null|undefined} unknownType
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                BamlTy.prototype.unknownType = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * BamlTy type.
                 * @member {"stringType"|"intType"|"floatType"|"boolType"|"nullType"|"literalType"|"mediaType"|"enumType"|"classType"|"typeAliasType"|"listType"|"mapType"|"unionVariantType"|"optionalType"|"anyType"|"uint8arrayType"|"unknownType"|undefined} type
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 */
                Object.defineProperty(BamlTy.prototype, "type", {
                    get: $util.oneOfGetter($oneOfFields = ["stringType", "intType", "floatType", "boolType", "nullType", "literalType", "mediaType", "enumType", "classType", "typeAliasType", "listType", "mapType", "unionVariantType", "optionalType", "anyType", "uint8arrayType", "unknownType"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new BamlTy instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTy=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTy} BamlTy instance
                 */
                BamlTy.create = function create(properties) {
                    return new BamlTy(properties);
                };

                /**
                 * Encodes the specified BamlTy message. Does not implicitly {@link baml_core.cffi.v1.BamlTy.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTy} message BamlTy message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTy.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.stringType != null && Object.hasOwnProperty.call(message, "stringType"))
                        $root.baml_core.cffi.v1.BamlTyString.encode(message.stringType, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.intType != null && Object.hasOwnProperty.call(message, "intType"))
                        $root.baml_core.cffi.v1.BamlTyInt.encode(message.intType, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.floatType != null && Object.hasOwnProperty.call(message, "floatType"))
                        $root.baml_core.cffi.v1.BamlTyFloat.encode(message.floatType, writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    if (message.boolType != null && Object.hasOwnProperty.call(message, "boolType"))
                        $root.baml_core.cffi.v1.BamlTyBool.encode(message.boolType, writer.uint32(/* id 4, wireType 2 =*/34).fork()).ldelim();
                    if (message.nullType != null && Object.hasOwnProperty.call(message, "nullType"))
                        $root.baml_core.cffi.v1.BamlTyNull.encode(message.nullType, writer.uint32(/* id 5, wireType 2 =*/42).fork()).ldelim();
                    if (message.literalType != null && Object.hasOwnProperty.call(message, "literalType"))
                        $root.baml_core.cffi.v1.BamlTyLiteral.encode(message.literalType, writer.uint32(/* id 6, wireType 2 =*/50).fork()).ldelim();
                    if (message.mediaType != null && Object.hasOwnProperty.call(message, "mediaType"))
                        $root.baml_core.cffi.v1.BamlTyMedia.encode(message.mediaType, writer.uint32(/* id 7, wireType 2 =*/58).fork()).ldelim();
                    if (message.enumType != null && Object.hasOwnProperty.call(message, "enumType"))
                        $root.baml_core.cffi.v1.BamlTyEnum.encode(message.enumType, writer.uint32(/* id 8, wireType 2 =*/66).fork()).ldelim();
                    if (message.classType != null && Object.hasOwnProperty.call(message, "classType"))
                        $root.baml_core.cffi.v1.BamlTyClass.encode(message.classType, writer.uint32(/* id 9, wireType 2 =*/74).fork()).ldelim();
                    if (message.typeAliasType != null && Object.hasOwnProperty.call(message, "typeAliasType"))
                        $root.baml_core.cffi.v1.BamlTyTypeAlias.encode(message.typeAliasType, writer.uint32(/* id 10, wireType 2 =*/82).fork()).ldelim();
                    if (message.listType != null && Object.hasOwnProperty.call(message, "listType"))
                        $root.baml_core.cffi.v1.BamlTyList.encode(message.listType, writer.uint32(/* id 11, wireType 2 =*/90).fork()).ldelim();
                    if (message.mapType != null && Object.hasOwnProperty.call(message, "mapType"))
                        $root.baml_core.cffi.v1.BamlTyMap.encode(message.mapType, writer.uint32(/* id 12, wireType 2 =*/98).fork()).ldelim();
                    if (message.unionVariantType != null && Object.hasOwnProperty.call(message, "unionVariantType"))
                        $root.baml_core.cffi.v1.BamlTyUnionVariant.encode(message.unionVariantType, writer.uint32(/* id 14, wireType 2 =*/114).fork()).ldelim();
                    if (message.optionalType != null && Object.hasOwnProperty.call(message, "optionalType"))
                        $root.baml_core.cffi.v1.BamlTyOptional.encode(message.optionalType, writer.uint32(/* id 15, wireType 2 =*/122).fork()).ldelim();
                    if (message.anyType != null && Object.hasOwnProperty.call(message, "anyType"))
                        $root.baml_core.cffi.v1.BamlTyAny.encode(message.anyType, writer.uint32(/* id 18, wireType 2 =*/146).fork()).ldelim();
                    if (message.uint8arrayType != null && Object.hasOwnProperty.call(message, "uint8arrayType"))
                        $root.baml_core.cffi.v1.BamlTyUint8Array.encode(message.uint8arrayType, writer.uint32(/* id 19, wireType 2 =*/154).fork()).ldelim();
                    if (message.unknownType != null && Object.hasOwnProperty.call(message, "unknownType"))
                        $root.baml_core.cffi.v1.BamlTyUnknown.encode(message.unknownType, writer.uint32(/* id 20, wireType 2 =*/162).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTy message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTy.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTy} message BamlTy message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTy.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTy message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTy} BamlTy
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTy.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTy();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.stringType = $root.baml_core.cffi.v1.BamlTyString.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.intType = $root.baml_core.cffi.v1.BamlTyInt.decode(reader, reader.uint32());
                                break;
                            }
                        case 3: {
                                message.floatType = $root.baml_core.cffi.v1.BamlTyFloat.decode(reader, reader.uint32());
                                break;
                            }
                        case 4: {
                                message.boolType = $root.baml_core.cffi.v1.BamlTyBool.decode(reader, reader.uint32());
                                break;
                            }
                        case 5: {
                                message.nullType = $root.baml_core.cffi.v1.BamlTyNull.decode(reader, reader.uint32());
                                break;
                            }
                        case 6: {
                                message.literalType = $root.baml_core.cffi.v1.BamlTyLiteral.decode(reader, reader.uint32());
                                break;
                            }
                        case 7: {
                                message.mediaType = $root.baml_core.cffi.v1.BamlTyMedia.decode(reader, reader.uint32());
                                break;
                            }
                        case 8: {
                                message.enumType = $root.baml_core.cffi.v1.BamlTyEnum.decode(reader, reader.uint32());
                                break;
                            }
                        case 9: {
                                message.classType = $root.baml_core.cffi.v1.BamlTyClass.decode(reader, reader.uint32());
                                break;
                            }
                        case 10: {
                                message.typeAliasType = $root.baml_core.cffi.v1.BamlTyTypeAlias.decode(reader, reader.uint32());
                                break;
                            }
                        case 11: {
                                message.listType = $root.baml_core.cffi.v1.BamlTyList.decode(reader, reader.uint32());
                                break;
                            }
                        case 12: {
                                message.mapType = $root.baml_core.cffi.v1.BamlTyMap.decode(reader, reader.uint32());
                                break;
                            }
                        case 14: {
                                message.unionVariantType = $root.baml_core.cffi.v1.BamlTyUnionVariant.decode(reader, reader.uint32());
                                break;
                            }
                        case 15: {
                                message.optionalType = $root.baml_core.cffi.v1.BamlTyOptional.decode(reader, reader.uint32());
                                break;
                            }
                        case 18: {
                                message.anyType = $root.baml_core.cffi.v1.BamlTyAny.decode(reader, reader.uint32());
                                break;
                            }
                        case 19: {
                                message.uint8arrayType = $root.baml_core.cffi.v1.BamlTyUint8Array.decode(reader, reader.uint32());
                                break;
                            }
                        case 20: {
                                message.unknownType = $root.baml_core.cffi.v1.BamlTyUnknown.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTy message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTy} BamlTy
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTy.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTy message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTy.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    var properties = {};
                    if (message.stringType != null && message.hasOwnProperty("stringType")) {
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyString.verify(message.stringType);
                            if (error)
                                return "stringType." + error;
                        }
                    }
                    if (message.intType != null && message.hasOwnProperty("intType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyInt.verify(message.intType);
                            if (error)
                                return "intType." + error;
                        }
                    }
                    if (message.floatType != null && message.hasOwnProperty("floatType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyFloat.verify(message.floatType);
                            if (error)
                                return "floatType." + error;
                        }
                    }
                    if (message.boolType != null && message.hasOwnProperty("boolType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyBool.verify(message.boolType);
                            if (error)
                                return "boolType." + error;
                        }
                    }
                    if (message.nullType != null && message.hasOwnProperty("nullType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyNull.verify(message.nullType);
                            if (error)
                                return "nullType." + error;
                        }
                    }
                    if (message.literalType != null && message.hasOwnProperty("literalType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyLiteral.verify(message.literalType);
                            if (error)
                                return "literalType." + error;
                        }
                    }
                    if (message.mediaType != null && message.hasOwnProperty("mediaType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyMedia.verify(message.mediaType);
                            if (error)
                                return "mediaType." + error;
                        }
                    }
                    if (message.enumType != null && message.hasOwnProperty("enumType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyEnum.verify(message.enumType);
                            if (error)
                                return "enumType." + error;
                        }
                    }
                    if (message.classType != null && message.hasOwnProperty("classType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyClass.verify(message.classType);
                            if (error)
                                return "classType." + error;
                        }
                    }
                    if (message.typeAliasType != null && message.hasOwnProperty("typeAliasType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyTypeAlias.verify(message.typeAliasType);
                            if (error)
                                return "typeAliasType." + error;
                        }
                    }
                    if (message.listType != null && message.hasOwnProperty("listType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyList.verify(message.listType);
                            if (error)
                                return "listType." + error;
                        }
                    }
                    if (message.mapType != null && message.hasOwnProperty("mapType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyMap.verify(message.mapType);
                            if (error)
                                return "mapType." + error;
                        }
                    }
                    if (message.unionVariantType != null && message.hasOwnProperty("unionVariantType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyUnionVariant.verify(message.unionVariantType);
                            if (error)
                                return "unionVariantType." + error;
                        }
                    }
                    if (message.optionalType != null && message.hasOwnProperty("optionalType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyOptional.verify(message.optionalType);
                            if (error)
                                return "optionalType." + error;
                        }
                    }
                    if (message.anyType != null && message.hasOwnProperty("anyType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyAny.verify(message.anyType);
                            if (error)
                                return "anyType." + error;
                        }
                    }
                    if (message.uint8arrayType != null && message.hasOwnProperty("uint8arrayType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyUint8Array.verify(message.uint8arrayType);
                            if (error)
                                return "uint8arrayType." + error;
                        }
                    }
                    if (message.unknownType != null && message.hasOwnProperty("unknownType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlTyUnknown.verify(message.unknownType);
                            if (error)
                                return "unknownType." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlTy message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTy} BamlTy
                 */
                BamlTy.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTy)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlTy();
                    if (object.stringType != null) {
                        if (typeof object.stringType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.stringType: object expected");
                        message.stringType = $root.baml_core.cffi.v1.BamlTyString.fromObject(object.stringType);
                    }
                    if (object.intType != null) {
                        if (typeof object.intType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.intType: object expected");
                        message.intType = $root.baml_core.cffi.v1.BamlTyInt.fromObject(object.intType);
                    }
                    if (object.floatType != null) {
                        if (typeof object.floatType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.floatType: object expected");
                        message.floatType = $root.baml_core.cffi.v1.BamlTyFloat.fromObject(object.floatType);
                    }
                    if (object.boolType != null) {
                        if (typeof object.boolType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.boolType: object expected");
                        message.boolType = $root.baml_core.cffi.v1.BamlTyBool.fromObject(object.boolType);
                    }
                    if (object.nullType != null) {
                        if (typeof object.nullType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.nullType: object expected");
                        message.nullType = $root.baml_core.cffi.v1.BamlTyNull.fromObject(object.nullType);
                    }
                    if (object.literalType != null) {
                        if (typeof object.literalType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.literalType: object expected");
                        message.literalType = $root.baml_core.cffi.v1.BamlTyLiteral.fromObject(object.literalType);
                    }
                    if (object.mediaType != null) {
                        if (typeof object.mediaType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.mediaType: object expected");
                        message.mediaType = $root.baml_core.cffi.v1.BamlTyMedia.fromObject(object.mediaType);
                    }
                    if (object.enumType != null) {
                        if (typeof object.enumType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.enumType: object expected");
                        message.enumType = $root.baml_core.cffi.v1.BamlTyEnum.fromObject(object.enumType);
                    }
                    if (object.classType != null) {
                        if (typeof object.classType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.classType: object expected");
                        message.classType = $root.baml_core.cffi.v1.BamlTyClass.fromObject(object.classType);
                    }
                    if (object.typeAliasType != null) {
                        if (typeof object.typeAliasType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.typeAliasType: object expected");
                        message.typeAliasType = $root.baml_core.cffi.v1.BamlTyTypeAlias.fromObject(object.typeAliasType);
                    }
                    if (object.listType != null) {
                        if (typeof object.listType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.listType: object expected");
                        message.listType = $root.baml_core.cffi.v1.BamlTyList.fromObject(object.listType);
                    }
                    if (object.mapType != null) {
                        if (typeof object.mapType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.mapType: object expected");
                        message.mapType = $root.baml_core.cffi.v1.BamlTyMap.fromObject(object.mapType);
                    }
                    if (object.unionVariantType != null) {
                        if (typeof object.unionVariantType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.unionVariantType: object expected");
                        message.unionVariantType = $root.baml_core.cffi.v1.BamlTyUnionVariant.fromObject(object.unionVariantType);
                    }
                    if (object.optionalType != null) {
                        if (typeof object.optionalType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.optionalType: object expected");
                        message.optionalType = $root.baml_core.cffi.v1.BamlTyOptional.fromObject(object.optionalType);
                    }
                    if (object.anyType != null) {
                        if (typeof object.anyType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.anyType: object expected");
                        message.anyType = $root.baml_core.cffi.v1.BamlTyAny.fromObject(object.anyType);
                    }
                    if (object.uint8arrayType != null) {
                        if (typeof object.uint8arrayType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.uint8arrayType: object expected");
                        message.uint8arrayType = $root.baml_core.cffi.v1.BamlTyUint8Array.fromObject(object.uint8arrayType);
                    }
                    if (object.unknownType != null) {
                        if (typeof object.unknownType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTy.unknownType: object expected");
                        message.unknownType = $root.baml_core.cffi.v1.BamlTyUnknown.fromObject(object.unknownType);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlTy message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @static
                 * @param {baml_core.cffi.v1.BamlTy} message BamlTy
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTy.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (message.stringType != null && message.hasOwnProperty("stringType")) {
                        object.stringType = $root.baml_core.cffi.v1.BamlTyString.toObject(message.stringType, options);
                        if (options.oneofs)
                            object.type = "stringType";
                    }
                    if (message.intType != null && message.hasOwnProperty("intType")) {
                        object.intType = $root.baml_core.cffi.v1.BamlTyInt.toObject(message.intType, options);
                        if (options.oneofs)
                            object.type = "intType";
                    }
                    if (message.floatType != null && message.hasOwnProperty("floatType")) {
                        object.floatType = $root.baml_core.cffi.v1.BamlTyFloat.toObject(message.floatType, options);
                        if (options.oneofs)
                            object.type = "floatType";
                    }
                    if (message.boolType != null && message.hasOwnProperty("boolType")) {
                        object.boolType = $root.baml_core.cffi.v1.BamlTyBool.toObject(message.boolType, options);
                        if (options.oneofs)
                            object.type = "boolType";
                    }
                    if (message.nullType != null && message.hasOwnProperty("nullType")) {
                        object.nullType = $root.baml_core.cffi.v1.BamlTyNull.toObject(message.nullType, options);
                        if (options.oneofs)
                            object.type = "nullType";
                    }
                    if (message.literalType != null && message.hasOwnProperty("literalType")) {
                        object.literalType = $root.baml_core.cffi.v1.BamlTyLiteral.toObject(message.literalType, options);
                        if (options.oneofs)
                            object.type = "literalType";
                    }
                    if (message.mediaType != null && message.hasOwnProperty("mediaType")) {
                        object.mediaType = $root.baml_core.cffi.v1.BamlTyMedia.toObject(message.mediaType, options);
                        if (options.oneofs)
                            object.type = "mediaType";
                    }
                    if (message.enumType != null && message.hasOwnProperty("enumType")) {
                        object.enumType = $root.baml_core.cffi.v1.BamlTyEnum.toObject(message.enumType, options);
                        if (options.oneofs)
                            object.type = "enumType";
                    }
                    if (message.classType != null && message.hasOwnProperty("classType")) {
                        object.classType = $root.baml_core.cffi.v1.BamlTyClass.toObject(message.classType, options);
                        if (options.oneofs)
                            object.type = "classType";
                    }
                    if (message.typeAliasType != null && message.hasOwnProperty("typeAliasType")) {
                        object.typeAliasType = $root.baml_core.cffi.v1.BamlTyTypeAlias.toObject(message.typeAliasType, options);
                        if (options.oneofs)
                            object.type = "typeAliasType";
                    }
                    if (message.listType != null && message.hasOwnProperty("listType")) {
                        object.listType = $root.baml_core.cffi.v1.BamlTyList.toObject(message.listType, options);
                        if (options.oneofs)
                            object.type = "listType";
                    }
                    if (message.mapType != null && message.hasOwnProperty("mapType")) {
                        object.mapType = $root.baml_core.cffi.v1.BamlTyMap.toObject(message.mapType, options);
                        if (options.oneofs)
                            object.type = "mapType";
                    }
                    if (message.unionVariantType != null && message.hasOwnProperty("unionVariantType")) {
                        object.unionVariantType = $root.baml_core.cffi.v1.BamlTyUnionVariant.toObject(message.unionVariantType, options);
                        if (options.oneofs)
                            object.type = "unionVariantType";
                    }
                    if (message.optionalType != null && message.hasOwnProperty("optionalType")) {
                        object.optionalType = $root.baml_core.cffi.v1.BamlTyOptional.toObject(message.optionalType, options);
                        if (options.oneofs)
                            object.type = "optionalType";
                    }
                    if (message.anyType != null && message.hasOwnProperty("anyType")) {
                        object.anyType = $root.baml_core.cffi.v1.BamlTyAny.toObject(message.anyType, options);
                        if (options.oneofs)
                            object.type = "anyType";
                    }
                    if (message.uint8arrayType != null && message.hasOwnProperty("uint8arrayType")) {
                        object.uint8arrayType = $root.baml_core.cffi.v1.BamlTyUint8Array.toObject(message.uint8arrayType, options);
                        if (options.oneofs)
                            object.type = "uint8arrayType";
                    }
                    if (message.unknownType != null && message.hasOwnProperty("unknownType")) {
                        object.unknownType = $root.baml_core.cffi.v1.BamlTyUnknown.toObject(message.unknownType, options);
                        if (options.oneofs)
                            object.type = "unknownType";
                    }
                    return object;
                };

                /**
                 * Converts this BamlTy to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTy.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTy
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTy
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTy.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTy";
                };

                return BamlTy;
            })();

            v1.BamlTyString = (function() {

                /**
                 * Properties of a BamlTyString.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyString
                 */

                /**
                 * Constructs a new BamlTyString.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyString.
                 * @implements IBamlTyString
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyString=} [properties] Properties to set
                 */
                function BamlTyString(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlTyString instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyString
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyString=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyString} BamlTyString instance
                 */
                BamlTyString.create = function create(properties) {
                    return new BamlTyString(properties);
                };

                /**
                 * Encodes the specified BamlTyString message. Does not implicitly {@link baml_core.cffi.v1.BamlTyString.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyString
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyString} message BamlTyString message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyString.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyString message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyString.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyString
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyString} message BamlTyString message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyString.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyString message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyString
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyString} BamlTyString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyString.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyString();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyString message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyString
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyString} BamlTyString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyString.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyString message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyString
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyString.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlTyString message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyString
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyString} BamlTyString
                 */
                BamlTyString.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyString)
                        return object;
                    return new $root.baml_core.cffi.v1.BamlTyString();
                };

                /**
                 * Creates a plain object from a BamlTyString message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyString
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyString} message BamlTyString
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyString.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlTyString to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyString
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyString.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyString
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyString
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyString.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyString";
                };

                return BamlTyString;
            })();

            v1.BamlTyInt = (function() {

                /**
                 * Properties of a BamlTyInt.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyInt
                 */

                /**
                 * Constructs a new BamlTyInt.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyInt.
                 * @implements IBamlTyInt
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyInt=} [properties] Properties to set
                 */
                function BamlTyInt(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlTyInt instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyInt
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyInt=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyInt} BamlTyInt instance
                 */
                BamlTyInt.create = function create(properties) {
                    return new BamlTyInt(properties);
                };

                /**
                 * Encodes the specified BamlTyInt message. Does not implicitly {@link baml_core.cffi.v1.BamlTyInt.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyInt
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyInt} message BamlTyInt message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyInt.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyInt message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyInt.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyInt
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyInt} message BamlTyInt message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyInt.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyInt message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyInt
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyInt} BamlTyInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyInt.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyInt();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyInt message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyInt
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyInt} BamlTyInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyInt.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyInt message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyInt
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyInt.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlTyInt message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyInt
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyInt} BamlTyInt
                 */
                BamlTyInt.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyInt)
                        return object;
                    return new $root.baml_core.cffi.v1.BamlTyInt();
                };

                /**
                 * Creates a plain object from a BamlTyInt message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyInt
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyInt} message BamlTyInt
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyInt.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlTyInt to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyInt
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyInt.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyInt
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyInt
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyInt.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyInt";
                };

                return BamlTyInt;
            })();

            v1.BamlTyFloat = (function() {

                /**
                 * Properties of a BamlTyFloat.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyFloat
                 */

                /**
                 * Constructs a new BamlTyFloat.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyFloat.
                 * @implements IBamlTyFloat
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyFloat=} [properties] Properties to set
                 */
                function BamlTyFloat(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlTyFloat instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyFloat
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyFloat=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyFloat} BamlTyFloat instance
                 */
                BamlTyFloat.create = function create(properties) {
                    return new BamlTyFloat(properties);
                };

                /**
                 * Encodes the specified BamlTyFloat message. Does not implicitly {@link baml_core.cffi.v1.BamlTyFloat.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyFloat
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyFloat} message BamlTyFloat message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyFloat.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyFloat message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyFloat.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyFloat
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyFloat} message BamlTyFloat message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyFloat.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyFloat message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyFloat
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyFloat} BamlTyFloat
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyFloat.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyFloat();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyFloat message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyFloat
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyFloat} BamlTyFloat
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyFloat.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyFloat message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyFloat
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyFloat.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlTyFloat message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyFloat
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyFloat} BamlTyFloat
                 */
                BamlTyFloat.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyFloat)
                        return object;
                    return new $root.baml_core.cffi.v1.BamlTyFloat();
                };

                /**
                 * Creates a plain object from a BamlTyFloat message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyFloat
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyFloat} message BamlTyFloat
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyFloat.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlTyFloat to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyFloat
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyFloat.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyFloat
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyFloat
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyFloat.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyFloat";
                };

                return BamlTyFloat;
            })();

            v1.BamlTyBool = (function() {

                /**
                 * Properties of a BamlTyBool.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyBool
                 */

                /**
                 * Constructs a new BamlTyBool.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyBool.
                 * @implements IBamlTyBool
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyBool=} [properties] Properties to set
                 */
                function BamlTyBool(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlTyBool instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyBool
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyBool=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyBool} BamlTyBool instance
                 */
                BamlTyBool.create = function create(properties) {
                    return new BamlTyBool(properties);
                };

                /**
                 * Encodes the specified BamlTyBool message. Does not implicitly {@link baml_core.cffi.v1.BamlTyBool.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyBool
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyBool} message BamlTyBool message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyBool.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyBool message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyBool.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyBool
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyBool} message BamlTyBool message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyBool.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyBool message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyBool
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyBool} BamlTyBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyBool.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyBool();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyBool message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyBool
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyBool} BamlTyBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyBool.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyBool message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyBool
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyBool.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlTyBool message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyBool
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyBool} BamlTyBool
                 */
                BamlTyBool.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyBool)
                        return object;
                    return new $root.baml_core.cffi.v1.BamlTyBool();
                };

                /**
                 * Creates a plain object from a BamlTyBool message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyBool
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyBool} message BamlTyBool
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyBool.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlTyBool to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyBool
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyBool.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyBool
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyBool
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyBool.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyBool";
                };

                return BamlTyBool;
            })();

            v1.BamlTyNull = (function() {

                /**
                 * Properties of a BamlTyNull.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyNull
                 */

                /**
                 * Constructs a new BamlTyNull.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyNull.
                 * @implements IBamlTyNull
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyNull=} [properties] Properties to set
                 */
                function BamlTyNull(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlTyNull instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyNull
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyNull=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyNull} BamlTyNull instance
                 */
                BamlTyNull.create = function create(properties) {
                    return new BamlTyNull(properties);
                };

                /**
                 * Encodes the specified BamlTyNull message. Does not implicitly {@link baml_core.cffi.v1.BamlTyNull.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyNull
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyNull} message BamlTyNull message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyNull.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyNull message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyNull.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyNull
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyNull} message BamlTyNull message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyNull.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyNull message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyNull
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyNull} BamlTyNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyNull.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyNull();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyNull message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyNull
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyNull} BamlTyNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyNull.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyNull message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyNull
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyNull.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlTyNull message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyNull
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyNull} BamlTyNull
                 */
                BamlTyNull.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyNull)
                        return object;
                    return new $root.baml_core.cffi.v1.BamlTyNull();
                };

                /**
                 * Creates a plain object from a BamlTyNull message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyNull
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyNull} message BamlTyNull
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyNull.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlTyNull to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyNull
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyNull.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyNull
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyNull
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyNull.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyNull";
                };

                return BamlTyNull;
            })();

            v1.BamlTyUint8Array = (function() {

                /**
                 * Properties of a BamlTyUint8Array.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyUint8Array
                 */

                /**
                 * Constructs a new BamlTyUint8Array.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyUint8Array.
                 * @implements IBamlTyUint8Array
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyUint8Array=} [properties] Properties to set
                 */
                function BamlTyUint8Array(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlTyUint8Array instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyUint8Array
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyUint8Array=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyUint8Array} BamlTyUint8Array instance
                 */
                BamlTyUint8Array.create = function create(properties) {
                    return new BamlTyUint8Array(properties);
                };

                /**
                 * Encodes the specified BamlTyUint8Array message. Does not implicitly {@link baml_core.cffi.v1.BamlTyUint8Array.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyUint8Array
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyUint8Array} message BamlTyUint8Array message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyUint8Array.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyUint8Array message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyUint8Array.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyUint8Array
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyUint8Array} message BamlTyUint8Array message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyUint8Array.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyUint8Array message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyUint8Array
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyUint8Array} BamlTyUint8Array
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyUint8Array.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyUint8Array();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyUint8Array message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyUint8Array
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyUint8Array} BamlTyUint8Array
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyUint8Array.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyUint8Array message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyUint8Array
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyUint8Array.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlTyUint8Array message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyUint8Array
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyUint8Array} BamlTyUint8Array
                 */
                BamlTyUint8Array.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyUint8Array)
                        return object;
                    return new $root.baml_core.cffi.v1.BamlTyUint8Array();
                };

                /**
                 * Creates a plain object from a BamlTyUint8Array message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyUint8Array
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyUint8Array} message BamlTyUint8Array
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyUint8Array.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlTyUint8Array to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyUint8Array
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyUint8Array.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyUint8Array
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyUint8Array
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyUint8Array.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyUint8Array";
                };

                return BamlTyUint8Array;
            })();

            v1.BamlTyAny = (function() {

                /**
                 * Properties of a BamlTyAny.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyAny
                 */

                /**
                 * Constructs a new BamlTyAny.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyAny.
                 * @implements IBamlTyAny
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyAny=} [properties] Properties to set
                 */
                function BamlTyAny(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlTyAny instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyAny
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyAny=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyAny} BamlTyAny instance
                 */
                BamlTyAny.create = function create(properties) {
                    return new BamlTyAny(properties);
                };

                /**
                 * Encodes the specified BamlTyAny message. Does not implicitly {@link baml_core.cffi.v1.BamlTyAny.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyAny
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyAny} message BamlTyAny message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyAny.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyAny message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyAny.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyAny
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyAny} message BamlTyAny message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyAny.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyAny message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyAny
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyAny} BamlTyAny
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyAny.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyAny();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyAny message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyAny
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyAny} BamlTyAny
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyAny.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyAny message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyAny
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyAny.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlTyAny message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyAny
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyAny} BamlTyAny
                 */
                BamlTyAny.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyAny)
                        return object;
                    return new $root.baml_core.cffi.v1.BamlTyAny();
                };

                /**
                 * Creates a plain object from a BamlTyAny message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyAny
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyAny} message BamlTyAny
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyAny.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlTyAny to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyAny
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyAny.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyAny
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyAny
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyAny.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyAny";
                };

                return BamlTyAny;
            })();

            v1.BamlTyUnknown = (function() {

                /**
                 * Properties of a BamlTyUnknown.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyUnknown
                 */

                /**
                 * Constructs a new BamlTyUnknown.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyUnknown.
                 * @implements IBamlTyUnknown
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyUnknown=} [properties] Properties to set
                 */
                function BamlTyUnknown(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlTyUnknown instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyUnknown
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyUnknown=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyUnknown} BamlTyUnknown instance
                 */
                BamlTyUnknown.create = function create(properties) {
                    return new BamlTyUnknown(properties);
                };

                /**
                 * Encodes the specified BamlTyUnknown message. Does not implicitly {@link baml_core.cffi.v1.BamlTyUnknown.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyUnknown
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyUnknown} message BamlTyUnknown message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyUnknown.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyUnknown message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyUnknown.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyUnknown
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyUnknown} message BamlTyUnknown message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyUnknown.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyUnknown message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyUnknown
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyUnknown} BamlTyUnknown
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyUnknown.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyUnknown();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyUnknown message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyUnknown
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyUnknown} BamlTyUnknown
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyUnknown.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyUnknown message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyUnknown
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyUnknown.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlTyUnknown message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyUnknown
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyUnknown} BamlTyUnknown
                 */
                BamlTyUnknown.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyUnknown)
                        return object;
                    return new $root.baml_core.cffi.v1.BamlTyUnknown();
                };

                /**
                 * Creates a plain object from a BamlTyUnknown message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyUnknown
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyUnknown} message BamlTyUnknown
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyUnknown.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlTyUnknown to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyUnknown
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyUnknown.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyUnknown
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyUnknown
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyUnknown.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyUnknown";
                };

                return BamlTyUnknown;
            })();

            v1.BamlLiteralString = (function() {

                /**
                 * Properties of a BamlLiteralString.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlLiteralString
                 * @property {string|null} [value] BamlLiteralString value
                 */

                /**
                 * Constructs a new BamlLiteralString.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlLiteralString.
                 * @implements IBamlLiteralString
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlLiteralString=} [properties] Properties to set
                 */
                function BamlLiteralString(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlLiteralString value.
                 * @member {string} value
                 * @memberof baml_core.cffi.v1.BamlLiteralString
                 * @instance
                 */
                BamlLiteralString.prototype.value = "";

                /**
                 * Creates a new BamlLiteralString instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlLiteralString
                 * @static
                 * @param {baml_core.cffi.v1.IBamlLiteralString=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlLiteralString} BamlLiteralString instance
                 */
                BamlLiteralString.create = function create(properties) {
                    return new BamlLiteralString(properties);
                };

                /**
                 * Encodes the specified BamlLiteralString message. Does not implicitly {@link baml_core.cffi.v1.BamlLiteralString.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlLiteralString
                 * @static
                 * @param {baml_core.cffi.v1.IBamlLiteralString} message BamlLiteralString message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlLiteralString.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.value);
                    return writer;
                };

                /**
                 * Encodes the specified BamlLiteralString message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlLiteralString.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlLiteralString
                 * @static
                 * @param {baml_core.cffi.v1.IBamlLiteralString} message BamlLiteralString message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlLiteralString.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlLiteralString message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlLiteralString
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlLiteralString} BamlLiteralString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlLiteralString.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlLiteralString();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.value = reader.string();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlLiteralString message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlLiteralString
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlLiteralString} BamlLiteralString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlLiteralString.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlLiteralString message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlLiteralString
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlLiteralString.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.value != null && message.hasOwnProperty("value"))
                        if (!$util.isString(message.value))
                            return "value: string expected";
                    return null;
                };

                /**
                 * Creates a BamlLiteralString message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlLiteralString
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlLiteralString} BamlLiteralString
                 */
                BamlLiteralString.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlLiteralString)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlLiteralString();
                    if (object.value != null)
                        message.value = String(object.value);
                    return message;
                };

                /**
                 * Creates a plain object from a BamlLiteralString message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlLiteralString
                 * @static
                 * @param {baml_core.cffi.v1.BamlLiteralString} message BamlLiteralString
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlLiteralString.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.value = "";
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = message.value;
                    return object;
                };

                /**
                 * Converts this BamlLiteralString to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlLiteralString
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlLiteralString.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlLiteralString
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlLiteralString
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlLiteralString.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlLiteralString";
                };

                return BamlLiteralString;
            })();

            v1.BamlLiteralInt = (function() {

                /**
                 * Properties of a BamlLiteralInt.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlLiteralInt
                 * @property {number|Long|null} [value] BamlLiteralInt value
                 */

                /**
                 * Constructs a new BamlLiteralInt.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlLiteralInt.
                 * @implements IBamlLiteralInt
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlLiteralInt=} [properties] Properties to set
                 */
                function BamlLiteralInt(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlLiteralInt value.
                 * @member {number|Long} value
                 * @memberof baml_core.cffi.v1.BamlLiteralInt
                 * @instance
                 */
                BamlLiteralInt.prototype.value = $util.Long ? $util.Long.fromBits(0,0,false) : 0;

                /**
                 * Creates a new BamlLiteralInt instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {baml_core.cffi.v1.IBamlLiteralInt=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlLiteralInt} BamlLiteralInt instance
                 */
                BamlLiteralInt.create = function create(properties) {
                    return new BamlLiteralInt(properties);
                };

                /**
                 * Encodes the specified BamlLiteralInt message. Does not implicitly {@link baml_core.cffi.v1.BamlLiteralInt.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {baml_core.cffi.v1.IBamlLiteralInt} message BamlLiteralInt message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlLiteralInt.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        writer.uint32(/* id 1, wireType 0 =*/8).int64(message.value);
                    return writer;
                };

                /**
                 * Encodes the specified BamlLiteralInt message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlLiteralInt.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {baml_core.cffi.v1.IBamlLiteralInt} message BamlLiteralInt message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlLiteralInt.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlLiteralInt message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlLiteralInt} BamlLiteralInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlLiteralInt.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlLiteralInt();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.value = reader.int64();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlLiteralInt message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlLiteralInt} BamlLiteralInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlLiteralInt.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlLiteralInt message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlLiteralInt.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.value != null && message.hasOwnProperty("value"))
                        if (!$util.isInteger(message.value) && !(message.value && $util.isInteger(message.value.low) && $util.isInteger(message.value.high)))
                            return "value: integer|Long expected";
                    return null;
                };

                /**
                 * Creates a BamlLiteralInt message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlLiteralInt} BamlLiteralInt
                 */
                BamlLiteralInt.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlLiteralInt)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlLiteralInt();
                    if (object.value != null)
                        if ($util.Long)
                            (message.value = $util.Long.fromValue(object.value)).unsigned = false;
                        else if (typeof object.value === "string")
                            message.value = parseInt(object.value, 10);
                        else if (typeof object.value === "number")
                            message.value = object.value;
                        else if (typeof object.value === "object")
                            message.value = new $util.LongBits(object.value.low >>> 0, object.value.high >>> 0).toNumber();
                    return message;
                };

                /**
                 * Creates a plain object from a BamlLiteralInt message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {baml_core.cffi.v1.BamlLiteralInt} message BamlLiteralInt
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlLiteralInt.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        if ($util.Long) {
                            var long = new $util.Long(0, 0, false);
                            object.value = options.longs === String ? long.toString() : options.longs === Number ? long.toNumber() : long;
                        } else
                            object.value = options.longs === String ? "0" : 0;
                    if (message.value != null && message.hasOwnProperty("value"))
                        if (typeof message.value === "number")
                            object.value = options.longs === String ? String(message.value) : message.value;
                        else
                            object.value = options.longs === String ? $util.Long.prototype.toString.call(message.value) : options.longs === Number ? new $util.LongBits(message.value.low >>> 0, message.value.high >>> 0).toNumber() : message.value;
                    return object;
                };

                /**
                 * Converts this BamlLiteralInt to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlLiteralInt
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlLiteralInt.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlLiteralInt
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlLiteralInt.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlLiteralInt";
                };

                return BamlLiteralInt;
            })();

            v1.BamlLiteralBool = (function() {

                /**
                 * Properties of a BamlLiteralBool.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlLiteralBool
                 * @property {boolean|null} [value] BamlLiteralBool value
                 */

                /**
                 * Constructs a new BamlLiteralBool.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlLiteralBool.
                 * @implements IBamlLiteralBool
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlLiteralBool=} [properties] Properties to set
                 */
                function BamlLiteralBool(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlLiteralBool value.
                 * @member {boolean} value
                 * @memberof baml_core.cffi.v1.BamlLiteralBool
                 * @instance
                 */
                BamlLiteralBool.prototype.value = false;

                /**
                 * Creates a new BamlLiteralBool instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {baml_core.cffi.v1.IBamlLiteralBool=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlLiteralBool} BamlLiteralBool instance
                 */
                BamlLiteralBool.create = function create(properties) {
                    return new BamlLiteralBool(properties);
                };

                /**
                 * Encodes the specified BamlLiteralBool message. Does not implicitly {@link baml_core.cffi.v1.BamlLiteralBool.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {baml_core.cffi.v1.IBamlLiteralBool} message BamlLiteralBool message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlLiteralBool.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        writer.uint32(/* id 1, wireType 0 =*/8).bool(message.value);
                    return writer;
                };

                /**
                 * Encodes the specified BamlLiteralBool message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlLiteralBool.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {baml_core.cffi.v1.IBamlLiteralBool} message BamlLiteralBool message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlLiteralBool.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlLiteralBool message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlLiteralBool} BamlLiteralBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlLiteralBool.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlLiteralBool();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.value = reader.bool();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlLiteralBool message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlLiteralBool} BamlLiteralBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlLiteralBool.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlLiteralBool message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlLiteralBool.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.value != null && message.hasOwnProperty("value"))
                        if (typeof message.value !== "boolean")
                            return "value: boolean expected";
                    return null;
                };

                /**
                 * Creates a BamlLiteralBool message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlLiteralBool} BamlLiteralBool
                 */
                BamlLiteralBool.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlLiteralBool)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlLiteralBool();
                    if (object.value != null)
                        message.value = Boolean(object.value);
                    return message;
                };

                /**
                 * Creates a plain object from a BamlLiteralBool message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {baml_core.cffi.v1.BamlLiteralBool} message BamlLiteralBool
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlLiteralBool.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.value = false;
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = message.value;
                    return object;
                };

                /**
                 * Converts this BamlLiteralBool to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlLiteralBool
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlLiteralBool.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlLiteralBool
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlLiteralBool.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlLiteralBool";
                };

                return BamlLiteralBool;
            })();

            v1.BamlTyLiteral = (function() {

                /**
                 * Properties of a BamlTyLiteral.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyLiteral
                 * @property {baml_core.cffi.v1.IBamlLiteralString|null} [stringLiteral] BamlTyLiteral stringLiteral
                 * @property {baml_core.cffi.v1.IBamlLiteralInt|null} [intLiteral] BamlTyLiteral intLiteral
                 * @property {baml_core.cffi.v1.IBamlLiteralBool|null} [boolLiteral] BamlTyLiteral boolLiteral
                 */

                /**
                 * Constructs a new BamlTyLiteral.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyLiteral.
                 * @implements IBamlTyLiteral
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyLiteral=} [properties] Properties to set
                 */
                function BamlTyLiteral(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlTyLiteral stringLiteral.
                 * @member {baml_core.cffi.v1.IBamlLiteralString|null|undefined} stringLiteral
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @instance
                 */
                BamlTyLiteral.prototype.stringLiteral = null;

                /**
                 * BamlTyLiteral intLiteral.
                 * @member {baml_core.cffi.v1.IBamlLiteralInt|null|undefined} intLiteral
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @instance
                 */
                BamlTyLiteral.prototype.intLiteral = null;

                /**
                 * BamlTyLiteral boolLiteral.
                 * @member {baml_core.cffi.v1.IBamlLiteralBool|null|undefined} boolLiteral
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @instance
                 */
                BamlTyLiteral.prototype.boolLiteral = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * BamlTyLiteral literal.
                 * @member {"stringLiteral"|"intLiteral"|"boolLiteral"|undefined} literal
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @instance
                 */
                Object.defineProperty(BamlTyLiteral.prototype, "literal", {
                    get: $util.oneOfGetter($oneOfFields = ["stringLiteral", "intLiteral", "boolLiteral"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new BamlTyLiteral instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyLiteral=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyLiteral} BamlTyLiteral instance
                 */
                BamlTyLiteral.create = function create(properties) {
                    return new BamlTyLiteral(properties);
                };

                /**
                 * Encodes the specified BamlTyLiteral message. Does not implicitly {@link baml_core.cffi.v1.BamlTyLiteral.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyLiteral} message BamlTyLiteral message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyLiteral.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.stringLiteral != null && Object.hasOwnProperty.call(message, "stringLiteral"))
                        $root.baml_core.cffi.v1.BamlLiteralString.encode(message.stringLiteral, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.intLiteral != null && Object.hasOwnProperty.call(message, "intLiteral"))
                        $root.baml_core.cffi.v1.BamlLiteralInt.encode(message.intLiteral, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.boolLiteral != null && Object.hasOwnProperty.call(message, "boolLiteral"))
                        $root.baml_core.cffi.v1.BamlLiteralBool.encode(message.boolLiteral, writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyLiteral message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyLiteral.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyLiteral} message BamlTyLiteral message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyLiteral.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyLiteral message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyLiteral} BamlTyLiteral
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyLiteral.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyLiteral();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.stringLiteral = $root.baml_core.cffi.v1.BamlLiteralString.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.intLiteral = $root.baml_core.cffi.v1.BamlLiteralInt.decode(reader, reader.uint32());
                                break;
                            }
                        case 3: {
                                message.boolLiteral = $root.baml_core.cffi.v1.BamlLiteralBool.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyLiteral message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyLiteral} BamlTyLiteral
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyLiteral.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyLiteral message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyLiteral.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    var properties = {};
                    if (message.stringLiteral != null && message.hasOwnProperty("stringLiteral")) {
                        properties.literal = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlLiteralString.verify(message.stringLiteral);
                            if (error)
                                return "stringLiteral." + error;
                        }
                    }
                    if (message.intLiteral != null && message.hasOwnProperty("intLiteral")) {
                        if (properties.literal === 1)
                            return "literal: multiple values";
                        properties.literal = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlLiteralInt.verify(message.intLiteral);
                            if (error)
                                return "intLiteral." + error;
                        }
                    }
                    if (message.boolLiteral != null && message.hasOwnProperty("boolLiteral")) {
                        if (properties.literal === 1)
                            return "literal: multiple values";
                        properties.literal = 1;
                        {
                            var error = $root.baml_core.cffi.v1.BamlLiteralBool.verify(message.boolLiteral);
                            if (error)
                                return "boolLiteral." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlTyLiteral message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyLiteral} BamlTyLiteral
                 */
                BamlTyLiteral.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyLiteral)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlTyLiteral();
                    if (object.stringLiteral != null) {
                        if (typeof object.stringLiteral !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTyLiteral.stringLiteral: object expected");
                        message.stringLiteral = $root.baml_core.cffi.v1.BamlLiteralString.fromObject(object.stringLiteral);
                    }
                    if (object.intLiteral != null) {
                        if (typeof object.intLiteral !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTyLiteral.intLiteral: object expected");
                        message.intLiteral = $root.baml_core.cffi.v1.BamlLiteralInt.fromObject(object.intLiteral);
                    }
                    if (object.boolLiteral != null) {
                        if (typeof object.boolLiteral !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTyLiteral.boolLiteral: object expected");
                        message.boolLiteral = $root.baml_core.cffi.v1.BamlLiteralBool.fromObject(object.boolLiteral);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlTyLiteral message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyLiteral} message BamlTyLiteral
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyLiteral.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (message.stringLiteral != null && message.hasOwnProperty("stringLiteral")) {
                        object.stringLiteral = $root.baml_core.cffi.v1.BamlLiteralString.toObject(message.stringLiteral, options);
                        if (options.oneofs)
                            object.literal = "stringLiteral";
                    }
                    if (message.intLiteral != null && message.hasOwnProperty("intLiteral")) {
                        object.intLiteral = $root.baml_core.cffi.v1.BamlLiteralInt.toObject(message.intLiteral, options);
                        if (options.oneofs)
                            object.literal = "intLiteral";
                    }
                    if (message.boolLiteral != null && message.hasOwnProperty("boolLiteral")) {
                        object.boolLiteral = $root.baml_core.cffi.v1.BamlLiteralBool.toObject(message.boolLiteral, options);
                        if (options.oneofs)
                            object.literal = "boolLiteral";
                    }
                    return object;
                };

                /**
                 * Converts this BamlTyLiteral to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyLiteral.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyLiteral
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyLiteral
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyLiteral.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyLiteral";
                };

                return BamlTyLiteral;
            })();

            v1.BamlTyMedia = (function() {

                /**
                 * Properties of a BamlTyMedia.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyMedia
                 * @property {baml_core.cffi.v1.MediaTypeEnum|null} [media] BamlTyMedia media
                 */

                /**
                 * Constructs a new BamlTyMedia.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyMedia.
                 * @implements IBamlTyMedia
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyMedia=} [properties] Properties to set
                 */
                function BamlTyMedia(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlTyMedia media.
                 * @member {baml_core.cffi.v1.MediaTypeEnum} media
                 * @memberof baml_core.cffi.v1.BamlTyMedia
                 * @instance
                 */
                BamlTyMedia.prototype.media = 0;

                /**
                 * Creates a new BamlTyMedia instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyMedia
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyMedia=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyMedia} BamlTyMedia instance
                 */
                BamlTyMedia.create = function create(properties) {
                    return new BamlTyMedia(properties);
                };

                /**
                 * Encodes the specified BamlTyMedia message. Does not implicitly {@link baml_core.cffi.v1.BamlTyMedia.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyMedia
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyMedia} message BamlTyMedia message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyMedia.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.media != null && Object.hasOwnProperty.call(message, "media"))
                        writer.uint32(/* id 1, wireType 0 =*/8).int32(message.media);
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyMedia message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyMedia.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyMedia
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyMedia} message BamlTyMedia message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyMedia.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyMedia message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyMedia
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyMedia} BamlTyMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyMedia.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyMedia();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.media = reader.int32();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyMedia message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyMedia
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyMedia} BamlTyMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyMedia.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyMedia message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyMedia
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyMedia.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.media != null && message.hasOwnProperty("media"))
                        switch (message.media) {
                        default:
                            return "media: enum value expected";
                        case 0:
                        case 1:
                        case 2:
                        case 3:
                        case 4:
                        case 5:
                            break;
                        }
                    return null;
                };

                /**
                 * Creates a BamlTyMedia message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyMedia
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyMedia} BamlTyMedia
                 */
                BamlTyMedia.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyMedia)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlTyMedia();
                    switch (object.media) {
                    default:
                        if (typeof object.media === "number") {
                            message.media = object.media;
                            break;
                        }
                        break;
                    case "MEDIA_TYPE_UNSPECIFIED":
                    case 0:
                        message.media = 0;
                        break;
                    case "IMAGE":
                    case 1:
                        message.media = 1;
                        break;
                    case "AUDIO":
                    case 2:
                        message.media = 2;
                        break;
                    case "PDF":
                    case 3:
                        message.media = 3;
                        break;
                    case "VIDEO":
                    case 4:
                        message.media = 4;
                        break;
                    case "OTHER":
                    case 5:
                        message.media = 5;
                        break;
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlTyMedia message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyMedia
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyMedia} message BamlTyMedia
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyMedia.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.media = options.enums === String ? "MEDIA_TYPE_UNSPECIFIED" : 0;
                    if (message.media != null && message.hasOwnProperty("media"))
                        object.media = options.enums === String ? $root.baml_core.cffi.v1.MediaTypeEnum[message.media] === undefined ? message.media : $root.baml_core.cffi.v1.MediaTypeEnum[message.media] : message.media;
                    return object;
                };

                /**
                 * Converts this BamlTyMedia to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyMedia
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyMedia.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyMedia
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyMedia
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyMedia.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyMedia";
                };

                return BamlTyMedia;
            })();

            v1.BamlTyEnum = (function() {

                /**
                 * Properties of a BamlTyEnum.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyEnum
                 * @property {string|null} [name] BamlTyEnum name
                 */

                /**
                 * Constructs a new BamlTyEnum.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyEnum.
                 * @implements IBamlTyEnum
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyEnum=} [properties] Properties to set
                 */
                function BamlTyEnum(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlTyEnum name.
                 * @member {string} name
                 * @memberof baml_core.cffi.v1.BamlTyEnum
                 * @instance
                 */
                BamlTyEnum.prototype.name = "";

                /**
                 * Creates a new BamlTyEnum instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyEnum
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyEnum=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyEnum} BamlTyEnum instance
                 */
                BamlTyEnum.create = function create(properties) {
                    return new BamlTyEnum(properties);
                };

                /**
                 * Encodes the specified BamlTyEnum message. Does not implicitly {@link baml_core.cffi.v1.BamlTyEnum.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyEnum
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyEnum} message BamlTyEnum message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyEnum.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.name);
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyEnum message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyEnum.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyEnum
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyEnum} message BamlTyEnum message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyEnum.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyEnum message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyEnum
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyEnum} BamlTyEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyEnum.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyEnum();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = reader.string();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyEnum message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyEnum
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyEnum} BamlTyEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyEnum.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyEnum message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyEnum
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyEnum.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name"))
                        if (!$util.isString(message.name))
                            return "name: string expected";
                    return null;
                };

                /**
                 * Creates a BamlTyEnum message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyEnum
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyEnum} BamlTyEnum
                 */
                BamlTyEnum.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyEnum)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlTyEnum();
                    if (object.name != null)
                        message.name = String(object.name);
                    return message;
                };

                /**
                 * Creates a plain object from a BamlTyEnum message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyEnum
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyEnum} message BamlTyEnum
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyEnum.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.name = "";
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = message.name;
                    return object;
                };

                /**
                 * Converts this BamlTyEnum to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyEnum
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyEnum.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyEnum
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyEnum
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyEnum.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyEnum";
                };

                return BamlTyEnum;
            })();

            v1.BamlTyClass = (function() {

                /**
                 * Properties of a BamlTyClass.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyClass
                 * @property {baml_core.cffi.v1.IBamlTyName|null} [name] BamlTyClass name
                 */

                /**
                 * Constructs a new BamlTyClass.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyClass.
                 * @implements IBamlTyClass
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyClass=} [properties] Properties to set
                 */
                function BamlTyClass(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlTyClass name.
                 * @member {baml_core.cffi.v1.IBamlTyName|null|undefined} name
                 * @memberof baml_core.cffi.v1.BamlTyClass
                 * @instance
                 */
                BamlTyClass.prototype.name = null;

                /**
                 * Creates a new BamlTyClass instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyClass
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyClass=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyClass} BamlTyClass instance
                 */
                BamlTyClass.create = function create(properties) {
                    return new BamlTyClass(properties);
                };

                /**
                 * Encodes the specified BamlTyClass message. Does not implicitly {@link baml_core.cffi.v1.BamlTyClass.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyClass
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyClass} message BamlTyClass message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyClass.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml_core.cffi.v1.BamlTyName.encode(message.name, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyClass message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyClass.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyClass
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyClass} message BamlTyClass message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyClass.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyClass message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyClass
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyClass} BamlTyClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyClass.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyClass();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = $root.baml_core.cffi.v1.BamlTyName.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyClass message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyClass
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyClass} BamlTyClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyClass.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyClass message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyClass
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyClass.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml_core.cffi.v1.BamlTyName.verify(message.name);
                        if (error)
                            return "name." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlTyClass message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyClass
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyClass} BamlTyClass
                 */
                BamlTyClass.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyClass)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlTyClass();
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTyClass.name: object expected");
                        message.name = $root.baml_core.cffi.v1.BamlTyName.fromObject(object.name);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlTyClass message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyClass
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyClass} message BamlTyClass
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyClass.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.name = null;
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = $root.baml_core.cffi.v1.BamlTyName.toObject(message.name, options);
                    return object;
                };

                /**
                 * Converts this BamlTyClass to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyClass
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyClass.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyClass
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyClass
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyClass.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyClass";
                };

                return BamlTyClass;
            })();

            v1.BamlTyTypeAlias = (function() {

                /**
                 * Properties of a BamlTyTypeAlias.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyTypeAlias
                 * @property {baml_core.cffi.v1.IBamlTyName|null} [name] BamlTyTypeAlias name
                 */

                /**
                 * Constructs a new BamlTyTypeAlias.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyTypeAlias.
                 * @implements IBamlTyTypeAlias
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyTypeAlias=} [properties] Properties to set
                 */
                function BamlTyTypeAlias(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlTyTypeAlias name.
                 * @member {baml_core.cffi.v1.IBamlTyName|null|undefined} name
                 * @memberof baml_core.cffi.v1.BamlTyTypeAlias
                 * @instance
                 */
                BamlTyTypeAlias.prototype.name = null;

                /**
                 * Creates a new BamlTyTypeAlias instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyTypeAlias
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyTypeAlias=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyTypeAlias} BamlTyTypeAlias instance
                 */
                BamlTyTypeAlias.create = function create(properties) {
                    return new BamlTyTypeAlias(properties);
                };

                /**
                 * Encodes the specified BamlTyTypeAlias message. Does not implicitly {@link baml_core.cffi.v1.BamlTyTypeAlias.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyTypeAlias
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyTypeAlias} message BamlTyTypeAlias message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyTypeAlias.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml_core.cffi.v1.BamlTyName.encode(message.name, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyTypeAlias message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyTypeAlias.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyTypeAlias
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyTypeAlias} message BamlTyTypeAlias message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyTypeAlias.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyTypeAlias message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyTypeAlias
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyTypeAlias} BamlTyTypeAlias
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyTypeAlias.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyTypeAlias();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = $root.baml_core.cffi.v1.BamlTyName.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyTypeAlias message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyTypeAlias
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyTypeAlias} BamlTyTypeAlias
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyTypeAlias.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyTypeAlias message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyTypeAlias
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyTypeAlias.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml_core.cffi.v1.BamlTyName.verify(message.name);
                        if (error)
                            return "name." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlTyTypeAlias message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyTypeAlias
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyTypeAlias} BamlTyTypeAlias
                 */
                BamlTyTypeAlias.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyTypeAlias)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlTyTypeAlias();
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTyTypeAlias.name: object expected");
                        message.name = $root.baml_core.cffi.v1.BamlTyName.fromObject(object.name);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlTyTypeAlias message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyTypeAlias
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyTypeAlias} message BamlTyTypeAlias
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyTypeAlias.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.name = null;
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = $root.baml_core.cffi.v1.BamlTyName.toObject(message.name, options);
                    return object;
                };

                /**
                 * Converts this BamlTyTypeAlias to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyTypeAlias
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyTypeAlias.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyTypeAlias
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyTypeAlias
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyTypeAlias.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyTypeAlias";
                };

                return BamlTyTypeAlias;
            })();

            v1.BamlTyList = (function() {

                /**
                 * Properties of a BamlTyList.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyList
                 * @property {baml_core.cffi.v1.IBamlTy|null} [itemType] BamlTyList itemType
                 */

                /**
                 * Constructs a new BamlTyList.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyList.
                 * @implements IBamlTyList
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyList=} [properties] Properties to set
                 */
                function BamlTyList(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlTyList itemType.
                 * @member {baml_core.cffi.v1.IBamlTy|null|undefined} itemType
                 * @memberof baml_core.cffi.v1.BamlTyList
                 * @instance
                 */
                BamlTyList.prototype.itemType = null;

                /**
                 * Creates a new BamlTyList instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyList
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyList=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyList} BamlTyList instance
                 */
                BamlTyList.create = function create(properties) {
                    return new BamlTyList(properties);
                };

                /**
                 * Encodes the specified BamlTyList message. Does not implicitly {@link baml_core.cffi.v1.BamlTyList.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyList
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyList} message BamlTyList message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyList.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.itemType != null && Object.hasOwnProperty.call(message, "itemType"))
                        $root.baml_core.cffi.v1.BamlTy.encode(message.itemType, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyList message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyList.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyList
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyList} message BamlTyList message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyList.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyList message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyList
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyList} BamlTyList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyList.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyList();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.itemType = $root.baml_core.cffi.v1.BamlTy.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyList message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyList
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyList} BamlTyList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyList.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyList message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyList
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyList.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.itemType != null && message.hasOwnProperty("itemType")) {
                        var error = $root.baml_core.cffi.v1.BamlTy.verify(message.itemType);
                        if (error)
                            return "itemType." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlTyList message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyList
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyList} BamlTyList
                 */
                BamlTyList.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyList)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlTyList();
                    if (object.itemType != null) {
                        if (typeof object.itemType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTyList.itemType: object expected");
                        message.itemType = $root.baml_core.cffi.v1.BamlTy.fromObject(object.itemType);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlTyList message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyList
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyList} message BamlTyList
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyList.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.itemType = null;
                    if (message.itemType != null && message.hasOwnProperty("itemType"))
                        object.itemType = $root.baml_core.cffi.v1.BamlTy.toObject(message.itemType, options);
                    return object;
                };

                /**
                 * Converts this BamlTyList to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyList
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyList.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyList
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyList
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyList.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyList";
                };

                return BamlTyList;
            })();

            v1.BamlTyMap = (function() {

                /**
                 * Properties of a BamlTyMap.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyMap
                 * @property {baml_core.cffi.v1.IBamlTy|null} [keyType] BamlTyMap keyType
                 * @property {baml_core.cffi.v1.IBamlTy|null} [valueType] BamlTyMap valueType
                 */

                /**
                 * Constructs a new BamlTyMap.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyMap.
                 * @implements IBamlTyMap
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyMap=} [properties] Properties to set
                 */
                function BamlTyMap(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlTyMap keyType.
                 * @member {baml_core.cffi.v1.IBamlTy|null|undefined} keyType
                 * @memberof baml_core.cffi.v1.BamlTyMap
                 * @instance
                 */
                BamlTyMap.prototype.keyType = null;

                /**
                 * BamlTyMap valueType.
                 * @member {baml_core.cffi.v1.IBamlTy|null|undefined} valueType
                 * @memberof baml_core.cffi.v1.BamlTyMap
                 * @instance
                 */
                BamlTyMap.prototype.valueType = null;

                /**
                 * Creates a new BamlTyMap instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyMap
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyMap=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyMap} BamlTyMap instance
                 */
                BamlTyMap.create = function create(properties) {
                    return new BamlTyMap(properties);
                };

                /**
                 * Encodes the specified BamlTyMap message. Does not implicitly {@link baml_core.cffi.v1.BamlTyMap.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyMap
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyMap} message BamlTyMap message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyMap.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.keyType != null && Object.hasOwnProperty.call(message, "keyType"))
                        $root.baml_core.cffi.v1.BamlTy.encode(message.keyType, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.valueType != null && Object.hasOwnProperty.call(message, "valueType"))
                        $root.baml_core.cffi.v1.BamlTy.encode(message.valueType, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyMap message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyMap.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyMap
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyMap} message BamlTyMap message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyMap.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyMap message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyMap
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyMap} BamlTyMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyMap.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyMap();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.keyType = $root.baml_core.cffi.v1.BamlTy.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.valueType = $root.baml_core.cffi.v1.BamlTy.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyMap message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyMap
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyMap} BamlTyMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyMap.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyMap message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyMap
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyMap.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.keyType != null && message.hasOwnProperty("keyType")) {
                        var error = $root.baml_core.cffi.v1.BamlTy.verify(message.keyType);
                        if (error)
                            return "keyType." + error;
                    }
                    if (message.valueType != null && message.hasOwnProperty("valueType")) {
                        var error = $root.baml_core.cffi.v1.BamlTy.verify(message.valueType);
                        if (error)
                            return "valueType." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlTyMap message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyMap
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyMap} BamlTyMap
                 */
                BamlTyMap.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyMap)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlTyMap();
                    if (object.keyType != null) {
                        if (typeof object.keyType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTyMap.keyType: object expected");
                        message.keyType = $root.baml_core.cffi.v1.BamlTy.fromObject(object.keyType);
                    }
                    if (object.valueType != null) {
                        if (typeof object.valueType !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTyMap.valueType: object expected");
                        message.valueType = $root.baml_core.cffi.v1.BamlTy.fromObject(object.valueType);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlTyMap message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyMap
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyMap} message BamlTyMap
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyMap.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults) {
                        object.keyType = null;
                        object.valueType = null;
                    }
                    if (message.keyType != null && message.hasOwnProperty("keyType"))
                        object.keyType = $root.baml_core.cffi.v1.BamlTy.toObject(message.keyType, options);
                    if (message.valueType != null && message.hasOwnProperty("valueType"))
                        object.valueType = $root.baml_core.cffi.v1.BamlTy.toObject(message.valueType, options);
                    return object;
                };

                /**
                 * Converts this BamlTyMap to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyMap
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyMap.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyMap
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyMap
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyMap.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyMap";
                };

                return BamlTyMap;
            })();

            v1.BamlTyUnionVariant = (function() {

                /**
                 * Properties of a BamlTyUnionVariant.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyUnionVariant
                 * @property {baml_core.cffi.v1.IBamlTyName|null} [name] BamlTyUnionVariant name
                 */

                /**
                 * Constructs a new BamlTyUnionVariant.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyUnionVariant.
                 * @implements IBamlTyUnionVariant
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyUnionVariant=} [properties] Properties to set
                 */
                function BamlTyUnionVariant(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlTyUnionVariant name.
                 * @member {baml_core.cffi.v1.IBamlTyName|null|undefined} name
                 * @memberof baml_core.cffi.v1.BamlTyUnionVariant
                 * @instance
                 */
                BamlTyUnionVariant.prototype.name = null;

                /**
                 * Creates a new BamlTyUnionVariant instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyUnionVariant
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyUnionVariant=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyUnionVariant} BamlTyUnionVariant instance
                 */
                BamlTyUnionVariant.create = function create(properties) {
                    return new BamlTyUnionVariant(properties);
                };

                /**
                 * Encodes the specified BamlTyUnionVariant message. Does not implicitly {@link baml_core.cffi.v1.BamlTyUnionVariant.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyUnionVariant
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyUnionVariant} message BamlTyUnionVariant message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyUnionVariant.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml_core.cffi.v1.BamlTyName.encode(message.name, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyUnionVariant message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyUnionVariant.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyUnionVariant
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyUnionVariant} message BamlTyUnionVariant message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyUnionVariant.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyUnionVariant message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyUnionVariant
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyUnionVariant} BamlTyUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyUnionVariant.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyUnionVariant();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = $root.baml_core.cffi.v1.BamlTyName.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyUnionVariant message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyUnionVariant
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyUnionVariant} BamlTyUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyUnionVariant.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyUnionVariant message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyUnionVariant
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyUnionVariant.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml_core.cffi.v1.BamlTyName.verify(message.name);
                        if (error)
                            return "name." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlTyUnionVariant message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyUnionVariant
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyUnionVariant} BamlTyUnionVariant
                 */
                BamlTyUnionVariant.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyUnionVariant)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlTyUnionVariant();
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTyUnionVariant.name: object expected");
                        message.name = $root.baml_core.cffi.v1.BamlTyName.fromObject(object.name);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlTyUnionVariant message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyUnionVariant
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyUnionVariant} message BamlTyUnionVariant
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyUnionVariant.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.name = null;
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = $root.baml_core.cffi.v1.BamlTyName.toObject(message.name, options);
                    return object;
                };

                /**
                 * Converts this BamlTyUnionVariant to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyUnionVariant
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyUnionVariant.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyUnionVariant
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyUnionVariant
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyUnionVariant.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyUnionVariant";
                };

                return BamlTyUnionVariant;
            })();

            v1.BamlTyOptional = (function() {

                /**
                 * Properties of a BamlTyOptional.
                 * @memberof baml_core.cffi.v1
                 * @interface IBamlTyOptional
                 * @property {baml_core.cffi.v1.IBamlTy|null} [value] BamlTyOptional value
                 */

                /**
                 * Constructs a new BamlTyOptional.
                 * @memberof baml_core.cffi.v1
                 * @classdesc Represents a BamlTyOptional.
                 * @implements IBamlTyOptional
                 * @constructor
                 * @param {baml_core.cffi.v1.IBamlTyOptional=} [properties] Properties to set
                 */
                function BamlTyOptional(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlTyOptional value.
                 * @member {baml_core.cffi.v1.IBamlTy|null|undefined} value
                 * @memberof baml_core.cffi.v1.BamlTyOptional
                 * @instance
                 */
                BamlTyOptional.prototype.value = null;

                /**
                 * Creates a new BamlTyOptional instance using the specified properties.
                 * @function create
                 * @memberof baml_core.cffi.v1.BamlTyOptional
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyOptional=} [properties] Properties to set
                 * @returns {baml_core.cffi.v1.BamlTyOptional} BamlTyOptional instance
                 */
                BamlTyOptional.create = function create(properties) {
                    return new BamlTyOptional(properties);
                };

                /**
                 * Encodes the specified BamlTyOptional message. Does not implicitly {@link baml_core.cffi.v1.BamlTyOptional.verify|verify} messages.
                 * @function encode
                 * @memberof baml_core.cffi.v1.BamlTyOptional
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyOptional} message BamlTyOptional message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyOptional.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        $root.baml_core.cffi.v1.BamlTy.encode(message.value, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlTyOptional message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyOptional.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyOptional
                 * @static
                 * @param {baml_core.cffi.v1.IBamlTyOptional} message BamlTyOptional message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTyOptional.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTyOptional message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml_core.cffi.v1.BamlTyOptional
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml_core.cffi.v1.BamlTyOptional} BamlTyOptional
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyOptional.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml_core.cffi.v1.BamlTyOptional();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.value = $root.baml_core.cffi.v1.BamlTy.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a BamlTyOptional message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml_core.cffi.v1.BamlTyOptional
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml_core.cffi.v1.BamlTyOptional} BamlTyOptional
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTyOptional.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTyOptional message.
                 * @function verify
                 * @memberof baml_core.cffi.v1.BamlTyOptional
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTyOptional.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.value != null && message.hasOwnProperty("value")) {
                        var error = $root.baml_core.cffi.v1.BamlTy.verify(message.value);
                        if (error)
                            return "value." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlTyOptional message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml_core.cffi.v1.BamlTyOptional
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml_core.cffi.v1.BamlTyOptional} BamlTyOptional
                 */
                BamlTyOptional.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml_core.cffi.v1.BamlTyOptional)
                        return object;
                    var message = new $root.baml_core.cffi.v1.BamlTyOptional();
                    if (object.value != null) {
                        if (typeof object.value !== "object")
                            throw TypeError(".baml_core.cffi.v1.BamlTyOptional.value: object expected");
                        message.value = $root.baml_core.cffi.v1.BamlTy.fromObject(object.value);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlTyOptional message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml_core.cffi.v1.BamlTyOptional
                 * @static
                 * @param {baml_core.cffi.v1.BamlTyOptional} message BamlTyOptional
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTyOptional.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.value = null;
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = $root.baml_core.cffi.v1.BamlTy.toObject(message.value, options);
                    return object;
                };

                /**
                 * Converts this BamlTyOptional to JSON.
                 * @function toJSON
                 * @memberof baml_core.cffi.v1.BamlTyOptional
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTyOptional.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTyOptional
                 * @function getTypeUrl
                 * @memberof baml_core.cffi.v1.BamlTyOptional
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTyOptional.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml_core.cffi.v1.BamlTyOptional";
                };

                return BamlTyOptional;
            })();

            return v1;
        })();

        return cffi;
    })();

    return baml_core;
})();

module.exports = $root;
