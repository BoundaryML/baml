/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
/*eslint-disable block-scoped-var, id-length, no-control-regex, no-magic-numbers, no-prototype-builtins, no-redeclare, no-shadow, no-var, sort-vars*/
"use strict";

var $protobuf = require("protobufjs/minimal");

// Common aliases
var $Reader = $protobuf.Reader, $Writer = $protobuf.Writer, $util = $protobuf.util;

// Exported root namespace
var $root = $protobuf.roots["default"] || ($protobuf.roots["default"] = {});

$root.baml = (function() {

    /**
     * Namespace baml.
     * @exports baml
     * @namespace
     */
    var baml = {};

    baml.cffi = (function() {

        /**
         * Namespace cffi.
         * @memberof baml
         * @namespace
         */
        var cffi = {};

        cffi.v1 = (function() {

            /**
             * Namespace v1.
             * @memberof baml.cffi
             * @namespace
             */
            var v1 = {};

            /**
             * BamlHandleType enum.
             * @name baml.cffi.v1.BamlHandleType
             * @enum {number}
             * @property {number} HANDLE_UNSPECIFIED=0 HANDLE_UNSPECIFIED value
             * @property {number} HANDLE_UNKNOWN=1 HANDLE_UNKNOWN value
             * @property {number} RESOURCE_FILE=2 RESOURCE_FILE value
             * @property {number} RESOURCE_SOCKET=3 RESOURCE_SOCKET value
             * @property {number} RESOURCE_HTTP_RESPONSE=4 RESOURCE_HTTP_RESPONSE value
             * @property {number} FUNCTION_REF=5 FUNCTION_REF value
             * @property {number} ADT_MEDIA_IMAGE=6 ADT_MEDIA_IMAGE value
             * @property {number} ADT_MEDIA_AUDIO=7 ADT_MEDIA_AUDIO value
             * @property {number} ADT_MEDIA_VIDEO=8 ADT_MEDIA_VIDEO value
             * @property {number} ADT_MEDIA_PDF=9 ADT_MEDIA_PDF value
             * @property {number} ADT_MEDIA_GENERIC=10 ADT_MEDIA_GENERIC value
             * @property {number} ADT_PROMPT_AST=11 ADT_PROMPT_AST value
             * @property {number} ADT_COLLECTOR=12 ADT_COLLECTOR value
             * @property {number} ADT_TYPE=13 ADT_TYPE value
             */
            v1.BamlHandleType = (function() {
                var valuesById = {}, values = Object.create(valuesById);
                values[valuesById[0] = "HANDLE_UNSPECIFIED"] = 0;
                values[valuesById[1] = "HANDLE_UNKNOWN"] = 1;
                values[valuesById[2] = "RESOURCE_FILE"] = 2;
                values[valuesById[3] = "RESOURCE_SOCKET"] = 3;
                values[valuesById[4] = "RESOURCE_HTTP_RESPONSE"] = 4;
                values[valuesById[5] = "FUNCTION_REF"] = 5;
                values[valuesById[6] = "ADT_MEDIA_IMAGE"] = 6;
                values[valuesById[7] = "ADT_MEDIA_AUDIO"] = 7;
                values[valuesById[8] = "ADT_MEDIA_VIDEO"] = 8;
                values[valuesById[9] = "ADT_MEDIA_PDF"] = 9;
                values[valuesById[10] = "ADT_MEDIA_GENERIC"] = 10;
                values[valuesById[11] = "ADT_PROMPT_AST"] = 11;
                values[valuesById[12] = "ADT_COLLECTOR"] = 12;
                values[valuesById[13] = "ADT_TYPE"] = 13;
                return values;
            })();

            v1.BamlHandle = (function() {

                /**
                 * Properties of a BamlHandle.
                 * @memberof baml.cffi.v1
                 * @interface IBamlHandle
                 * @property {number|Long|null} [key] BamlHandle key
                 * @property {baml.cffi.v1.BamlHandleType|null} [handleType] BamlHandle handleType
                 */

                /**
                 * Constructs a new BamlHandle.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlHandle.
                 * @implements IBamlHandle
                 * @constructor
                 * @param {baml.cffi.v1.IBamlHandle=} [properties] Properties to set
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
                 * @memberof baml.cffi.v1.BamlHandle
                 * @instance
                 */
                BamlHandle.prototype.key = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * BamlHandle handleType.
                 * @member {baml.cffi.v1.BamlHandleType} handleType
                 * @memberof baml.cffi.v1.BamlHandle
                 * @instance
                 */
                BamlHandle.prototype.handleType = 0;

                /**
                 * Creates a new BamlHandle instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlHandle
                 * @static
                 * @param {baml.cffi.v1.IBamlHandle=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlHandle} BamlHandle instance
                 */
                BamlHandle.create = function create(properties) {
                    return new BamlHandle(properties);
                };

                /**
                 * Encodes the specified BamlHandle message. Does not implicitly {@link baml.cffi.v1.BamlHandle.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlHandle
                 * @static
                 * @param {baml.cffi.v1.IBamlHandle} message BamlHandle message or plain object to encode
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
                 * Encodes the specified BamlHandle message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlHandle.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlHandle
                 * @static
                 * @param {baml.cffi.v1.IBamlHandle} message BamlHandle message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlHandle.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlHandle message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlHandle
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlHandle} BamlHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlHandle.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlHandle();
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
                 * @memberof baml.cffi.v1.BamlHandle
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlHandle} BamlHandle
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
                 * @memberof baml.cffi.v1.BamlHandle
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
                        case 3:
                        case 4:
                        case 5:
                        case 6:
                        case 7:
                        case 8:
                        case 9:
                        case 10:
                        case 11:
                        case 12:
                        case 13:
                            break;
                        }
                    return null;
                };

                /**
                 * Creates a BamlHandle message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlHandle
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlHandle} BamlHandle
                 */
                BamlHandle.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlHandle)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlHandle();
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
                    case "HANDLE_UNKNOWN":
                    case 1:
                        message.handleType = 1;
                        break;
                    case "RESOURCE_FILE":
                    case 2:
                        message.handleType = 2;
                        break;
                    case "RESOURCE_SOCKET":
                    case 3:
                        message.handleType = 3;
                        break;
                    case "RESOURCE_HTTP_RESPONSE":
                    case 4:
                        message.handleType = 4;
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
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlHandle message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlHandle
                 * @static
                 * @param {baml.cffi.v1.BamlHandle} message BamlHandle
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
                        object.handleType = options.enums === String ? $root.baml.cffi.v1.BamlHandleType[message.handleType] === undefined ? message.handleType : $root.baml.cffi.v1.BamlHandleType[message.handleType] : message.handleType;
                    return object;
                };

                /**
                 * Converts this BamlHandle to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlHandle
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlHandle.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlHandle
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlHandle
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlHandle.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlHandle";
                };

                return BamlHandle;
            })();

            v1.InboundValue = (function() {

                /**
                 * Properties of an InboundValue.
                 * @memberof baml.cffi.v1
                 * @interface IInboundValue
                 * @property {string|null} [stringValue] InboundValue stringValue
                 * @property {number|Long|null} [intValue] InboundValue intValue
                 * @property {number|null} [floatValue] InboundValue floatValue
                 * @property {boolean|null} [boolValue] InboundValue boolValue
                 * @property {baml.cffi.v1.IInboundListValue|null} [listValue] InboundValue listValue
                 * @property {baml.cffi.v1.IInboundMapValue|null} [mapValue] InboundValue mapValue
                 * @property {baml.cffi.v1.IInboundClassValue|null} [classValue] InboundValue classValue
                 * @property {baml.cffi.v1.IInboundEnumValue|null} [enumValue] InboundValue enumValue
                 * @property {baml.cffi.v1.IBamlHandle|null} [handle] InboundValue handle
                 * @property {Uint8Array|null} [uint8arrayValue] InboundValue uint8arrayValue
                 */

                /**
                 * Constructs a new InboundValue.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents an InboundValue.
                 * @implements IInboundValue
                 * @constructor
                 * @param {baml.cffi.v1.IInboundValue=} [properties] Properties to set
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
                 * @memberof baml.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.stringValue = null;

                /**
                 * InboundValue intValue.
                 * @member {number|Long|null|undefined} intValue
                 * @memberof baml.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.intValue = null;

                /**
                 * InboundValue floatValue.
                 * @member {number|null|undefined} floatValue
                 * @memberof baml.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.floatValue = null;

                /**
                 * InboundValue boolValue.
                 * @member {boolean|null|undefined} boolValue
                 * @memberof baml.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.boolValue = null;

                /**
                 * InboundValue listValue.
                 * @member {baml.cffi.v1.IInboundListValue|null|undefined} listValue
                 * @memberof baml.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.listValue = null;

                /**
                 * InboundValue mapValue.
                 * @member {baml.cffi.v1.IInboundMapValue|null|undefined} mapValue
                 * @memberof baml.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.mapValue = null;

                /**
                 * InboundValue classValue.
                 * @member {baml.cffi.v1.IInboundClassValue|null|undefined} classValue
                 * @memberof baml.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.classValue = null;

                /**
                 * InboundValue enumValue.
                 * @member {baml.cffi.v1.IInboundEnumValue|null|undefined} enumValue
                 * @memberof baml.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.enumValue = null;

                /**
                 * InboundValue handle.
                 * @member {baml.cffi.v1.IBamlHandle|null|undefined} handle
                 * @memberof baml.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.handle = null;

                /**
                 * InboundValue uint8arrayValue.
                 * @member {Uint8Array|null|undefined} uint8arrayValue
                 * @memberof baml.cffi.v1.InboundValue
                 * @instance
                 */
                InboundValue.prototype.uint8arrayValue = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * InboundValue value.
                 * @member {"stringValue"|"intValue"|"floatValue"|"boolValue"|"listValue"|"mapValue"|"classValue"|"enumValue"|"handle"|"uint8arrayValue"|undefined} value
                 * @memberof baml.cffi.v1.InboundValue
                 * @instance
                 */
                Object.defineProperty(InboundValue.prototype, "value", {
                    get: $util.oneOfGetter($oneOfFields = ["stringValue", "intValue", "floatValue", "boolValue", "listValue", "mapValue", "classValue", "enumValue", "handle", "uint8arrayValue"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new InboundValue instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.InboundValue
                 * @static
                 * @param {baml.cffi.v1.IInboundValue=} [properties] Properties to set
                 * @returns {baml.cffi.v1.InboundValue} InboundValue instance
                 */
                InboundValue.create = function create(properties) {
                    return new InboundValue(properties);
                };

                /**
                 * Encodes the specified InboundValue message. Does not implicitly {@link baml.cffi.v1.InboundValue.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.InboundValue
                 * @static
                 * @param {baml.cffi.v1.IInboundValue} message InboundValue message or plain object to encode
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
                        $root.baml.cffi.v1.InboundListValue.encode(message.listValue, writer.uint32(/* id 6, wireType 2 =*/50).fork()).ldelim();
                    if (message.mapValue != null && Object.hasOwnProperty.call(message, "mapValue"))
                        $root.baml.cffi.v1.InboundMapValue.encode(message.mapValue, writer.uint32(/* id 7, wireType 2 =*/58).fork()).ldelim();
                    if (message.classValue != null && Object.hasOwnProperty.call(message, "classValue"))
                        $root.baml.cffi.v1.InboundClassValue.encode(message.classValue, writer.uint32(/* id 8, wireType 2 =*/66).fork()).ldelim();
                    if (message.enumValue != null && Object.hasOwnProperty.call(message, "enumValue"))
                        $root.baml.cffi.v1.InboundEnumValue.encode(message.enumValue, writer.uint32(/* id 9, wireType 2 =*/74).fork()).ldelim();
                    if (message.handle != null && Object.hasOwnProperty.call(message, "handle"))
                        $root.baml.cffi.v1.BamlHandle.encode(message.handle, writer.uint32(/* id 10, wireType 2 =*/82).fork()).ldelim();
                    if (message.uint8arrayValue != null && Object.hasOwnProperty.call(message, "uint8arrayValue"))
                        writer.uint32(/* id 11, wireType 2 =*/90).bytes(message.uint8arrayValue);
                    return writer;
                };

                /**
                 * Encodes the specified InboundValue message, length delimited. Does not implicitly {@link baml.cffi.v1.InboundValue.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.InboundValue
                 * @static
                 * @param {baml.cffi.v1.IInboundValue} message InboundValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundValue.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an InboundValue message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.InboundValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.InboundValue} InboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundValue.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.InboundValue();
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
                                message.listValue = $root.baml.cffi.v1.InboundListValue.decode(reader, reader.uint32());
                                break;
                            }
                        case 7: {
                                message.mapValue = $root.baml.cffi.v1.InboundMapValue.decode(reader, reader.uint32());
                                break;
                            }
                        case 8: {
                                message.classValue = $root.baml.cffi.v1.InboundClassValue.decode(reader, reader.uint32());
                                break;
                            }
                        case 9: {
                                message.enumValue = $root.baml.cffi.v1.InboundEnumValue.decode(reader, reader.uint32());
                                break;
                            }
                        case 10: {
                                message.handle = $root.baml.cffi.v1.BamlHandle.decode(reader, reader.uint32());
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
                 * @memberof baml.cffi.v1.InboundValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.InboundValue} InboundValue
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
                 * @memberof baml.cffi.v1.InboundValue
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
                            var error = $root.baml.cffi.v1.InboundListValue.verify(message.listValue);
                            if (error)
                                return "listValue." + error;
                        }
                    }
                    if (message.mapValue != null && message.hasOwnProperty("mapValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.InboundMapValue.verify(message.mapValue);
                            if (error)
                                return "mapValue." + error;
                        }
                    }
                    if (message.classValue != null && message.hasOwnProperty("classValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.InboundClassValue.verify(message.classValue);
                            if (error)
                                return "classValue." + error;
                        }
                    }
                    if (message.enumValue != null && message.hasOwnProperty("enumValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.InboundEnumValue.verify(message.enumValue);
                            if (error)
                                return "enumValue." + error;
                        }
                    }
                    if (message.handle != null && message.hasOwnProperty("handle")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlHandle.verify(message.handle);
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
                 * @memberof baml.cffi.v1.InboundValue
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.InboundValue} InboundValue
                 */
                InboundValue.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.InboundValue)
                        return object;
                    var message = new $root.baml.cffi.v1.InboundValue();
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
                            throw TypeError(".baml.cffi.v1.InboundValue.listValue: object expected");
                        message.listValue = $root.baml.cffi.v1.InboundListValue.fromObject(object.listValue);
                    }
                    if (object.mapValue != null) {
                        if (typeof object.mapValue !== "object")
                            throw TypeError(".baml.cffi.v1.InboundValue.mapValue: object expected");
                        message.mapValue = $root.baml.cffi.v1.InboundMapValue.fromObject(object.mapValue);
                    }
                    if (object.classValue != null) {
                        if (typeof object.classValue !== "object")
                            throw TypeError(".baml.cffi.v1.InboundValue.classValue: object expected");
                        message.classValue = $root.baml.cffi.v1.InboundClassValue.fromObject(object.classValue);
                    }
                    if (object.enumValue != null) {
                        if (typeof object.enumValue !== "object")
                            throw TypeError(".baml.cffi.v1.InboundValue.enumValue: object expected");
                        message.enumValue = $root.baml.cffi.v1.InboundEnumValue.fromObject(object.enumValue);
                    }
                    if (object.handle != null) {
                        if (typeof object.handle !== "object")
                            throw TypeError(".baml.cffi.v1.InboundValue.handle: object expected");
                        message.handle = $root.baml.cffi.v1.BamlHandle.fromObject(object.handle);
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
                 * @memberof baml.cffi.v1.InboundValue
                 * @static
                 * @param {baml.cffi.v1.InboundValue} message InboundValue
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
                        object.listValue = $root.baml.cffi.v1.InboundListValue.toObject(message.listValue, options);
                        if (options.oneofs)
                            object.value = "listValue";
                    }
                    if (message.mapValue != null && message.hasOwnProperty("mapValue")) {
                        object.mapValue = $root.baml.cffi.v1.InboundMapValue.toObject(message.mapValue, options);
                        if (options.oneofs)
                            object.value = "mapValue";
                    }
                    if (message.classValue != null && message.hasOwnProperty("classValue")) {
                        object.classValue = $root.baml.cffi.v1.InboundClassValue.toObject(message.classValue, options);
                        if (options.oneofs)
                            object.value = "classValue";
                    }
                    if (message.enumValue != null && message.hasOwnProperty("enumValue")) {
                        object.enumValue = $root.baml.cffi.v1.InboundEnumValue.toObject(message.enumValue, options);
                        if (options.oneofs)
                            object.value = "enumValue";
                    }
                    if (message.handle != null && message.hasOwnProperty("handle")) {
                        object.handle = $root.baml.cffi.v1.BamlHandle.toObject(message.handle, options);
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
                 * @memberof baml.cffi.v1.InboundValue
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                InboundValue.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for InboundValue
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.InboundValue
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                InboundValue.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.InboundValue";
                };

                return InboundValue;
            })();

            v1.InboundListValue = (function() {

                /**
                 * Properties of an InboundListValue.
                 * @memberof baml.cffi.v1
                 * @interface IInboundListValue
                 * @property {Array.<baml.cffi.v1.IInboundValue>|null} [values] InboundListValue values
                 */

                /**
                 * Constructs a new InboundListValue.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents an InboundListValue.
                 * @implements IInboundListValue
                 * @constructor
                 * @param {baml.cffi.v1.IInboundListValue=} [properties] Properties to set
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
                 * @member {Array.<baml.cffi.v1.IInboundValue>} values
                 * @memberof baml.cffi.v1.InboundListValue
                 * @instance
                 */
                InboundListValue.prototype.values = $util.emptyArray;

                /**
                 * Creates a new InboundListValue instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.InboundListValue
                 * @static
                 * @param {baml.cffi.v1.IInboundListValue=} [properties] Properties to set
                 * @returns {baml.cffi.v1.InboundListValue} InboundListValue instance
                 */
                InboundListValue.create = function create(properties) {
                    return new InboundListValue(properties);
                };

                /**
                 * Encodes the specified InboundListValue message. Does not implicitly {@link baml.cffi.v1.InboundListValue.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.InboundListValue
                 * @static
                 * @param {baml.cffi.v1.IInboundListValue} message InboundListValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundListValue.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.values != null && message.values.length)
                        for (var i = 0; i < message.values.length; ++i)
                            $root.baml.cffi.v1.InboundValue.encode(message.values[i], writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified InboundListValue message, length delimited. Does not implicitly {@link baml.cffi.v1.InboundListValue.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.InboundListValue
                 * @static
                 * @param {baml.cffi.v1.IInboundListValue} message InboundListValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundListValue.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an InboundListValue message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.InboundListValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.InboundListValue} InboundListValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundListValue.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.InboundListValue();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                if (!(message.values && message.values.length))
                                    message.values = [];
                                message.values.push($root.baml.cffi.v1.InboundValue.decode(reader, reader.uint32()));
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
                 * @memberof baml.cffi.v1.InboundListValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.InboundListValue} InboundListValue
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
                 * @memberof baml.cffi.v1.InboundListValue
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
                            var error = $root.baml.cffi.v1.InboundValue.verify(message.values[i]);
                            if (error)
                                return "values." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates an InboundListValue message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.InboundListValue
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.InboundListValue} InboundListValue
                 */
                InboundListValue.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.InboundListValue)
                        return object;
                    var message = new $root.baml.cffi.v1.InboundListValue();
                    if (object.values) {
                        if (!Array.isArray(object.values))
                            throw TypeError(".baml.cffi.v1.InboundListValue.values: array expected");
                        message.values = [];
                        for (var i = 0; i < object.values.length; ++i) {
                            if (typeof object.values[i] !== "object")
                                throw TypeError(".baml.cffi.v1.InboundListValue.values: object expected");
                            message.values[i] = $root.baml.cffi.v1.InboundValue.fromObject(object.values[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from an InboundListValue message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.InboundListValue
                 * @static
                 * @param {baml.cffi.v1.InboundListValue} message InboundListValue
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
                            object.values[j] = $root.baml.cffi.v1.InboundValue.toObject(message.values[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this InboundListValue to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.InboundListValue
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                InboundListValue.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for InboundListValue
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.InboundListValue
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                InboundListValue.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.InboundListValue";
                };

                return InboundListValue;
            })();

            v1.InboundMapValue = (function() {

                /**
                 * Properties of an InboundMapValue.
                 * @memberof baml.cffi.v1
                 * @interface IInboundMapValue
                 * @property {Array.<baml.cffi.v1.IInboundMapEntry>|null} [entries] InboundMapValue entries
                 */

                /**
                 * Constructs a new InboundMapValue.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents an InboundMapValue.
                 * @implements IInboundMapValue
                 * @constructor
                 * @param {baml.cffi.v1.IInboundMapValue=} [properties] Properties to set
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
                 * @member {Array.<baml.cffi.v1.IInboundMapEntry>} entries
                 * @memberof baml.cffi.v1.InboundMapValue
                 * @instance
                 */
                InboundMapValue.prototype.entries = $util.emptyArray;

                /**
                 * Creates a new InboundMapValue instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.InboundMapValue
                 * @static
                 * @param {baml.cffi.v1.IInboundMapValue=} [properties] Properties to set
                 * @returns {baml.cffi.v1.InboundMapValue} InboundMapValue instance
                 */
                InboundMapValue.create = function create(properties) {
                    return new InboundMapValue(properties);
                };

                /**
                 * Encodes the specified InboundMapValue message. Does not implicitly {@link baml.cffi.v1.InboundMapValue.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.InboundMapValue
                 * @static
                 * @param {baml.cffi.v1.IInboundMapValue} message InboundMapValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundMapValue.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.entries != null && message.entries.length)
                        for (var i = 0; i < message.entries.length; ++i)
                            $root.baml.cffi.v1.InboundMapEntry.encode(message.entries[i], writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified InboundMapValue message, length delimited. Does not implicitly {@link baml.cffi.v1.InboundMapValue.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.InboundMapValue
                 * @static
                 * @param {baml.cffi.v1.IInboundMapValue} message InboundMapValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundMapValue.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an InboundMapValue message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.InboundMapValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.InboundMapValue} InboundMapValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundMapValue.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.InboundMapValue();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                if (!(message.entries && message.entries.length))
                                    message.entries = [];
                                message.entries.push($root.baml.cffi.v1.InboundMapEntry.decode(reader, reader.uint32()));
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
                 * @memberof baml.cffi.v1.InboundMapValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.InboundMapValue} InboundMapValue
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
                 * @memberof baml.cffi.v1.InboundMapValue
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
                            var error = $root.baml.cffi.v1.InboundMapEntry.verify(message.entries[i]);
                            if (error)
                                return "entries." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates an InboundMapValue message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.InboundMapValue
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.InboundMapValue} InboundMapValue
                 */
                InboundMapValue.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.InboundMapValue)
                        return object;
                    var message = new $root.baml.cffi.v1.InboundMapValue();
                    if (object.entries) {
                        if (!Array.isArray(object.entries))
                            throw TypeError(".baml.cffi.v1.InboundMapValue.entries: array expected");
                        message.entries = [];
                        for (var i = 0; i < object.entries.length; ++i) {
                            if (typeof object.entries[i] !== "object")
                                throw TypeError(".baml.cffi.v1.InboundMapValue.entries: object expected");
                            message.entries[i] = $root.baml.cffi.v1.InboundMapEntry.fromObject(object.entries[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from an InboundMapValue message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.InboundMapValue
                 * @static
                 * @param {baml.cffi.v1.InboundMapValue} message InboundMapValue
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
                            object.entries[j] = $root.baml.cffi.v1.InboundMapEntry.toObject(message.entries[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this InboundMapValue to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.InboundMapValue
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                InboundMapValue.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for InboundMapValue
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.InboundMapValue
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                InboundMapValue.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.InboundMapValue";
                };

                return InboundMapValue;
            })();

            v1.InboundMapEntry = (function() {

                /**
                 * Properties of an InboundMapEntry.
                 * @memberof baml.cffi.v1
                 * @interface IInboundMapEntry
                 * @property {string|null} [stringKey] InboundMapEntry stringKey
                 * @property {number|Long|null} [intKey] InboundMapEntry intKey
                 * @property {boolean|null} [boolKey] InboundMapEntry boolKey
                 * @property {baml.cffi.v1.IInboundEnumValue|null} [enumKey] InboundMapEntry enumKey
                 * @property {baml.cffi.v1.IInboundValue|null} [value] InboundMapEntry value
                 */

                /**
                 * Constructs a new InboundMapEntry.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents an InboundMapEntry.
                 * @implements IInboundMapEntry
                 * @constructor
                 * @param {baml.cffi.v1.IInboundMapEntry=} [properties] Properties to set
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
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @instance
                 */
                InboundMapEntry.prototype.stringKey = null;

                /**
                 * InboundMapEntry intKey.
                 * @member {number|Long|null|undefined} intKey
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @instance
                 */
                InboundMapEntry.prototype.intKey = null;

                /**
                 * InboundMapEntry boolKey.
                 * @member {boolean|null|undefined} boolKey
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @instance
                 */
                InboundMapEntry.prototype.boolKey = null;

                /**
                 * InboundMapEntry enumKey.
                 * @member {baml.cffi.v1.IInboundEnumValue|null|undefined} enumKey
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @instance
                 */
                InboundMapEntry.prototype.enumKey = null;

                /**
                 * InboundMapEntry value.
                 * @member {baml.cffi.v1.IInboundValue|null|undefined} value
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @instance
                 */
                InboundMapEntry.prototype.value = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * InboundMapEntry key.
                 * @member {"stringKey"|"intKey"|"boolKey"|"enumKey"|undefined} key
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @instance
                 */
                Object.defineProperty(InboundMapEntry.prototype, "key", {
                    get: $util.oneOfGetter($oneOfFields = ["stringKey", "intKey", "boolKey", "enumKey"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new InboundMapEntry instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @static
                 * @param {baml.cffi.v1.IInboundMapEntry=} [properties] Properties to set
                 * @returns {baml.cffi.v1.InboundMapEntry} InboundMapEntry instance
                 */
                InboundMapEntry.create = function create(properties) {
                    return new InboundMapEntry(properties);
                };

                /**
                 * Encodes the specified InboundMapEntry message. Does not implicitly {@link baml.cffi.v1.InboundMapEntry.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @static
                 * @param {baml.cffi.v1.IInboundMapEntry} message InboundMapEntry message or plain object to encode
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
                        $root.baml.cffi.v1.InboundEnumValue.encode(message.enumKey, writer.uint32(/* id 5, wireType 2 =*/42).fork()).ldelim();
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        $root.baml.cffi.v1.InboundValue.encode(message.value, writer.uint32(/* id 6, wireType 2 =*/50).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified InboundMapEntry message, length delimited. Does not implicitly {@link baml.cffi.v1.InboundMapEntry.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @static
                 * @param {baml.cffi.v1.IInboundMapEntry} message InboundMapEntry message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundMapEntry.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an InboundMapEntry message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.InboundMapEntry} InboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundMapEntry.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.InboundMapEntry();
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
                                message.enumKey = $root.baml.cffi.v1.InboundEnumValue.decode(reader, reader.uint32());
                                break;
                            }
                        case 6: {
                                message.value = $root.baml.cffi.v1.InboundValue.decode(reader, reader.uint32());
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
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.InboundMapEntry} InboundMapEntry
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
                 * @memberof baml.cffi.v1.InboundMapEntry
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
                            var error = $root.baml.cffi.v1.InboundEnumValue.verify(message.enumKey);
                            if (error)
                                return "enumKey." + error;
                        }
                    }
                    if (message.value != null && message.hasOwnProperty("value")) {
                        var error = $root.baml.cffi.v1.InboundValue.verify(message.value);
                        if (error)
                            return "value." + error;
                    }
                    return null;
                };

                /**
                 * Creates an InboundMapEntry message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.InboundMapEntry} InboundMapEntry
                 */
                InboundMapEntry.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.InboundMapEntry)
                        return object;
                    var message = new $root.baml.cffi.v1.InboundMapEntry();
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
                            throw TypeError(".baml.cffi.v1.InboundMapEntry.enumKey: object expected");
                        message.enumKey = $root.baml.cffi.v1.InboundEnumValue.fromObject(object.enumKey);
                    }
                    if (object.value != null) {
                        if (typeof object.value !== "object")
                            throw TypeError(".baml.cffi.v1.InboundMapEntry.value: object expected");
                        message.value = $root.baml.cffi.v1.InboundValue.fromObject(object.value);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from an InboundMapEntry message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @static
                 * @param {baml.cffi.v1.InboundMapEntry} message InboundMapEntry
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
                        object.enumKey = $root.baml.cffi.v1.InboundEnumValue.toObject(message.enumKey, options);
                        if (options.oneofs)
                            object.key = "enumKey";
                    }
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = $root.baml.cffi.v1.InboundValue.toObject(message.value, options);
                    return object;
                };

                /**
                 * Converts this InboundMapEntry to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                InboundMapEntry.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for InboundMapEntry
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.InboundMapEntry
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                InboundMapEntry.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.InboundMapEntry";
                };

                return InboundMapEntry;
            })();

            v1.InboundClassValue = (function() {

                /**
                 * Properties of an InboundClassValue.
                 * @memberof baml.cffi.v1
                 * @interface IInboundClassValue
                 * @property {string|null} [name] InboundClassValue name
                 * @property {Array.<baml.cffi.v1.IInboundMapEntry>|null} [fields] InboundClassValue fields
                 */

                /**
                 * Constructs a new InboundClassValue.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents an InboundClassValue.
                 * @implements IInboundClassValue
                 * @constructor
                 * @param {baml.cffi.v1.IInboundClassValue=} [properties] Properties to set
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
                 * @memberof baml.cffi.v1.InboundClassValue
                 * @instance
                 */
                InboundClassValue.prototype.name = "";

                /**
                 * InboundClassValue fields.
                 * @member {Array.<baml.cffi.v1.IInboundMapEntry>} fields
                 * @memberof baml.cffi.v1.InboundClassValue
                 * @instance
                 */
                InboundClassValue.prototype.fields = $util.emptyArray;

                /**
                 * Creates a new InboundClassValue instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.InboundClassValue
                 * @static
                 * @param {baml.cffi.v1.IInboundClassValue=} [properties] Properties to set
                 * @returns {baml.cffi.v1.InboundClassValue} InboundClassValue instance
                 */
                InboundClassValue.create = function create(properties) {
                    return new InboundClassValue(properties);
                };

                /**
                 * Encodes the specified InboundClassValue message. Does not implicitly {@link baml.cffi.v1.InboundClassValue.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.InboundClassValue
                 * @static
                 * @param {baml.cffi.v1.IInboundClassValue} message InboundClassValue message or plain object to encode
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
                            $root.baml.cffi.v1.InboundMapEntry.encode(message.fields[i], writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified InboundClassValue message, length delimited. Does not implicitly {@link baml.cffi.v1.InboundClassValue.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.InboundClassValue
                 * @static
                 * @param {baml.cffi.v1.IInboundClassValue} message InboundClassValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundClassValue.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an InboundClassValue message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.InboundClassValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.InboundClassValue} InboundClassValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundClassValue.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.InboundClassValue();
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
                                message.fields.push($root.baml.cffi.v1.InboundMapEntry.decode(reader, reader.uint32()));
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
                 * @memberof baml.cffi.v1.InboundClassValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.InboundClassValue} InboundClassValue
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
                 * @memberof baml.cffi.v1.InboundClassValue
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
                            var error = $root.baml.cffi.v1.InboundMapEntry.verify(message.fields[i]);
                            if (error)
                                return "fields." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates an InboundClassValue message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.InboundClassValue
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.InboundClassValue} InboundClassValue
                 */
                InboundClassValue.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.InboundClassValue)
                        return object;
                    var message = new $root.baml.cffi.v1.InboundClassValue();
                    if (object.name != null)
                        message.name = String(object.name);
                    if (object.fields) {
                        if (!Array.isArray(object.fields))
                            throw TypeError(".baml.cffi.v1.InboundClassValue.fields: array expected");
                        message.fields = [];
                        for (var i = 0; i < object.fields.length; ++i) {
                            if (typeof object.fields[i] !== "object")
                                throw TypeError(".baml.cffi.v1.InboundClassValue.fields: object expected");
                            message.fields[i] = $root.baml.cffi.v1.InboundMapEntry.fromObject(object.fields[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from an InboundClassValue message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.InboundClassValue
                 * @static
                 * @param {baml.cffi.v1.InboundClassValue} message InboundClassValue
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
                            object.fields[j] = $root.baml.cffi.v1.InboundMapEntry.toObject(message.fields[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this InboundClassValue to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.InboundClassValue
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                InboundClassValue.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for InboundClassValue
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.InboundClassValue
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                InboundClassValue.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.InboundClassValue";
                };

                return InboundClassValue;
            })();

            v1.InboundEnumValue = (function() {

                /**
                 * Properties of an InboundEnumValue.
                 * @memberof baml.cffi.v1
                 * @interface IInboundEnumValue
                 * @property {string|null} [name] InboundEnumValue name
                 * @property {string|null} [value] InboundEnumValue value
                 */

                /**
                 * Constructs a new InboundEnumValue.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents an InboundEnumValue.
                 * @implements IInboundEnumValue
                 * @constructor
                 * @param {baml.cffi.v1.IInboundEnumValue=} [properties] Properties to set
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
                 * @memberof baml.cffi.v1.InboundEnumValue
                 * @instance
                 */
                InboundEnumValue.prototype.name = "";

                /**
                 * InboundEnumValue value.
                 * @member {string} value
                 * @memberof baml.cffi.v1.InboundEnumValue
                 * @instance
                 */
                InboundEnumValue.prototype.value = "";

                /**
                 * Creates a new InboundEnumValue instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.InboundEnumValue
                 * @static
                 * @param {baml.cffi.v1.IInboundEnumValue=} [properties] Properties to set
                 * @returns {baml.cffi.v1.InboundEnumValue} InboundEnumValue instance
                 */
                InboundEnumValue.create = function create(properties) {
                    return new InboundEnumValue(properties);
                };

                /**
                 * Encodes the specified InboundEnumValue message. Does not implicitly {@link baml.cffi.v1.InboundEnumValue.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.InboundEnumValue
                 * @static
                 * @param {baml.cffi.v1.IInboundEnumValue} message InboundEnumValue message or plain object to encode
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
                 * Encodes the specified InboundEnumValue message, length delimited. Does not implicitly {@link baml.cffi.v1.InboundEnumValue.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.InboundEnumValue
                 * @static
                 * @param {baml.cffi.v1.IInboundEnumValue} message InboundEnumValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                InboundEnumValue.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an InboundEnumValue message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.InboundEnumValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.InboundEnumValue} InboundEnumValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                InboundEnumValue.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.InboundEnumValue();
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
                 * @memberof baml.cffi.v1.InboundEnumValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.InboundEnumValue} InboundEnumValue
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
                 * @memberof baml.cffi.v1.InboundEnumValue
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
                 * @memberof baml.cffi.v1.InboundEnumValue
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.InboundEnumValue} InboundEnumValue
                 */
                InboundEnumValue.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.InboundEnumValue)
                        return object;
                    var message = new $root.baml.cffi.v1.InboundEnumValue();
                    if (object.name != null)
                        message.name = String(object.name);
                    if (object.value != null)
                        message.value = String(object.value);
                    return message;
                };

                /**
                 * Creates a plain object from an InboundEnumValue message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.InboundEnumValue
                 * @static
                 * @param {baml.cffi.v1.InboundEnumValue} message InboundEnumValue
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
                 * @memberof baml.cffi.v1.InboundEnumValue
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                InboundEnumValue.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for InboundEnumValue
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.InboundEnumValue
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                InboundEnumValue.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.InboundEnumValue";
                };

                return InboundEnumValue;
            })();

            v1.CallFunctionArgs = (function() {

                /**
                 * Properties of a CallFunctionArgs.
                 * @memberof baml.cffi.v1
                 * @interface ICallFunctionArgs
                 * @property {Array.<baml.cffi.v1.IInboundMapEntry>|null} [kwargs] CallFunctionArgs kwargs
                 */

                /**
                 * Constructs a new CallFunctionArgs.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a CallFunctionArgs.
                 * @implements ICallFunctionArgs
                 * @constructor
                 * @param {baml.cffi.v1.ICallFunctionArgs=} [properties] Properties to set
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
                 * @member {Array.<baml.cffi.v1.IInboundMapEntry>} kwargs
                 * @memberof baml.cffi.v1.CallFunctionArgs
                 * @instance
                 */
                CallFunctionArgs.prototype.kwargs = $util.emptyArray;

                /**
                 * Creates a new CallFunctionArgs instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {baml.cffi.v1.ICallFunctionArgs=} [properties] Properties to set
                 * @returns {baml.cffi.v1.CallFunctionArgs} CallFunctionArgs instance
                 */
                CallFunctionArgs.create = function create(properties) {
                    return new CallFunctionArgs(properties);
                };

                /**
                 * Encodes the specified CallFunctionArgs message. Does not implicitly {@link baml.cffi.v1.CallFunctionArgs.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {baml.cffi.v1.ICallFunctionArgs} message CallFunctionArgs message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CallFunctionArgs.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.kwargs != null && message.kwargs.length)
                        for (var i = 0; i < message.kwargs.length; ++i)
                            $root.baml.cffi.v1.InboundMapEntry.encode(message.kwargs[i], writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified CallFunctionArgs message, length delimited. Does not implicitly {@link baml.cffi.v1.CallFunctionArgs.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {baml.cffi.v1.ICallFunctionArgs} message CallFunctionArgs message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CallFunctionArgs.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a CallFunctionArgs message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.CallFunctionArgs} CallFunctionArgs
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CallFunctionArgs.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.CallFunctionArgs();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                if (!(message.kwargs && message.kwargs.length))
                                    message.kwargs = [];
                                message.kwargs.push($root.baml.cffi.v1.InboundMapEntry.decode(reader, reader.uint32()));
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
                 * @memberof baml.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.CallFunctionArgs} CallFunctionArgs
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
                 * @memberof baml.cffi.v1.CallFunctionArgs
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
                            var error = $root.baml.cffi.v1.InboundMapEntry.verify(message.kwargs[i]);
                            if (error)
                                return "kwargs." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a CallFunctionArgs message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.CallFunctionArgs} CallFunctionArgs
                 */
                CallFunctionArgs.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.CallFunctionArgs)
                        return object;
                    var message = new $root.baml.cffi.v1.CallFunctionArgs();
                    if (object.kwargs) {
                        if (!Array.isArray(object.kwargs))
                            throw TypeError(".baml.cffi.v1.CallFunctionArgs.kwargs: array expected");
                        message.kwargs = [];
                        for (var i = 0; i < object.kwargs.length; ++i) {
                            if (typeof object.kwargs[i] !== "object")
                                throw TypeError(".baml.cffi.v1.CallFunctionArgs.kwargs: object expected");
                            message.kwargs[i] = $root.baml.cffi.v1.InboundMapEntry.fromObject(object.kwargs[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a CallFunctionArgs message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {baml.cffi.v1.CallFunctionArgs} message CallFunctionArgs
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
                            object.kwargs[j] = $root.baml.cffi.v1.InboundMapEntry.toObject(message.kwargs[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this CallFunctionArgs to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.CallFunctionArgs
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                CallFunctionArgs.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for CallFunctionArgs
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.CallFunctionArgs
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                CallFunctionArgs.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.CallFunctionArgs";
                };

                return CallFunctionArgs;
            })();

            v1.CallAck = (function() {

                /**
                 * Properties of a CallAck.
                 * @memberof baml.cffi.v1
                 * @interface ICallAck
                 * @property {string|null} [error] CallAck error
                 */

                /**
                 * Constructs a new CallAck.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a CallAck.
                 * @implements ICallAck
                 * @constructor
                 * @param {baml.cffi.v1.ICallAck=} [properties] Properties to set
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
                 * @memberof baml.cffi.v1.CallAck
                 * @instance
                 */
                CallAck.prototype.error = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * CallAck response.
                 * @member {"error"|undefined} response
                 * @memberof baml.cffi.v1.CallAck
                 * @instance
                 */
                Object.defineProperty(CallAck.prototype, "response", {
                    get: $util.oneOfGetter($oneOfFields = ["error"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new CallAck instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.CallAck
                 * @static
                 * @param {baml.cffi.v1.ICallAck=} [properties] Properties to set
                 * @returns {baml.cffi.v1.CallAck} CallAck instance
                 */
                CallAck.create = function create(properties) {
                    return new CallAck(properties);
                };

                /**
                 * Encodes the specified CallAck message. Does not implicitly {@link baml.cffi.v1.CallAck.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.CallAck
                 * @static
                 * @param {baml.cffi.v1.ICallAck} message CallAck message or plain object to encode
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
                 * Encodes the specified CallAck message, length delimited. Does not implicitly {@link baml.cffi.v1.CallAck.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.CallAck
                 * @static
                 * @param {baml.cffi.v1.ICallAck} message CallAck message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CallAck.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a CallAck message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.CallAck
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.CallAck} CallAck
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CallAck.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.CallAck();
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
                 * @memberof baml.cffi.v1.CallAck
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.CallAck} CallAck
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
                 * @memberof baml.cffi.v1.CallAck
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
                 * @memberof baml.cffi.v1.CallAck
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.CallAck} CallAck
                 */
                CallAck.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.CallAck)
                        return object;
                    var message = new $root.baml.cffi.v1.CallAck();
                    if (object.error != null)
                        message.error = String(object.error);
                    return message;
                };

                /**
                 * Creates a plain object from a CallAck message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.CallAck
                 * @static
                 * @param {baml.cffi.v1.CallAck} message CallAck
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
                 * @memberof baml.cffi.v1.CallAck
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                CallAck.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for CallAck
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.CallAck
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                CallAck.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.CallAck";
                };

                return CallAck;
            })();

            v1.BamlOutboundValue = (function() {

                /**
                 * Properties of a BamlOutboundValue.
                 * @memberof baml.cffi.v1
                 * @interface IBamlOutboundValue
                 * @property {baml.cffi.v1.IBamlValueNull|null} [nullValue] BamlOutboundValue nullValue
                 * @property {string|null} [stringValue] BamlOutboundValue stringValue
                 * @property {number|Long|null} [intValue] BamlOutboundValue intValue
                 * @property {number|null} [floatValue] BamlOutboundValue floatValue
                 * @property {boolean|null} [boolValue] BamlOutboundValue boolValue
                 * @property {baml.cffi.v1.IBamlValueClass|null} [classValue] BamlOutboundValue classValue
                 * @property {baml.cffi.v1.IBamlValueEnum|null} [enumValue] BamlOutboundValue enumValue
                 * @property {baml.cffi.v1.IBamlFieldTypeLiteral|null} [literalValue] BamlOutboundValue literalValue
                 * @property {baml.cffi.v1.IBamlValueList|null} [listValue] BamlOutboundValue listValue
                 * @property {baml.cffi.v1.IBamlValueMap|null} [mapValue] BamlOutboundValue mapValue
                 * @property {baml.cffi.v1.IBamlValueUnionVariant|null} [unionVariantValue] BamlOutboundValue unionVariantValue
                 * @property {baml.cffi.v1.IBamlValueChecked|null} [checkedValue] BamlOutboundValue checkedValue
                 * @property {baml.cffi.v1.IBamlValueStreamingState|null} [streamingStateValue] BamlOutboundValue streamingStateValue
                 * @property {baml.cffi.v1.IBamlHandle|null} [handleValue] BamlOutboundValue handleValue
                 * @property {baml.cffi.v1.IBamlValueMedia|null} [mediaValue] BamlOutboundValue mediaValue
                 * @property {baml.cffi.v1.IBamlValuePromptAst|null} [promptAstValue] BamlOutboundValue promptAstValue
                 * @property {Uint8Array|null} [uint8arrayValue] BamlOutboundValue uint8arrayValue
                 */

                /**
                 * Constructs a new BamlOutboundValue.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlOutboundValue.
                 * @implements IBamlOutboundValue
                 * @constructor
                 * @param {baml.cffi.v1.IBamlOutboundValue=} [properties] Properties to set
                 */
                function BamlOutboundValue(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlOutboundValue nullValue.
                 * @member {baml.cffi.v1.IBamlValueNull|null|undefined} nullValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.nullValue = null;

                /**
                 * BamlOutboundValue stringValue.
                 * @member {string|null|undefined} stringValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.stringValue = null;

                /**
                 * BamlOutboundValue intValue.
                 * @member {number|Long|null|undefined} intValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.intValue = null;

                /**
                 * BamlOutboundValue floatValue.
                 * @member {number|null|undefined} floatValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.floatValue = null;

                /**
                 * BamlOutboundValue boolValue.
                 * @member {boolean|null|undefined} boolValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.boolValue = null;

                /**
                 * BamlOutboundValue classValue.
                 * @member {baml.cffi.v1.IBamlValueClass|null|undefined} classValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.classValue = null;

                /**
                 * BamlOutboundValue enumValue.
                 * @member {baml.cffi.v1.IBamlValueEnum|null|undefined} enumValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.enumValue = null;

                /**
                 * BamlOutboundValue literalValue.
                 * @member {baml.cffi.v1.IBamlFieldTypeLiteral|null|undefined} literalValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.literalValue = null;

                /**
                 * BamlOutboundValue listValue.
                 * @member {baml.cffi.v1.IBamlValueList|null|undefined} listValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.listValue = null;

                /**
                 * BamlOutboundValue mapValue.
                 * @member {baml.cffi.v1.IBamlValueMap|null|undefined} mapValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.mapValue = null;

                /**
                 * BamlOutboundValue unionVariantValue.
                 * @member {baml.cffi.v1.IBamlValueUnionVariant|null|undefined} unionVariantValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.unionVariantValue = null;

                /**
                 * BamlOutboundValue checkedValue.
                 * @member {baml.cffi.v1.IBamlValueChecked|null|undefined} checkedValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.checkedValue = null;

                /**
                 * BamlOutboundValue streamingStateValue.
                 * @member {baml.cffi.v1.IBamlValueStreamingState|null|undefined} streamingStateValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.streamingStateValue = null;

                /**
                 * BamlOutboundValue handleValue.
                 * @member {baml.cffi.v1.IBamlHandle|null|undefined} handleValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.handleValue = null;

                /**
                 * BamlOutboundValue mediaValue.
                 * @member {baml.cffi.v1.IBamlValueMedia|null|undefined} mediaValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.mediaValue = null;

                /**
                 * BamlOutboundValue promptAstValue.
                 * @member {baml.cffi.v1.IBamlValuePromptAst|null|undefined} promptAstValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.promptAstValue = null;

                /**
                 * BamlOutboundValue uint8arrayValue.
                 * @member {Uint8Array|null|undefined} uint8arrayValue
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                BamlOutboundValue.prototype.uint8arrayValue = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * BamlOutboundValue value.
                 * @member {"nullValue"|"stringValue"|"intValue"|"floatValue"|"boolValue"|"classValue"|"enumValue"|"literalValue"|"listValue"|"mapValue"|"unionVariantValue"|"checkedValue"|"streamingStateValue"|"handleValue"|"mediaValue"|"promptAstValue"|"uint8arrayValue"|undefined} value
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 */
                Object.defineProperty(BamlOutboundValue.prototype, "value", {
                    get: $util.oneOfGetter($oneOfFields = ["nullValue", "stringValue", "intValue", "floatValue", "boolValue", "classValue", "enumValue", "literalValue", "listValue", "mapValue", "unionVariantValue", "checkedValue", "streamingStateValue", "handleValue", "mediaValue", "promptAstValue", "uint8arrayValue"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new BamlOutboundValue instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {baml.cffi.v1.IBamlOutboundValue=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlOutboundValue} BamlOutboundValue instance
                 */
                BamlOutboundValue.create = function create(properties) {
                    return new BamlOutboundValue(properties);
                };

                /**
                 * Encodes the specified BamlOutboundValue message. Does not implicitly {@link baml.cffi.v1.BamlOutboundValue.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {baml.cffi.v1.IBamlOutboundValue} message BamlOutboundValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlOutboundValue.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.nullValue != null && Object.hasOwnProperty.call(message, "nullValue"))
                        $root.baml.cffi.v1.BamlValueNull.encode(message.nullValue, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.stringValue != null && Object.hasOwnProperty.call(message, "stringValue"))
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.stringValue);
                    if (message.intValue != null && Object.hasOwnProperty.call(message, "intValue"))
                        writer.uint32(/* id 4, wireType 0 =*/32).int64(message.intValue);
                    if (message.floatValue != null && Object.hasOwnProperty.call(message, "floatValue"))
                        writer.uint32(/* id 5, wireType 1 =*/41).double(message.floatValue);
                    if (message.boolValue != null && Object.hasOwnProperty.call(message, "boolValue"))
                        writer.uint32(/* id 6, wireType 0 =*/48).bool(message.boolValue);
                    if (message.classValue != null && Object.hasOwnProperty.call(message, "classValue"))
                        $root.baml.cffi.v1.BamlValueClass.encode(message.classValue, writer.uint32(/* id 7, wireType 2 =*/58).fork()).ldelim();
                    if (message.enumValue != null && Object.hasOwnProperty.call(message, "enumValue"))
                        $root.baml.cffi.v1.BamlValueEnum.encode(message.enumValue, writer.uint32(/* id 8, wireType 2 =*/66).fork()).ldelim();
                    if (message.literalValue != null && Object.hasOwnProperty.call(message, "literalValue"))
                        $root.baml.cffi.v1.BamlFieldTypeLiteral.encode(message.literalValue, writer.uint32(/* id 9, wireType 2 =*/74).fork()).ldelim();
                    if (message.listValue != null && Object.hasOwnProperty.call(message, "listValue"))
                        $root.baml.cffi.v1.BamlValueList.encode(message.listValue, writer.uint32(/* id 11, wireType 2 =*/90).fork()).ldelim();
                    if (message.mapValue != null && Object.hasOwnProperty.call(message, "mapValue"))
                        $root.baml.cffi.v1.BamlValueMap.encode(message.mapValue, writer.uint32(/* id 12, wireType 2 =*/98).fork()).ldelim();
                    if (message.unionVariantValue != null && Object.hasOwnProperty.call(message, "unionVariantValue"))
                        $root.baml.cffi.v1.BamlValueUnionVariant.encode(message.unionVariantValue, writer.uint32(/* id 13, wireType 2 =*/106).fork()).ldelim();
                    if (message.checkedValue != null && Object.hasOwnProperty.call(message, "checkedValue"))
                        $root.baml.cffi.v1.BamlValueChecked.encode(message.checkedValue, writer.uint32(/* id 14, wireType 2 =*/114).fork()).ldelim();
                    if (message.streamingStateValue != null && Object.hasOwnProperty.call(message, "streamingStateValue"))
                        $root.baml.cffi.v1.BamlValueStreamingState.encode(message.streamingStateValue, writer.uint32(/* id 15, wireType 2 =*/122).fork()).ldelim();
                    if (message.handleValue != null && Object.hasOwnProperty.call(message, "handleValue"))
                        $root.baml.cffi.v1.BamlHandle.encode(message.handleValue, writer.uint32(/* id 16, wireType 2 =*/130).fork()).ldelim();
                    if (message.mediaValue != null && Object.hasOwnProperty.call(message, "mediaValue"))
                        $root.baml.cffi.v1.BamlValueMedia.encode(message.mediaValue, writer.uint32(/* id 17, wireType 2 =*/138).fork()).ldelim();
                    if (message.promptAstValue != null && Object.hasOwnProperty.call(message, "promptAstValue"))
                        $root.baml.cffi.v1.BamlValuePromptAst.encode(message.promptAstValue, writer.uint32(/* id 18, wireType 2 =*/146).fork()).ldelim();
                    if (message.uint8arrayValue != null && Object.hasOwnProperty.call(message, "uint8arrayValue"))
                        writer.uint32(/* id 19, wireType 2 =*/154).bytes(message.uint8arrayValue);
                    return writer;
                };

                /**
                 * Encodes the specified BamlOutboundValue message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlOutboundValue.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {baml.cffi.v1.IBamlOutboundValue} message BamlOutboundValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlOutboundValue.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlOutboundValue message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlOutboundValue} BamlOutboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlOutboundValue.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlOutboundValue();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 2: {
                                message.nullValue = $root.baml.cffi.v1.BamlValueNull.decode(reader, reader.uint32());
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
                                message.classValue = $root.baml.cffi.v1.BamlValueClass.decode(reader, reader.uint32());
                                break;
                            }
                        case 8: {
                                message.enumValue = $root.baml.cffi.v1.BamlValueEnum.decode(reader, reader.uint32());
                                break;
                            }
                        case 9: {
                                message.literalValue = $root.baml.cffi.v1.BamlFieldTypeLiteral.decode(reader, reader.uint32());
                                break;
                            }
                        case 11: {
                                message.listValue = $root.baml.cffi.v1.BamlValueList.decode(reader, reader.uint32());
                                break;
                            }
                        case 12: {
                                message.mapValue = $root.baml.cffi.v1.BamlValueMap.decode(reader, reader.uint32());
                                break;
                            }
                        case 13: {
                                message.unionVariantValue = $root.baml.cffi.v1.BamlValueUnionVariant.decode(reader, reader.uint32());
                                break;
                            }
                        case 14: {
                                message.checkedValue = $root.baml.cffi.v1.BamlValueChecked.decode(reader, reader.uint32());
                                break;
                            }
                        case 15: {
                                message.streamingStateValue = $root.baml.cffi.v1.BamlValueStreamingState.decode(reader, reader.uint32());
                                break;
                            }
                        case 16: {
                                message.handleValue = $root.baml.cffi.v1.BamlHandle.decode(reader, reader.uint32());
                                break;
                            }
                        case 17: {
                                message.mediaValue = $root.baml.cffi.v1.BamlValueMedia.decode(reader, reader.uint32());
                                break;
                            }
                        case 18: {
                                message.promptAstValue = $root.baml.cffi.v1.BamlValuePromptAst.decode(reader, reader.uint32());
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
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlOutboundValue} BamlOutboundValue
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
                 * @memberof baml.cffi.v1.BamlOutboundValue
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
                            var error = $root.baml.cffi.v1.BamlValueNull.verify(message.nullValue);
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
                            var error = $root.baml.cffi.v1.BamlValueClass.verify(message.classValue);
                            if (error)
                                return "classValue." + error;
                        }
                    }
                    if (message.enumValue != null && message.hasOwnProperty("enumValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlValueEnum.verify(message.enumValue);
                            if (error)
                                return "enumValue." + error;
                        }
                    }
                    if (message.literalValue != null && message.hasOwnProperty("literalValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeLiteral.verify(message.literalValue);
                            if (error)
                                return "literalValue." + error;
                        }
                    }
                    if (message.listValue != null && message.hasOwnProperty("listValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlValueList.verify(message.listValue);
                            if (error)
                                return "listValue." + error;
                        }
                    }
                    if (message.mapValue != null && message.hasOwnProperty("mapValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlValueMap.verify(message.mapValue);
                            if (error)
                                return "mapValue." + error;
                        }
                    }
                    if (message.unionVariantValue != null && message.hasOwnProperty("unionVariantValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlValueUnionVariant.verify(message.unionVariantValue);
                            if (error)
                                return "unionVariantValue." + error;
                        }
                    }
                    if (message.checkedValue != null && message.hasOwnProperty("checkedValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlValueChecked.verify(message.checkedValue);
                            if (error)
                                return "checkedValue." + error;
                        }
                    }
                    if (message.streamingStateValue != null && message.hasOwnProperty("streamingStateValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlValueStreamingState.verify(message.streamingStateValue);
                            if (error)
                                return "streamingStateValue." + error;
                        }
                    }
                    if (message.handleValue != null && message.hasOwnProperty("handleValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlHandle.verify(message.handleValue);
                            if (error)
                                return "handleValue." + error;
                        }
                    }
                    if (message.mediaValue != null && message.hasOwnProperty("mediaValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlValueMedia.verify(message.mediaValue);
                            if (error)
                                return "mediaValue." + error;
                        }
                    }
                    if (message.promptAstValue != null && message.hasOwnProperty("promptAstValue")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlValuePromptAst.verify(message.promptAstValue);
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
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlOutboundValue} BamlOutboundValue
                 */
                BamlOutboundValue.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlOutboundValue)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlOutboundValue();
                    if (object.nullValue != null) {
                        if (typeof object.nullValue !== "object")
                            throw TypeError(".baml.cffi.v1.BamlOutboundValue.nullValue: object expected");
                        message.nullValue = $root.baml.cffi.v1.BamlValueNull.fromObject(object.nullValue);
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
                            throw TypeError(".baml.cffi.v1.BamlOutboundValue.classValue: object expected");
                        message.classValue = $root.baml.cffi.v1.BamlValueClass.fromObject(object.classValue);
                    }
                    if (object.enumValue != null) {
                        if (typeof object.enumValue !== "object")
                            throw TypeError(".baml.cffi.v1.BamlOutboundValue.enumValue: object expected");
                        message.enumValue = $root.baml.cffi.v1.BamlValueEnum.fromObject(object.enumValue);
                    }
                    if (object.literalValue != null) {
                        if (typeof object.literalValue !== "object")
                            throw TypeError(".baml.cffi.v1.BamlOutboundValue.literalValue: object expected");
                        message.literalValue = $root.baml.cffi.v1.BamlFieldTypeLiteral.fromObject(object.literalValue);
                    }
                    if (object.listValue != null) {
                        if (typeof object.listValue !== "object")
                            throw TypeError(".baml.cffi.v1.BamlOutboundValue.listValue: object expected");
                        message.listValue = $root.baml.cffi.v1.BamlValueList.fromObject(object.listValue);
                    }
                    if (object.mapValue != null) {
                        if (typeof object.mapValue !== "object")
                            throw TypeError(".baml.cffi.v1.BamlOutboundValue.mapValue: object expected");
                        message.mapValue = $root.baml.cffi.v1.BamlValueMap.fromObject(object.mapValue);
                    }
                    if (object.unionVariantValue != null) {
                        if (typeof object.unionVariantValue !== "object")
                            throw TypeError(".baml.cffi.v1.BamlOutboundValue.unionVariantValue: object expected");
                        message.unionVariantValue = $root.baml.cffi.v1.BamlValueUnionVariant.fromObject(object.unionVariantValue);
                    }
                    if (object.checkedValue != null) {
                        if (typeof object.checkedValue !== "object")
                            throw TypeError(".baml.cffi.v1.BamlOutboundValue.checkedValue: object expected");
                        message.checkedValue = $root.baml.cffi.v1.BamlValueChecked.fromObject(object.checkedValue);
                    }
                    if (object.streamingStateValue != null) {
                        if (typeof object.streamingStateValue !== "object")
                            throw TypeError(".baml.cffi.v1.BamlOutboundValue.streamingStateValue: object expected");
                        message.streamingStateValue = $root.baml.cffi.v1.BamlValueStreamingState.fromObject(object.streamingStateValue);
                    }
                    if (object.handleValue != null) {
                        if (typeof object.handleValue !== "object")
                            throw TypeError(".baml.cffi.v1.BamlOutboundValue.handleValue: object expected");
                        message.handleValue = $root.baml.cffi.v1.BamlHandle.fromObject(object.handleValue);
                    }
                    if (object.mediaValue != null) {
                        if (typeof object.mediaValue !== "object")
                            throw TypeError(".baml.cffi.v1.BamlOutboundValue.mediaValue: object expected");
                        message.mediaValue = $root.baml.cffi.v1.BamlValueMedia.fromObject(object.mediaValue);
                    }
                    if (object.promptAstValue != null) {
                        if (typeof object.promptAstValue !== "object")
                            throw TypeError(".baml.cffi.v1.BamlOutboundValue.promptAstValue: object expected");
                        message.promptAstValue = $root.baml.cffi.v1.BamlValuePromptAst.fromObject(object.promptAstValue);
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
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {baml.cffi.v1.BamlOutboundValue} message BamlOutboundValue
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlOutboundValue.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (message.nullValue != null && message.hasOwnProperty("nullValue")) {
                        object.nullValue = $root.baml.cffi.v1.BamlValueNull.toObject(message.nullValue, options);
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
                        object.classValue = $root.baml.cffi.v1.BamlValueClass.toObject(message.classValue, options);
                        if (options.oneofs)
                            object.value = "classValue";
                    }
                    if (message.enumValue != null && message.hasOwnProperty("enumValue")) {
                        object.enumValue = $root.baml.cffi.v1.BamlValueEnum.toObject(message.enumValue, options);
                        if (options.oneofs)
                            object.value = "enumValue";
                    }
                    if (message.literalValue != null && message.hasOwnProperty("literalValue")) {
                        object.literalValue = $root.baml.cffi.v1.BamlFieldTypeLiteral.toObject(message.literalValue, options);
                        if (options.oneofs)
                            object.value = "literalValue";
                    }
                    if (message.listValue != null && message.hasOwnProperty("listValue")) {
                        object.listValue = $root.baml.cffi.v1.BamlValueList.toObject(message.listValue, options);
                        if (options.oneofs)
                            object.value = "listValue";
                    }
                    if (message.mapValue != null && message.hasOwnProperty("mapValue")) {
                        object.mapValue = $root.baml.cffi.v1.BamlValueMap.toObject(message.mapValue, options);
                        if (options.oneofs)
                            object.value = "mapValue";
                    }
                    if (message.unionVariantValue != null && message.hasOwnProperty("unionVariantValue")) {
                        object.unionVariantValue = $root.baml.cffi.v1.BamlValueUnionVariant.toObject(message.unionVariantValue, options);
                        if (options.oneofs)
                            object.value = "unionVariantValue";
                    }
                    if (message.checkedValue != null && message.hasOwnProperty("checkedValue")) {
                        object.checkedValue = $root.baml.cffi.v1.BamlValueChecked.toObject(message.checkedValue, options);
                        if (options.oneofs)
                            object.value = "checkedValue";
                    }
                    if (message.streamingStateValue != null && message.hasOwnProperty("streamingStateValue")) {
                        object.streamingStateValue = $root.baml.cffi.v1.BamlValueStreamingState.toObject(message.streamingStateValue, options);
                        if (options.oneofs)
                            object.value = "streamingStateValue";
                    }
                    if (message.handleValue != null && message.hasOwnProperty("handleValue")) {
                        object.handleValue = $root.baml.cffi.v1.BamlHandle.toObject(message.handleValue, options);
                        if (options.oneofs)
                            object.value = "handleValue";
                    }
                    if (message.mediaValue != null && message.hasOwnProperty("mediaValue")) {
                        object.mediaValue = $root.baml.cffi.v1.BamlValueMedia.toObject(message.mediaValue, options);
                        if (options.oneofs)
                            object.value = "mediaValue";
                    }
                    if (message.promptAstValue != null && message.hasOwnProperty("promptAstValue")) {
                        object.promptAstValue = $root.baml.cffi.v1.BamlValuePromptAst.toObject(message.promptAstValue, options);
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
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlOutboundValue.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlOutboundValue
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlOutboundValue
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlOutboundValue.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlOutboundValue";
                };

                return BamlOutboundValue;
            })();

            /**
             * BamlTypeNamespace enum.
             * @name baml.cffi.v1.BamlTypeNamespace
             * @enum {number}
             * @property {number} INTERNAL=0 INTERNAL value
             * @property {number} TYPES=1 TYPES value
             * @property {number} STREAM_TYPES=2 STREAM_TYPES value
             * @property {number} STREAM_STATE_TYPES=3 STREAM_STATE_TYPES value
             * @property {number} CHECKED_TYPES=4 CHECKED_TYPES value
             */
            v1.BamlTypeNamespace = (function() {
                var valuesById = {}, values = Object.create(valuesById);
                values[valuesById[0] = "INTERNAL"] = 0;
                values[valuesById[1] = "TYPES"] = 1;
                values[valuesById[2] = "STREAM_TYPES"] = 2;
                values[valuesById[3] = "STREAM_STATE_TYPES"] = 3;
                values[valuesById[4] = "CHECKED_TYPES"] = 4;
                return values;
            })();

            v1.BamlTypeName = (function() {

                /**
                 * Properties of a BamlTypeName.
                 * @memberof baml.cffi.v1
                 * @interface IBamlTypeName
                 * @property {baml.cffi.v1.BamlTypeNamespace|null} [namespace] BamlTypeName namespace
                 * @property {string|null} [name] BamlTypeName name
                 */

                /**
                 * Constructs a new BamlTypeName.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlTypeName.
                 * @implements IBamlTypeName
                 * @constructor
                 * @param {baml.cffi.v1.IBamlTypeName=} [properties] Properties to set
                 */
                function BamlTypeName(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlTypeName namespace.
                 * @member {baml.cffi.v1.BamlTypeNamespace} namespace
                 * @memberof baml.cffi.v1.BamlTypeName
                 * @instance
                 */
                BamlTypeName.prototype.namespace = 0;

                /**
                 * BamlTypeName name.
                 * @member {string} name
                 * @memberof baml.cffi.v1.BamlTypeName
                 * @instance
                 */
                BamlTypeName.prototype.name = "";

                /**
                 * Creates a new BamlTypeName instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlTypeName
                 * @static
                 * @param {baml.cffi.v1.IBamlTypeName=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlTypeName} BamlTypeName instance
                 */
                BamlTypeName.create = function create(properties) {
                    return new BamlTypeName(properties);
                };

                /**
                 * Encodes the specified BamlTypeName message. Does not implicitly {@link baml.cffi.v1.BamlTypeName.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlTypeName
                 * @static
                 * @param {baml.cffi.v1.IBamlTypeName} message BamlTypeName message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTypeName.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.namespace != null && Object.hasOwnProperty.call(message, "namespace"))
                        writer.uint32(/* id 1, wireType 0 =*/8).int32(message.namespace);
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.name);
                    return writer;
                };

                /**
                 * Encodes the specified BamlTypeName message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlTypeName.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlTypeName
                 * @static
                 * @param {baml.cffi.v1.IBamlTypeName} message BamlTypeName message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlTypeName.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlTypeName message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlTypeName
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlTypeName} BamlTypeName
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTypeName.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlTypeName();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.namespace = reader.int32();
                                break;
                            }
                        case 2: {
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
                 * Decodes a BamlTypeName message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlTypeName
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlTypeName} BamlTypeName
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlTypeName.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlTypeName message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlTypeName
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlTypeName.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.namespace != null && message.hasOwnProperty("namespace"))
                        switch (message.namespace) {
                        default:
                            return "namespace: enum value expected";
                        case 0:
                        case 1:
                        case 2:
                        case 3:
                        case 4:
                            break;
                        }
                    if (message.name != null && message.hasOwnProperty("name"))
                        if (!$util.isString(message.name))
                            return "name: string expected";
                    return null;
                };

                /**
                 * Creates a BamlTypeName message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlTypeName
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlTypeName} BamlTypeName
                 */
                BamlTypeName.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlTypeName)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlTypeName();
                    switch (object.namespace) {
                    default:
                        if (typeof object.namespace === "number") {
                            message.namespace = object.namespace;
                            break;
                        }
                        break;
                    case "INTERNAL":
                    case 0:
                        message.namespace = 0;
                        break;
                    case "TYPES":
                    case 1:
                        message.namespace = 1;
                        break;
                    case "STREAM_TYPES":
                    case 2:
                        message.namespace = 2;
                        break;
                    case "STREAM_STATE_TYPES":
                    case 3:
                        message.namespace = 3;
                        break;
                    case "CHECKED_TYPES":
                    case 4:
                        message.namespace = 4;
                        break;
                    }
                    if (object.name != null)
                        message.name = String(object.name);
                    return message;
                };

                /**
                 * Creates a plain object from a BamlTypeName message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlTypeName
                 * @static
                 * @param {baml.cffi.v1.BamlTypeName} message BamlTypeName
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlTypeName.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults) {
                        object.namespace = options.enums === String ? "INTERNAL" : 0;
                        object.name = "";
                    }
                    if (message.namespace != null && message.hasOwnProperty("namespace"))
                        object.namespace = options.enums === String ? $root.baml.cffi.v1.BamlTypeNamespace[message.namespace] === undefined ? message.namespace : $root.baml.cffi.v1.BamlTypeNamespace[message.namespace] : message.namespace;
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = message.name;
                    return object;
                };

                /**
                 * Converts this BamlTypeName to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlTypeName
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlTypeName.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlTypeName
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlTypeName
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlTypeName.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlTypeName";
                };

                return BamlTypeName;
            })();

            v1.BamlValueNull = (function() {

                /**
                 * Properties of a BamlValueNull.
                 * @memberof baml.cffi.v1
                 * @interface IBamlValueNull
                 */

                /**
                 * Constructs a new BamlValueNull.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValueNull.
                 * @implements IBamlValueNull
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValueNull=} [properties] Properties to set
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
                 * @memberof baml.cffi.v1.BamlValueNull
                 * @static
                 * @param {baml.cffi.v1.IBamlValueNull=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValueNull} BamlValueNull instance
                 */
                BamlValueNull.create = function create(properties) {
                    return new BamlValueNull(properties);
                };

                /**
                 * Encodes the specified BamlValueNull message. Does not implicitly {@link baml.cffi.v1.BamlValueNull.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValueNull
                 * @static
                 * @param {baml.cffi.v1.IBamlValueNull} message BamlValueNull message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueNull.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueNull message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueNull.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValueNull
                 * @static
                 * @param {baml.cffi.v1.IBamlValueNull} message BamlValueNull message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueNull.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueNull message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValueNull
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValueNull} BamlValueNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueNull.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValueNull();
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
                 * @memberof baml.cffi.v1.BamlValueNull
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValueNull} BamlValueNull
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
                 * @memberof baml.cffi.v1.BamlValueNull
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
                 * @memberof baml.cffi.v1.BamlValueNull
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValueNull} BamlValueNull
                 */
                BamlValueNull.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValueNull)
                        return object;
                    return new $root.baml.cffi.v1.BamlValueNull();
                };

                /**
                 * Creates a plain object from a BamlValueNull message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlValueNull
                 * @static
                 * @param {baml.cffi.v1.BamlValueNull} message BamlValueNull
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValueNull.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlValueNull to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlValueNull
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueNull.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueNull
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValueNull
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueNull.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValueNull";
                };

                return BamlValueNull;
            })();

            v1.BamlValueList = (function() {

                /**
                 * Properties of a BamlValueList.
                 * @memberof baml.cffi.v1
                 * @interface IBamlValueList
                 * @property {baml.cffi.v1.IBamlFieldType|null} [itemType] BamlValueList itemType
                 * @property {Array.<baml.cffi.v1.IBamlOutboundValue>|null} [items] BamlValueList items
                 */

                /**
                 * Constructs a new BamlValueList.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValueList.
                 * @implements IBamlValueList
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValueList=} [properties] Properties to set
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
                 * @member {baml.cffi.v1.IBamlFieldType|null|undefined} itemType
                 * @memberof baml.cffi.v1.BamlValueList
                 * @instance
                 */
                BamlValueList.prototype.itemType = null;

                /**
                 * BamlValueList items.
                 * @member {Array.<baml.cffi.v1.IBamlOutboundValue>} items
                 * @memberof baml.cffi.v1.BamlValueList
                 * @instance
                 */
                BamlValueList.prototype.items = $util.emptyArray;

                /**
                 * Creates a new BamlValueList instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlValueList
                 * @static
                 * @param {baml.cffi.v1.IBamlValueList=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValueList} BamlValueList instance
                 */
                BamlValueList.create = function create(properties) {
                    return new BamlValueList(properties);
                };

                /**
                 * Encodes the specified BamlValueList message. Does not implicitly {@link baml.cffi.v1.BamlValueList.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValueList
                 * @static
                 * @param {baml.cffi.v1.IBamlValueList} message BamlValueList message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueList.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.itemType != null && Object.hasOwnProperty.call(message, "itemType"))
                        $root.baml.cffi.v1.BamlFieldType.encode(message.itemType, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.items != null && message.items.length)
                        for (var i = 0; i < message.items.length; ++i)
                            $root.baml.cffi.v1.BamlOutboundValue.encode(message.items[i], writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueList message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueList.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValueList
                 * @static
                 * @param {baml.cffi.v1.IBamlValueList} message BamlValueList message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueList.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueList message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValueList
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValueList} BamlValueList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueList.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValueList();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.itemType = $root.baml.cffi.v1.BamlFieldType.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                if (!(message.items && message.items.length))
                                    message.items = [];
                                message.items.push($root.baml.cffi.v1.BamlOutboundValue.decode(reader, reader.uint32()));
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
                 * @memberof baml.cffi.v1.BamlValueList
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValueList} BamlValueList
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
                 * @memberof baml.cffi.v1.BamlValueList
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueList.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.itemType != null && message.hasOwnProperty("itemType")) {
                        var error = $root.baml.cffi.v1.BamlFieldType.verify(message.itemType);
                        if (error)
                            return "itemType." + error;
                    }
                    if (message.items != null && message.hasOwnProperty("items")) {
                        if (!Array.isArray(message.items))
                            return "items: array expected";
                        for (var i = 0; i < message.items.length; ++i) {
                            var error = $root.baml.cffi.v1.BamlOutboundValue.verify(message.items[i]);
                            if (error)
                                return "items." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValueList message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlValueList
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValueList} BamlValueList
                 */
                BamlValueList.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValueList)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlValueList();
                    if (object.itemType != null) {
                        if (typeof object.itemType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValueList.itemType: object expected");
                        message.itemType = $root.baml.cffi.v1.BamlFieldType.fromObject(object.itemType);
                    }
                    if (object.items) {
                        if (!Array.isArray(object.items))
                            throw TypeError(".baml.cffi.v1.BamlValueList.items: array expected");
                        message.items = [];
                        for (var i = 0; i < object.items.length; ++i) {
                            if (typeof object.items[i] !== "object")
                                throw TypeError(".baml.cffi.v1.BamlValueList.items: object expected");
                            message.items[i] = $root.baml.cffi.v1.BamlOutboundValue.fromObject(object.items[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValueList message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlValueList
                 * @static
                 * @param {baml.cffi.v1.BamlValueList} message BamlValueList
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
                        object.itemType = $root.baml.cffi.v1.BamlFieldType.toObject(message.itemType, options);
                    if (message.items && message.items.length) {
                        object.items = [];
                        for (var j = 0; j < message.items.length; ++j)
                            object.items[j] = $root.baml.cffi.v1.BamlOutboundValue.toObject(message.items[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this BamlValueList to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlValueList
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueList.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueList
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValueList
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueList.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValueList";
                };

                return BamlValueList;
            })();

            v1.BamlOutboundMapEntry = (function() {

                /**
                 * Properties of a BamlOutboundMapEntry.
                 * @memberof baml.cffi.v1
                 * @interface IBamlOutboundMapEntry
                 * @property {string|null} [key] BamlOutboundMapEntry key
                 * @property {baml.cffi.v1.IBamlOutboundValue|null} [value] BamlOutboundMapEntry value
                 */

                /**
                 * Constructs a new BamlOutboundMapEntry.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlOutboundMapEntry.
                 * @implements IBamlOutboundMapEntry
                 * @constructor
                 * @param {baml.cffi.v1.IBamlOutboundMapEntry=} [properties] Properties to set
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
                 * @memberof baml.cffi.v1.BamlOutboundMapEntry
                 * @instance
                 */
                BamlOutboundMapEntry.prototype.key = "";

                /**
                 * BamlOutboundMapEntry value.
                 * @member {baml.cffi.v1.IBamlOutboundValue|null|undefined} value
                 * @memberof baml.cffi.v1.BamlOutboundMapEntry
                 * @instance
                 */
                BamlOutboundMapEntry.prototype.value = null;

                /**
                 * Creates a new BamlOutboundMapEntry instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {baml.cffi.v1.IBamlOutboundMapEntry=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlOutboundMapEntry} BamlOutboundMapEntry instance
                 */
                BamlOutboundMapEntry.create = function create(properties) {
                    return new BamlOutboundMapEntry(properties);
                };

                /**
                 * Encodes the specified BamlOutboundMapEntry message. Does not implicitly {@link baml.cffi.v1.BamlOutboundMapEntry.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {baml.cffi.v1.IBamlOutboundMapEntry} message BamlOutboundMapEntry message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlOutboundMapEntry.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.key != null && Object.hasOwnProperty.call(message, "key"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.key);
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        $root.baml.cffi.v1.BamlOutboundValue.encode(message.value, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlOutboundMapEntry message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlOutboundMapEntry.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {baml.cffi.v1.IBamlOutboundMapEntry} message BamlOutboundMapEntry message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlOutboundMapEntry.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlOutboundMapEntry message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlOutboundMapEntry} BamlOutboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlOutboundMapEntry.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlOutboundMapEntry();
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
                                message.value = $root.baml.cffi.v1.BamlOutboundValue.decode(reader, reader.uint32());
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
                 * @memberof baml.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlOutboundMapEntry} BamlOutboundMapEntry
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
                 * @memberof baml.cffi.v1.BamlOutboundMapEntry
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
                        var error = $root.baml.cffi.v1.BamlOutboundValue.verify(message.value);
                        if (error)
                            return "value." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlOutboundMapEntry message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlOutboundMapEntry} BamlOutboundMapEntry
                 */
                BamlOutboundMapEntry.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlOutboundMapEntry)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlOutboundMapEntry();
                    if (object.key != null)
                        message.key = String(object.key);
                    if (object.value != null) {
                        if (typeof object.value !== "object")
                            throw TypeError(".baml.cffi.v1.BamlOutboundMapEntry.value: object expected");
                        message.value = $root.baml.cffi.v1.BamlOutboundValue.fromObject(object.value);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlOutboundMapEntry message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {baml.cffi.v1.BamlOutboundMapEntry} message BamlOutboundMapEntry
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
                        object.value = $root.baml.cffi.v1.BamlOutboundValue.toObject(message.value, options);
                    return object;
                };

                /**
                 * Converts this BamlOutboundMapEntry to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlOutboundMapEntry
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlOutboundMapEntry.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlOutboundMapEntry
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlOutboundMapEntry
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlOutboundMapEntry.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlOutboundMapEntry";
                };

                return BamlOutboundMapEntry;
            })();

            v1.BamlValueMap = (function() {

                /**
                 * Properties of a BamlValueMap.
                 * @memberof baml.cffi.v1
                 * @interface IBamlValueMap
                 * @property {baml.cffi.v1.IBamlFieldType|null} [keyType] BamlValueMap keyType
                 * @property {baml.cffi.v1.IBamlFieldType|null} [valueType] BamlValueMap valueType
                 * @property {Array.<baml.cffi.v1.IBamlOutboundMapEntry>|null} [entries] BamlValueMap entries
                 */

                /**
                 * Constructs a new BamlValueMap.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValueMap.
                 * @implements IBamlValueMap
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValueMap=} [properties] Properties to set
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
                 * @member {baml.cffi.v1.IBamlFieldType|null|undefined} keyType
                 * @memberof baml.cffi.v1.BamlValueMap
                 * @instance
                 */
                BamlValueMap.prototype.keyType = null;

                /**
                 * BamlValueMap valueType.
                 * @member {baml.cffi.v1.IBamlFieldType|null|undefined} valueType
                 * @memberof baml.cffi.v1.BamlValueMap
                 * @instance
                 */
                BamlValueMap.prototype.valueType = null;

                /**
                 * BamlValueMap entries.
                 * @member {Array.<baml.cffi.v1.IBamlOutboundMapEntry>} entries
                 * @memberof baml.cffi.v1.BamlValueMap
                 * @instance
                 */
                BamlValueMap.prototype.entries = $util.emptyArray;

                /**
                 * Creates a new BamlValueMap instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlValueMap
                 * @static
                 * @param {baml.cffi.v1.IBamlValueMap=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValueMap} BamlValueMap instance
                 */
                BamlValueMap.create = function create(properties) {
                    return new BamlValueMap(properties);
                };

                /**
                 * Encodes the specified BamlValueMap message. Does not implicitly {@link baml.cffi.v1.BamlValueMap.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValueMap
                 * @static
                 * @param {baml.cffi.v1.IBamlValueMap} message BamlValueMap message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueMap.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.keyType != null && Object.hasOwnProperty.call(message, "keyType"))
                        $root.baml.cffi.v1.BamlFieldType.encode(message.keyType, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.valueType != null && Object.hasOwnProperty.call(message, "valueType"))
                        $root.baml.cffi.v1.BamlFieldType.encode(message.valueType, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.entries != null && message.entries.length)
                        for (var i = 0; i < message.entries.length; ++i)
                            $root.baml.cffi.v1.BamlOutboundMapEntry.encode(message.entries[i], writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueMap message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueMap.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValueMap
                 * @static
                 * @param {baml.cffi.v1.IBamlValueMap} message BamlValueMap message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueMap.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueMap message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValueMap
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValueMap} BamlValueMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueMap.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValueMap();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.keyType = $root.baml.cffi.v1.BamlFieldType.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.valueType = $root.baml.cffi.v1.BamlFieldType.decode(reader, reader.uint32());
                                break;
                            }
                        case 3: {
                                if (!(message.entries && message.entries.length))
                                    message.entries = [];
                                message.entries.push($root.baml.cffi.v1.BamlOutboundMapEntry.decode(reader, reader.uint32()));
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
                 * @memberof baml.cffi.v1.BamlValueMap
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValueMap} BamlValueMap
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
                 * @memberof baml.cffi.v1.BamlValueMap
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueMap.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.keyType != null && message.hasOwnProperty("keyType")) {
                        var error = $root.baml.cffi.v1.BamlFieldType.verify(message.keyType);
                        if (error)
                            return "keyType." + error;
                    }
                    if (message.valueType != null && message.hasOwnProperty("valueType")) {
                        var error = $root.baml.cffi.v1.BamlFieldType.verify(message.valueType);
                        if (error)
                            return "valueType." + error;
                    }
                    if (message.entries != null && message.hasOwnProperty("entries")) {
                        if (!Array.isArray(message.entries))
                            return "entries: array expected";
                        for (var i = 0; i < message.entries.length; ++i) {
                            var error = $root.baml.cffi.v1.BamlOutboundMapEntry.verify(message.entries[i]);
                            if (error)
                                return "entries." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValueMap message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlValueMap
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValueMap} BamlValueMap
                 */
                BamlValueMap.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValueMap)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlValueMap();
                    if (object.keyType != null) {
                        if (typeof object.keyType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValueMap.keyType: object expected");
                        message.keyType = $root.baml.cffi.v1.BamlFieldType.fromObject(object.keyType);
                    }
                    if (object.valueType != null) {
                        if (typeof object.valueType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValueMap.valueType: object expected");
                        message.valueType = $root.baml.cffi.v1.BamlFieldType.fromObject(object.valueType);
                    }
                    if (object.entries) {
                        if (!Array.isArray(object.entries))
                            throw TypeError(".baml.cffi.v1.BamlValueMap.entries: array expected");
                        message.entries = [];
                        for (var i = 0; i < object.entries.length; ++i) {
                            if (typeof object.entries[i] !== "object")
                                throw TypeError(".baml.cffi.v1.BamlValueMap.entries: object expected");
                            message.entries[i] = $root.baml.cffi.v1.BamlOutboundMapEntry.fromObject(object.entries[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValueMap message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlValueMap
                 * @static
                 * @param {baml.cffi.v1.BamlValueMap} message BamlValueMap
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
                        object.keyType = $root.baml.cffi.v1.BamlFieldType.toObject(message.keyType, options);
                    if (message.valueType != null && message.hasOwnProperty("valueType"))
                        object.valueType = $root.baml.cffi.v1.BamlFieldType.toObject(message.valueType, options);
                    if (message.entries && message.entries.length) {
                        object.entries = [];
                        for (var j = 0; j < message.entries.length; ++j)
                            object.entries[j] = $root.baml.cffi.v1.BamlOutboundMapEntry.toObject(message.entries[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this BamlValueMap to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlValueMap
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueMap.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueMap
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValueMap
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueMap.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValueMap";
                };

                return BamlValueMap;
            })();

            v1.BamlValueClass = (function() {

                /**
                 * Properties of a BamlValueClass.
                 * @memberof baml.cffi.v1
                 * @interface IBamlValueClass
                 * @property {baml.cffi.v1.IBamlTypeName|null} [name] BamlValueClass name
                 * @property {Array.<baml.cffi.v1.IBamlOutboundMapEntry>|null} [fields] BamlValueClass fields
                 */

                /**
                 * Constructs a new BamlValueClass.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValueClass.
                 * @implements IBamlValueClass
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValueClass=} [properties] Properties to set
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
                 * @member {baml.cffi.v1.IBamlTypeName|null|undefined} name
                 * @memberof baml.cffi.v1.BamlValueClass
                 * @instance
                 */
                BamlValueClass.prototype.name = null;

                /**
                 * BamlValueClass fields.
                 * @member {Array.<baml.cffi.v1.IBamlOutboundMapEntry>} fields
                 * @memberof baml.cffi.v1.BamlValueClass
                 * @instance
                 */
                BamlValueClass.prototype.fields = $util.emptyArray;

                /**
                 * Creates a new BamlValueClass instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlValueClass
                 * @static
                 * @param {baml.cffi.v1.IBamlValueClass=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValueClass} BamlValueClass instance
                 */
                BamlValueClass.create = function create(properties) {
                    return new BamlValueClass(properties);
                };

                /**
                 * Encodes the specified BamlValueClass message. Does not implicitly {@link baml.cffi.v1.BamlValueClass.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValueClass
                 * @static
                 * @param {baml.cffi.v1.IBamlValueClass} message BamlValueClass message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueClass.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml.cffi.v1.BamlTypeName.encode(message.name, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.fields != null && message.fields.length)
                        for (var i = 0; i < message.fields.length; ++i)
                            $root.baml.cffi.v1.BamlOutboundMapEntry.encode(message.fields[i], writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueClass message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueClass.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValueClass
                 * @static
                 * @param {baml.cffi.v1.IBamlValueClass} message BamlValueClass message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueClass.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueClass message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValueClass
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValueClass} BamlValueClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueClass.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValueClass();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = $root.baml.cffi.v1.BamlTypeName.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                if (!(message.fields && message.fields.length))
                                    message.fields = [];
                                message.fields.push($root.baml.cffi.v1.BamlOutboundMapEntry.decode(reader, reader.uint32()));
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
                 * @memberof baml.cffi.v1.BamlValueClass
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValueClass} BamlValueClass
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
                 * @memberof baml.cffi.v1.BamlValueClass
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueClass.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml.cffi.v1.BamlTypeName.verify(message.name);
                        if (error)
                            return "name." + error;
                    }
                    if (message.fields != null && message.hasOwnProperty("fields")) {
                        if (!Array.isArray(message.fields))
                            return "fields: array expected";
                        for (var i = 0; i < message.fields.length; ++i) {
                            var error = $root.baml.cffi.v1.BamlOutboundMapEntry.verify(message.fields[i]);
                            if (error)
                                return "fields." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValueClass message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlValueClass
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValueClass} BamlValueClass
                 */
                BamlValueClass.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValueClass)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlValueClass();
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValueClass.name: object expected");
                        message.name = $root.baml.cffi.v1.BamlTypeName.fromObject(object.name);
                    }
                    if (object.fields) {
                        if (!Array.isArray(object.fields))
                            throw TypeError(".baml.cffi.v1.BamlValueClass.fields: array expected");
                        message.fields = [];
                        for (var i = 0; i < object.fields.length; ++i) {
                            if (typeof object.fields[i] !== "object")
                                throw TypeError(".baml.cffi.v1.BamlValueClass.fields: object expected");
                            message.fields[i] = $root.baml.cffi.v1.BamlOutboundMapEntry.fromObject(object.fields[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValueClass message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlValueClass
                 * @static
                 * @param {baml.cffi.v1.BamlValueClass} message BamlValueClass
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
                        object.name = $root.baml.cffi.v1.BamlTypeName.toObject(message.name, options);
                    if (message.fields && message.fields.length) {
                        object.fields = [];
                        for (var j = 0; j < message.fields.length; ++j)
                            object.fields[j] = $root.baml.cffi.v1.BamlOutboundMapEntry.toObject(message.fields[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this BamlValueClass to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlValueClass
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueClass.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueClass
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValueClass
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueClass.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValueClass";
                };

                return BamlValueClass;
            })();

            v1.BamlValueEnum = (function() {

                /**
                 * Properties of a BamlValueEnum.
                 * @memberof baml.cffi.v1
                 * @interface IBamlValueEnum
                 * @property {baml.cffi.v1.IBamlTypeName|null} [name] BamlValueEnum name
                 * @property {string|null} [value] BamlValueEnum value
                 * @property {boolean|null} [isDynamic] BamlValueEnum isDynamic
                 */

                /**
                 * Constructs a new BamlValueEnum.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValueEnum.
                 * @implements IBamlValueEnum
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValueEnum=} [properties] Properties to set
                 */
                function BamlValueEnum(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValueEnum name.
                 * @member {baml.cffi.v1.IBamlTypeName|null|undefined} name
                 * @memberof baml.cffi.v1.BamlValueEnum
                 * @instance
                 */
                BamlValueEnum.prototype.name = null;

                /**
                 * BamlValueEnum value.
                 * @member {string} value
                 * @memberof baml.cffi.v1.BamlValueEnum
                 * @instance
                 */
                BamlValueEnum.prototype.value = "";

                /**
                 * BamlValueEnum isDynamic.
                 * @member {boolean} isDynamic
                 * @memberof baml.cffi.v1.BamlValueEnum
                 * @instance
                 */
                BamlValueEnum.prototype.isDynamic = false;

                /**
                 * Creates a new BamlValueEnum instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlValueEnum
                 * @static
                 * @param {baml.cffi.v1.IBamlValueEnum=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValueEnum} BamlValueEnum instance
                 */
                BamlValueEnum.create = function create(properties) {
                    return new BamlValueEnum(properties);
                };

                /**
                 * Encodes the specified BamlValueEnum message. Does not implicitly {@link baml.cffi.v1.BamlValueEnum.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValueEnum
                 * @static
                 * @param {baml.cffi.v1.IBamlValueEnum} message BamlValueEnum message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueEnum.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml.cffi.v1.BamlTypeName.encode(message.name, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.value);
                    if (message.isDynamic != null && Object.hasOwnProperty.call(message, "isDynamic"))
                        writer.uint32(/* id 3, wireType 0 =*/24).bool(message.isDynamic);
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueEnum message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueEnum.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValueEnum
                 * @static
                 * @param {baml.cffi.v1.IBamlValueEnum} message BamlValueEnum message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueEnum.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueEnum message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValueEnum
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValueEnum} BamlValueEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueEnum.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValueEnum();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = $root.baml.cffi.v1.BamlTypeName.decode(reader, reader.uint32());
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
                 * @memberof baml.cffi.v1.BamlValueEnum
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValueEnum} BamlValueEnum
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
                 * @memberof baml.cffi.v1.BamlValueEnum
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueEnum.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml.cffi.v1.BamlTypeName.verify(message.name);
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
                 * @memberof baml.cffi.v1.BamlValueEnum
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValueEnum} BamlValueEnum
                 */
                BamlValueEnum.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValueEnum)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlValueEnum();
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValueEnum.name: object expected");
                        message.name = $root.baml.cffi.v1.BamlTypeName.fromObject(object.name);
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
                 * @memberof baml.cffi.v1.BamlValueEnum
                 * @static
                 * @param {baml.cffi.v1.BamlValueEnum} message BamlValueEnum
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
                        object.name = $root.baml.cffi.v1.BamlTypeName.toObject(message.name, options);
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = message.value;
                    if (message.isDynamic != null && message.hasOwnProperty("isDynamic"))
                        object.isDynamic = message.isDynamic;
                    return object;
                };

                /**
                 * Converts this BamlValueEnum to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlValueEnum
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueEnum.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueEnum
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValueEnum
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueEnum.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValueEnum";
                };

                return BamlValueEnum;
            })();

            v1.BamlValueUnionVariant = (function() {

                /**
                 * Properties of a BamlValueUnionVariant.
                 * @memberof baml.cffi.v1
                 * @interface IBamlValueUnionVariant
                 * @property {baml.cffi.v1.IBamlTypeName|null} [name] BamlValueUnionVariant name
                 * @property {boolean|null} [isOptional] BamlValueUnionVariant isOptional
                 * @property {boolean|null} [isSinglePattern] BamlValueUnionVariant isSinglePattern
                 * @property {baml.cffi.v1.IBamlFieldType|null} [selfType] BamlValueUnionVariant selfType
                 * @property {string|null} [valueOptionName] BamlValueUnionVariant valueOptionName
                 * @property {baml.cffi.v1.IBamlOutboundValue|null} [value] BamlValueUnionVariant value
                 */

                /**
                 * Constructs a new BamlValueUnionVariant.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValueUnionVariant.
                 * @implements IBamlValueUnionVariant
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValueUnionVariant=} [properties] Properties to set
                 */
                function BamlValueUnionVariant(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValueUnionVariant name.
                 * @member {baml.cffi.v1.IBamlTypeName|null|undefined} name
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @instance
                 */
                BamlValueUnionVariant.prototype.name = null;

                /**
                 * BamlValueUnionVariant isOptional.
                 * @member {boolean} isOptional
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @instance
                 */
                BamlValueUnionVariant.prototype.isOptional = false;

                /**
                 * BamlValueUnionVariant isSinglePattern.
                 * @member {boolean} isSinglePattern
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @instance
                 */
                BamlValueUnionVariant.prototype.isSinglePattern = false;

                /**
                 * BamlValueUnionVariant selfType.
                 * @member {baml.cffi.v1.IBamlFieldType|null|undefined} selfType
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @instance
                 */
                BamlValueUnionVariant.prototype.selfType = null;

                /**
                 * BamlValueUnionVariant valueOptionName.
                 * @member {string} valueOptionName
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @instance
                 */
                BamlValueUnionVariant.prototype.valueOptionName = "";

                /**
                 * BamlValueUnionVariant value.
                 * @member {baml.cffi.v1.IBamlOutboundValue|null|undefined} value
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @instance
                 */
                BamlValueUnionVariant.prototype.value = null;

                /**
                 * Creates a new BamlValueUnionVariant instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {baml.cffi.v1.IBamlValueUnionVariant=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValueUnionVariant} BamlValueUnionVariant instance
                 */
                BamlValueUnionVariant.create = function create(properties) {
                    return new BamlValueUnionVariant(properties);
                };

                /**
                 * Encodes the specified BamlValueUnionVariant message. Does not implicitly {@link baml.cffi.v1.BamlValueUnionVariant.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {baml.cffi.v1.IBamlValueUnionVariant} message BamlValueUnionVariant message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueUnionVariant.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml.cffi.v1.BamlTypeName.encode(message.name, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.isOptional != null && Object.hasOwnProperty.call(message, "isOptional"))
                        writer.uint32(/* id 2, wireType 0 =*/16).bool(message.isOptional);
                    if (message.isSinglePattern != null && Object.hasOwnProperty.call(message, "isSinglePattern"))
                        writer.uint32(/* id 3, wireType 0 =*/24).bool(message.isSinglePattern);
                    if (message.selfType != null && Object.hasOwnProperty.call(message, "selfType"))
                        $root.baml.cffi.v1.BamlFieldType.encode(message.selfType, writer.uint32(/* id 4, wireType 2 =*/34).fork()).ldelim();
                    if (message.valueOptionName != null && Object.hasOwnProperty.call(message, "valueOptionName"))
                        writer.uint32(/* id 5, wireType 2 =*/42).string(message.valueOptionName);
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        $root.baml.cffi.v1.BamlOutboundValue.encode(message.value, writer.uint32(/* id 6, wireType 2 =*/50).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueUnionVariant message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueUnionVariant.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {baml.cffi.v1.IBamlValueUnionVariant} message BamlValueUnionVariant message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueUnionVariant.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueUnionVariant message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValueUnionVariant} BamlValueUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueUnionVariant.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValueUnionVariant();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = $root.baml.cffi.v1.BamlTypeName.decode(reader, reader.uint32());
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
                                message.selfType = $root.baml.cffi.v1.BamlFieldType.decode(reader, reader.uint32());
                                break;
                            }
                        case 5: {
                                message.valueOptionName = reader.string();
                                break;
                            }
                        case 6: {
                                message.value = $root.baml.cffi.v1.BamlOutboundValue.decode(reader, reader.uint32());
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
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValueUnionVariant} BamlValueUnionVariant
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
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueUnionVariant.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml.cffi.v1.BamlTypeName.verify(message.name);
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
                        var error = $root.baml.cffi.v1.BamlFieldType.verify(message.selfType);
                        if (error)
                            return "selfType." + error;
                    }
                    if (message.valueOptionName != null && message.hasOwnProperty("valueOptionName"))
                        if (!$util.isString(message.valueOptionName))
                            return "valueOptionName: string expected";
                    if (message.value != null && message.hasOwnProperty("value")) {
                        var error = $root.baml.cffi.v1.BamlOutboundValue.verify(message.value);
                        if (error)
                            return "value." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlValueUnionVariant message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValueUnionVariant} BamlValueUnionVariant
                 */
                BamlValueUnionVariant.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValueUnionVariant)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlValueUnionVariant();
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValueUnionVariant.name: object expected");
                        message.name = $root.baml.cffi.v1.BamlTypeName.fromObject(object.name);
                    }
                    if (object.isOptional != null)
                        message.isOptional = Boolean(object.isOptional);
                    if (object.isSinglePattern != null)
                        message.isSinglePattern = Boolean(object.isSinglePattern);
                    if (object.selfType != null) {
                        if (typeof object.selfType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValueUnionVariant.selfType: object expected");
                        message.selfType = $root.baml.cffi.v1.BamlFieldType.fromObject(object.selfType);
                    }
                    if (object.valueOptionName != null)
                        message.valueOptionName = String(object.valueOptionName);
                    if (object.value != null) {
                        if (typeof object.value !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValueUnionVariant.value: object expected");
                        message.value = $root.baml.cffi.v1.BamlOutboundValue.fromObject(object.value);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValueUnionVariant message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {baml.cffi.v1.BamlValueUnionVariant} message BamlValueUnionVariant
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
                        object.name = $root.baml.cffi.v1.BamlTypeName.toObject(message.name, options);
                    if (message.isOptional != null && message.hasOwnProperty("isOptional"))
                        object.isOptional = message.isOptional;
                    if (message.isSinglePattern != null && message.hasOwnProperty("isSinglePattern"))
                        object.isSinglePattern = message.isSinglePattern;
                    if (message.selfType != null && message.hasOwnProperty("selfType"))
                        object.selfType = $root.baml.cffi.v1.BamlFieldType.toObject(message.selfType, options);
                    if (message.valueOptionName != null && message.hasOwnProperty("valueOptionName"))
                        object.valueOptionName = message.valueOptionName;
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = $root.baml.cffi.v1.BamlOutboundValue.toObject(message.value, options);
                    return object;
                };

                /**
                 * Converts this BamlValueUnionVariant to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueUnionVariant.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueUnionVariant
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValueUnionVariant
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueUnionVariant.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValueUnionVariant";
                };

                return BamlValueUnionVariant;
            })();

            v1.BamlValueChecked = (function() {

                /**
                 * Properties of a BamlValueChecked.
                 * @memberof baml.cffi.v1
                 * @interface IBamlValueChecked
                 * @property {baml.cffi.v1.IBamlTypeName|null} [name] BamlValueChecked name
                 * @property {baml.cffi.v1.IBamlOutboundValue|null} [value] BamlValueChecked value
                 * @property {Array.<baml.cffi.v1.IBamlCheckValue>|null} [checks] BamlValueChecked checks
                 */

                /**
                 * Constructs a new BamlValueChecked.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValueChecked.
                 * @implements IBamlValueChecked
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValueChecked=} [properties] Properties to set
                 */
                function BamlValueChecked(properties) {
                    this.checks = [];
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValueChecked name.
                 * @member {baml.cffi.v1.IBamlTypeName|null|undefined} name
                 * @memberof baml.cffi.v1.BamlValueChecked
                 * @instance
                 */
                BamlValueChecked.prototype.name = null;

                /**
                 * BamlValueChecked value.
                 * @member {baml.cffi.v1.IBamlOutboundValue|null|undefined} value
                 * @memberof baml.cffi.v1.BamlValueChecked
                 * @instance
                 */
                BamlValueChecked.prototype.value = null;

                /**
                 * BamlValueChecked checks.
                 * @member {Array.<baml.cffi.v1.IBamlCheckValue>} checks
                 * @memberof baml.cffi.v1.BamlValueChecked
                 * @instance
                 */
                BamlValueChecked.prototype.checks = $util.emptyArray;

                /**
                 * Creates a new BamlValueChecked instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlValueChecked
                 * @static
                 * @param {baml.cffi.v1.IBamlValueChecked=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValueChecked} BamlValueChecked instance
                 */
                BamlValueChecked.create = function create(properties) {
                    return new BamlValueChecked(properties);
                };

                /**
                 * Encodes the specified BamlValueChecked message. Does not implicitly {@link baml.cffi.v1.BamlValueChecked.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValueChecked
                 * @static
                 * @param {baml.cffi.v1.IBamlValueChecked} message BamlValueChecked message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueChecked.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml.cffi.v1.BamlTypeName.encode(message.name, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        $root.baml.cffi.v1.BamlOutboundValue.encode(message.value, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.checks != null && message.checks.length)
                        for (var i = 0; i < message.checks.length; ++i)
                            $root.baml.cffi.v1.BamlCheckValue.encode(message.checks[i], writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueChecked message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueChecked.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValueChecked
                 * @static
                 * @param {baml.cffi.v1.IBamlValueChecked} message BamlValueChecked message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueChecked.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueChecked message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValueChecked
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValueChecked} BamlValueChecked
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueChecked.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValueChecked();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = $root.baml.cffi.v1.BamlTypeName.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.value = $root.baml.cffi.v1.BamlOutboundValue.decode(reader, reader.uint32());
                                break;
                            }
                        case 3: {
                                if (!(message.checks && message.checks.length))
                                    message.checks = [];
                                message.checks.push($root.baml.cffi.v1.BamlCheckValue.decode(reader, reader.uint32()));
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
                 * Decodes a BamlValueChecked message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlValueChecked
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValueChecked} BamlValueChecked
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueChecked.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValueChecked message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlValueChecked
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueChecked.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml.cffi.v1.BamlTypeName.verify(message.name);
                        if (error)
                            return "name." + error;
                    }
                    if (message.value != null && message.hasOwnProperty("value")) {
                        var error = $root.baml.cffi.v1.BamlOutboundValue.verify(message.value);
                        if (error)
                            return "value." + error;
                    }
                    if (message.checks != null && message.hasOwnProperty("checks")) {
                        if (!Array.isArray(message.checks))
                            return "checks: array expected";
                        for (var i = 0; i < message.checks.length; ++i) {
                            var error = $root.baml.cffi.v1.BamlCheckValue.verify(message.checks[i]);
                            if (error)
                                return "checks." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValueChecked message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlValueChecked
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValueChecked} BamlValueChecked
                 */
                BamlValueChecked.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValueChecked)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlValueChecked();
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValueChecked.name: object expected");
                        message.name = $root.baml.cffi.v1.BamlTypeName.fromObject(object.name);
                    }
                    if (object.value != null) {
                        if (typeof object.value !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValueChecked.value: object expected");
                        message.value = $root.baml.cffi.v1.BamlOutboundValue.fromObject(object.value);
                    }
                    if (object.checks) {
                        if (!Array.isArray(object.checks))
                            throw TypeError(".baml.cffi.v1.BamlValueChecked.checks: array expected");
                        message.checks = [];
                        for (var i = 0; i < object.checks.length; ++i) {
                            if (typeof object.checks[i] !== "object")
                                throw TypeError(".baml.cffi.v1.BamlValueChecked.checks: object expected");
                            message.checks[i] = $root.baml.cffi.v1.BamlCheckValue.fromObject(object.checks[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValueChecked message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlValueChecked
                 * @static
                 * @param {baml.cffi.v1.BamlValueChecked} message BamlValueChecked
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValueChecked.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.arrays || options.defaults)
                        object.checks = [];
                    if (options.defaults) {
                        object.name = null;
                        object.value = null;
                    }
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = $root.baml.cffi.v1.BamlTypeName.toObject(message.name, options);
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = $root.baml.cffi.v1.BamlOutboundValue.toObject(message.value, options);
                    if (message.checks && message.checks.length) {
                        object.checks = [];
                        for (var j = 0; j < message.checks.length; ++j)
                            object.checks[j] = $root.baml.cffi.v1.BamlCheckValue.toObject(message.checks[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this BamlValueChecked to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlValueChecked
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueChecked.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueChecked
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValueChecked
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueChecked.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValueChecked";
                };

                return BamlValueChecked;
            })();

            /**
             * MediaTypeEnum enum.
             * @name baml.cffi.v1.MediaTypeEnum
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
                 * @memberof baml.cffi.v1
                 * @interface IBamlValueMedia
                 * @property {baml.cffi.v1.MediaTypeEnum|null} [media] BamlValueMedia media
                 * @property {string|null} [mimeType] BamlValueMedia mimeType
                 * @property {string|null} [url] BamlValueMedia url
                 * @property {string|null} [base64] BamlValueMedia base64
                 * @property {string|null} [file] BamlValueMedia file
                 */

                /**
                 * Constructs a new BamlValueMedia.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValueMedia.
                 * @implements IBamlValueMedia
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValueMedia=} [properties] Properties to set
                 */
                function BamlValueMedia(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValueMedia media.
                 * @member {baml.cffi.v1.MediaTypeEnum} media
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @instance
                 */
                BamlValueMedia.prototype.media = 0;

                /**
                 * BamlValueMedia mimeType.
                 * @member {string|null|undefined} mimeType
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @instance
                 */
                BamlValueMedia.prototype.mimeType = null;

                /**
                 * BamlValueMedia url.
                 * @member {string|null|undefined} url
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @instance
                 */
                BamlValueMedia.prototype.url = null;

                /**
                 * BamlValueMedia base64.
                 * @member {string|null|undefined} base64
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @instance
                 */
                BamlValueMedia.prototype.base64 = null;

                /**
                 * BamlValueMedia file.
                 * @member {string|null|undefined} file
                 * @memberof baml.cffi.v1.BamlValueMedia
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
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @instance
                 */
                Object.defineProperty(BamlValueMedia.prototype, "value", {
                    get: $util.oneOfGetter($oneOfFields = ["url", "base64", "file"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new BamlValueMedia instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @static
                 * @param {baml.cffi.v1.IBamlValueMedia=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValueMedia} BamlValueMedia instance
                 */
                BamlValueMedia.create = function create(properties) {
                    return new BamlValueMedia(properties);
                };

                /**
                 * Encodes the specified BamlValueMedia message. Does not implicitly {@link baml.cffi.v1.BamlValueMedia.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @static
                 * @param {baml.cffi.v1.IBamlValueMedia} message BamlValueMedia message or plain object to encode
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
                 * Encodes the specified BamlValueMedia message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueMedia.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @static
                 * @param {baml.cffi.v1.IBamlValueMedia} message BamlValueMedia message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueMedia.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueMedia message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValueMedia} BamlValueMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueMedia.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValueMedia();
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
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValueMedia} BamlValueMedia
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
                 * @memberof baml.cffi.v1.BamlValueMedia
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
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValueMedia} BamlValueMedia
                 */
                BamlValueMedia.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValueMedia)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlValueMedia();
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
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @static
                 * @param {baml.cffi.v1.BamlValueMedia} message BamlValueMedia
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
                        object.media = options.enums === String ? $root.baml.cffi.v1.MediaTypeEnum[message.media] === undefined ? message.media : $root.baml.cffi.v1.MediaTypeEnum[message.media] : message.media;
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
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueMedia.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueMedia
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValueMedia
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueMedia.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValueMedia";
                };

                return BamlValueMedia;
            })();

            v1.BamlValuePromptAst = (function() {

                /**
                 * Properties of a BamlValuePromptAst.
                 * @memberof baml.cffi.v1
                 * @interface IBamlValuePromptAst
                 * @property {baml.cffi.v1.IBamlValuePromptAstSimple|null} [simple] BamlValuePromptAst simple
                 * @property {baml.cffi.v1.IBamlValuePromptAstMessage|null} [message] BamlValuePromptAst message
                 * @property {baml.cffi.v1.IBamlValuePromptAstMultiple|null} [multiple] BamlValuePromptAst multiple
                 */

                /**
                 * Constructs a new BamlValuePromptAst.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValuePromptAst.
                 * @implements IBamlValuePromptAst
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValuePromptAst=} [properties] Properties to set
                 */
                function BamlValuePromptAst(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValuePromptAst simple.
                 * @member {baml.cffi.v1.IBamlValuePromptAstSimple|null|undefined} simple
                 * @memberof baml.cffi.v1.BamlValuePromptAst
                 * @instance
                 */
                BamlValuePromptAst.prototype.simple = null;

                /**
                 * BamlValuePromptAst message.
                 * @member {baml.cffi.v1.IBamlValuePromptAstMessage|null|undefined} message
                 * @memberof baml.cffi.v1.BamlValuePromptAst
                 * @instance
                 */
                BamlValuePromptAst.prototype.message = null;

                /**
                 * BamlValuePromptAst multiple.
                 * @member {baml.cffi.v1.IBamlValuePromptAstMultiple|null|undefined} multiple
                 * @memberof baml.cffi.v1.BamlValuePromptAst
                 * @instance
                 */
                BamlValuePromptAst.prototype.multiple = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * BamlValuePromptAst value.
                 * @member {"simple"|"message"|"multiple"|undefined} value
                 * @memberof baml.cffi.v1.BamlValuePromptAst
                 * @instance
                 */
                Object.defineProperty(BamlValuePromptAst.prototype, "value", {
                    get: $util.oneOfGetter($oneOfFields = ["simple", "message", "multiple"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new BamlValuePromptAst instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAst=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValuePromptAst} BamlValuePromptAst instance
                 */
                BamlValuePromptAst.create = function create(properties) {
                    return new BamlValuePromptAst(properties);
                };

                /**
                 * Encodes the specified BamlValuePromptAst message. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAst.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAst} message BamlValuePromptAst message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAst.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.simple != null && Object.hasOwnProperty.call(message, "simple"))
                        $root.baml.cffi.v1.BamlValuePromptAstSimple.encode(message.simple, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.message != null && Object.hasOwnProperty.call(message, "message"))
                        $root.baml.cffi.v1.BamlValuePromptAstMessage.encode(message.message, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.multiple != null && Object.hasOwnProperty.call(message, "multiple"))
                        $root.baml.cffi.v1.BamlValuePromptAstMultiple.encode(message.multiple, writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValuePromptAst message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAst.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAst} message BamlValuePromptAst message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAst.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValuePromptAst message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValuePromptAst} BamlValuePromptAst
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAst.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValuePromptAst();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.simple = $root.baml.cffi.v1.BamlValuePromptAstSimple.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.message = $root.baml.cffi.v1.BamlValuePromptAstMessage.decode(reader, reader.uint32());
                                break;
                            }
                        case 3: {
                                message.multiple = $root.baml.cffi.v1.BamlValuePromptAstMultiple.decode(reader, reader.uint32());
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
                 * @memberof baml.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValuePromptAst} BamlValuePromptAst
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
                 * @memberof baml.cffi.v1.BamlValuePromptAst
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
                            var error = $root.baml.cffi.v1.BamlValuePromptAstSimple.verify(message.simple);
                            if (error)
                                return "simple." + error;
                        }
                    }
                    if (message.message != null && message.hasOwnProperty("message")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlValuePromptAstMessage.verify(message.message);
                            if (error)
                                return "message." + error;
                        }
                    }
                    if (message.multiple != null && message.hasOwnProperty("multiple")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlValuePromptAstMultiple.verify(message.multiple);
                            if (error)
                                return "multiple." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValuePromptAst message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValuePromptAst} BamlValuePromptAst
                 */
                BamlValuePromptAst.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValuePromptAst)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlValuePromptAst();
                    if (object.simple != null) {
                        if (typeof object.simple !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValuePromptAst.simple: object expected");
                        message.simple = $root.baml.cffi.v1.BamlValuePromptAstSimple.fromObject(object.simple);
                    }
                    if (object.message != null) {
                        if (typeof object.message !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValuePromptAst.message: object expected");
                        message.message = $root.baml.cffi.v1.BamlValuePromptAstMessage.fromObject(object.message);
                    }
                    if (object.multiple != null) {
                        if (typeof object.multiple !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValuePromptAst.multiple: object expected");
                        message.multiple = $root.baml.cffi.v1.BamlValuePromptAstMultiple.fromObject(object.multiple);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValuePromptAst message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {baml.cffi.v1.BamlValuePromptAst} message BamlValuePromptAst
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValuePromptAst.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (message.simple != null && message.hasOwnProperty("simple")) {
                        object.simple = $root.baml.cffi.v1.BamlValuePromptAstSimple.toObject(message.simple, options);
                        if (options.oneofs)
                            object.value = "simple";
                    }
                    if (message.message != null && message.hasOwnProperty("message")) {
                        object.message = $root.baml.cffi.v1.BamlValuePromptAstMessage.toObject(message.message, options);
                        if (options.oneofs)
                            object.value = "message";
                    }
                    if (message.multiple != null && message.hasOwnProperty("multiple")) {
                        object.multiple = $root.baml.cffi.v1.BamlValuePromptAstMultiple.toObject(message.multiple, options);
                        if (options.oneofs)
                            object.value = "multiple";
                    }
                    return object;
                };

                /**
                 * Converts this BamlValuePromptAst to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlValuePromptAst
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValuePromptAst.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValuePromptAst
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValuePromptAst
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValuePromptAst.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValuePromptAst";
                };

                return BamlValuePromptAst;
            })();

            v1.BamlValuePromptAstMessage = (function() {

                /**
                 * Properties of a BamlValuePromptAstMessage.
                 * @memberof baml.cffi.v1
                 * @interface IBamlValuePromptAstMessage
                 * @property {string|null} [role] BamlValuePromptAstMessage role
                 * @property {baml.cffi.v1.IBamlValuePromptAstSimple|null} [content] BamlValuePromptAstMessage content
                 * @property {string|null} [metadataAsJson] BamlValuePromptAstMessage metadataAsJson
                 */

                /**
                 * Constructs a new BamlValuePromptAstMessage.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValuePromptAstMessage.
                 * @implements IBamlValuePromptAstMessage
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValuePromptAstMessage=} [properties] Properties to set
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
                 * @memberof baml.cffi.v1.BamlValuePromptAstMessage
                 * @instance
                 */
                BamlValuePromptAstMessage.prototype.role = "";

                /**
                 * BamlValuePromptAstMessage content.
                 * @member {baml.cffi.v1.IBamlValuePromptAstSimple|null|undefined} content
                 * @memberof baml.cffi.v1.BamlValuePromptAstMessage
                 * @instance
                 */
                BamlValuePromptAstMessage.prototype.content = null;

                /**
                 * BamlValuePromptAstMessage metadataAsJson.
                 * @member {string} metadataAsJson
                 * @memberof baml.cffi.v1.BamlValuePromptAstMessage
                 * @instance
                 */
                BamlValuePromptAstMessage.prototype.metadataAsJson = "";

                /**
                 * Creates a new BamlValuePromptAstMessage instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAstMessage=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValuePromptAstMessage} BamlValuePromptAstMessage instance
                 */
                BamlValuePromptAstMessage.create = function create(properties) {
                    return new BamlValuePromptAstMessage(properties);
                };

                /**
                 * Encodes the specified BamlValuePromptAstMessage message. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstMessage.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAstMessage} message BamlValuePromptAstMessage message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstMessage.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.role != null && Object.hasOwnProperty.call(message, "role"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.role);
                    if (message.content != null && Object.hasOwnProperty.call(message, "content"))
                        $root.baml.cffi.v1.BamlValuePromptAstSimple.encode(message.content, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.metadataAsJson != null && Object.hasOwnProperty.call(message, "metadataAsJson"))
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.metadataAsJson);
                    return writer;
                };

                /**
                 * Encodes the specified BamlValuePromptAstMessage message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstMessage.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAstMessage} message BamlValuePromptAstMessage message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstMessage.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValuePromptAstMessage message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValuePromptAstMessage} BamlValuePromptAstMessage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAstMessage.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValuePromptAstMessage();
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
                                message.content = $root.baml.cffi.v1.BamlValuePromptAstSimple.decode(reader, reader.uint32());
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
                 * @memberof baml.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValuePromptAstMessage} BamlValuePromptAstMessage
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
                 * @memberof baml.cffi.v1.BamlValuePromptAstMessage
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
                        var error = $root.baml.cffi.v1.BamlValuePromptAstSimple.verify(message.content);
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
                 * @memberof baml.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValuePromptAstMessage} BamlValuePromptAstMessage
                 */
                BamlValuePromptAstMessage.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValuePromptAstMessage)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlValuePromptAstMessage();
                    if (object.role != null)
                        message.role = String(object.role);
                    if (object.content != null) {
                        if (typeof object.content !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValuePromptAstMessage.content: object expected");
                        message.content = $root.baml.cffi.v1.BamlValuePromptAstSimple.fromObject(object.content);
                    }
                    if (object.metadataAsJson != null)
                        message.metadataAsJson = String(object.metadataAsJson);
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValuePromptAstMessage message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {baml.cffi.v1.BamlValuePromptAstMessage} message BamlValuePromptAstMessage
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
                        object.content = $root.baml.cffi.v1.BamlValuePromptAstSimple.toObject(message.content, options);
                    if (message.metadataAsJson != null && message.hasOwnProperty("metadataAsJson"))
                        object.metadataAsJson = message.metadataAsJson;
                    return object;
                };

                /**
                 * Converts this BamlValuePromptAstMessage to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlValuePromptAstMessage
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValuePromptAstMessage.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValuePromptAstMessage
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValuePromptAstMessage
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValuePromptAstMessage.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValuePromptAstMessage";
                };

                return BamlValuePromptAstMessage;
            })();

            v1.BamlValuePromptAstMultiple = (function() {

                /**
                 * Properties of a BamlValuePromptAstMultiple.
                 * @memberof baml.cffi.v1
                 * @interface IBamlValuePromptAstMultiple
                 * @property {Array.<baml.cffi.v1.IBamlValuePromptAst>|null} [items] BamlValuePromptAstMultiple items
                 */

                /**
                 * Constructs a new BamlValuePromptAstMultiple.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValuePromptAstMultiple.
                 * @implements IBamlValuePromptAstMultiple
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValuePromptAstMultiple=} [properties] Properties to set
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
                 * @member {Array.<baml.cffi.v1.IBamlValuePromptAst>} items
                 * @memberof baml.cffi.v1.BamlValuePromptAstMultiple
                 * @instance
                 */
                BamlValuePromptAstMultiple.prototype.items = $util.emptyArray;

                /**
                 * Creates a new BamlValuePromptAstMultiple instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAstMultiple=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValuePromptAstMultiple} BamlValuePromptAstMultiple instance
                 */
                BamlValuePromptAstMultiple.create = function create(properties) {
                    return new BamlValuePromptAstMultiple(properties);
                };

                /**
                 * Encodes the specified BamlValuePromptAstMultiple message. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstMultiple.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAstMultiple} message BamlValuePromptAstMultiple message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstMultiple.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.items != null && message.items.length)
                        for (var i = 0; i < message.items.length; ++i)
                            $root.baml.cffi.v1.BamlValuePromptAst.encode(message.items[i], writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValuePromptAstMultiple message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstMultiple.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAstMultiple} message BamlValuePromptAstMultiple message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstMultiple.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValuePromptAstMultiple message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValuePromptAstMultiple} BamlValuePromptAstMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAstMultiple.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValuePromptAstMultiple();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                if (!(message.items && message.items.length))
                                    message.items = [];
                                message.items.push($root.baml.cffi.v1.BamlValuePromptAst.decode(reader, reader.uint32()));
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
                 * @memberof baml.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValuePromptAstMultiple} BamlValuePromptAstMultiple
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
                 * @memberof baml.cffi.v1.BamlValuePromptAstMultiple
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
                            var error = $root.baml.cffi.v1.BamlValuePromptAst.verify(message.items[i]);
                            if (error)
                                return "items." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValuePromptAstMultiple message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValuePromptAstMultiple} BamlValuePromptAstMultiple
                 */
                BamlValuePromptAstMultiple.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValuePromptAstMultiple)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlValuePromptAstMultiple();
                    if (object.items) {
                        if (!Array.isArray(object.items))
                            throw TypeError(".baml.cffi.v1.BamlValuePromptAstMultiple.items: array expected");
                        message.items = [];
                        for (var i = 0; i < object.items.length; ++i) {
                            if (typeof object.items[i] !== "object")
                                throw TypeError(".baml.cffi.v1.BamlValuePromptAstMultiple.items: object expected");
                            message.items[i] = $root.baml.cffi.v1.BamlValuePromptAst.fromObject(object.items[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValuePromptAstMultiple message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {baml.cffi.v1.BamlValuePromptAstMultiple} message BamlValuePromptAstMultiple
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
                            object.items[j] = $root.baml.cffi.v1.BamlValuePromptAst.toObject(message.items[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this BamlValuePromptAstMultiple to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlValuePromptAstMultiple
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValuePromptAstMultiple.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValuePromptAstMultiple
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValuePromptAstMultiple
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValuePromptAstMultiple.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValuePromptAstMultiple";
                };

                return BamlValuePromptAstMultiple;
            })();

            v1.BamlValuePromptAstSimple = (function() {

                /**
                 * Properties of a BamlValuePromptAstSimple.
                 * @memberof baml.cffi.v1
                 * @interface IBamlValuePromptAstSimple
                 * @property {string|null} [string] BamlValuePromptAstSimple string
                 * @property {baml.cffi.v1.IBamlValueMedia|null} [media] BamlValuePromptAstSimple media
                 * @property {baml.cffi.v1.IBamlValuePromptAstSimpleMultiple|null} [multiple] BamlValuePromptAstSimple multiple
                 */

                /**
                 * Constructs a new BamlValuePromptAstSimple.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValuePromptAstSimple.
                 * @implements IBamlValuePromptAstSimple
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValuePromptAstSimple=} [properties] Properties to set
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
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
                 * @instance
                 */
                BamlValuePromptAstSimple.prototype.string = null;

                /**
                 * BamlValuePromptAstSimple media.
                 * @member {baml.cffi.v1.IBamlValueMedia|null|undefined} media
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
                 * @instance
                 */
                BamlValuePromptAstSimple.prototype.media = null;

                /**
                 * BamlValuePromptAstSimple multiple.
                 * @member {baml.cffi.v1.IBamlValuePromptAstSimpleMultiple|null|undefined} multiple
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
                 * @instance
                 */
                BamlValuePromptAstSimple.prototype.multiple = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * BamlValuePromptAstSimple value.
                 * @member {"string"|"media"|"multiple"|undefined} value
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
                 * @instance
                 */
                Object.defineProperty(BamlValuePromptAstSimple.prototype, "value", {
                    get: $util.oneOfGetter($oneOfFields = ["string", "media", "multiple"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new BamlValuePromptAstSimple instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAstSimple=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValuePromptAstSimple} BamlValuePromptAstSimple instance
                 */
                BamlValuePromptAstSimple.create = function create(properties) {
                    return new BamlValuePromptAstSimple(properties);
                };

                /**
                 * Encodes the specified BamlValuePromptAstSimple message. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstSimple.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAstSimple} message BamlValuePromptAstSimple message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstSimple.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.string != null && Object.hasOwnProperty.call(message, "string"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.string);
                    if (message.media != null && Object.hasOwnProperty.call(message, "media"))
                        $root.baml.cffi.v1.BamlValueMedia.encode(message.media, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.multiple != null && Object.hasOwnProperty.call(message, "multiple"))
                        $root.baml.cffi.v1.BamlValuePromptAstSimpleMultiple.encode(message.multiple, writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValuePromptAstSimple message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstSimple.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAstSimple} message BamlValuePromptAstSimple message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstSimple.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValuePromptAstSimple message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValuePromptAstSimple} BamlValuePromptAstSimple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAstSimple.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValuePromptAstSimple();
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
                                message.media = $root.baml.cffi.v1.BamlValueMedia.decode(reader, reader.uint32());
                                break;
                            }
                        case 3: {
                                message.multiple = $root.baml.cffi.v1.BamlValuePromptAstSimpleMultiple.decode(reader, reader.uint32());
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
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValuePromptAstSimple} BamlValuePromptAstSimple
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
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
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
                            var error = $root.baml.cffi.v1.BamlValueMedia.verify(message.media);
                            if (error)
                                return "media." + error;
                        }
                    }
                    if (message.multiple != null && message.hasOwnProperty("multiple")) {
                        if (properties.value === 1)
                            return "value: multiple values";
                        properties.value = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlValuePromptAstSimpleMultiple.verify(message.multiple);
                            if (error)
                                return "multiple." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValuePromptAstSimple message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValuePromptAstSimple} BamlValuePromptAstSimple
                 */
                BamlValuePromptAstSimple.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValuePromptAstSimple)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlValuePromptAstSimple();
                    if (object.string != null)
                        message.string = String(object.string);
                    if (object.media != null) {
                        if (typeof object.media !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValuePromptAstSimple.media: object expected");
                        message.media = $root.baml.cffi.v1.BamlValueMedia.fromObject(object.media);
                    }
                    if (object.multiple != null) {
                        if (typeof object.multiple !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValuePromptAstSimple.multiple: object expected");
                        message.multiple = $root.baml.cffi.v1.BamlValuePromptAstSimpleMultiple.fromObject(object.multiple);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValuePromptAstSimple message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {baml.cffi.v1.BamlValuePromptAstSimple} message BamlValuePromptAstSimple
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
                        object.media = $root.baml.cffi.v1.BamlValueMedia.toObject(message.media, options);
                        if (options.oneofs)
                            object.value = "media";
                    }
                    if (message.multiple != null && message.hasOwnProperty("multiple")) {
                        object.multiple = $root.baml.cffi.v1.BamlValuePromptAstSimpleMultiple.toObject(message.multiple, options);
                        if (options.oneofs)
                            object.value = "multiple";
                    }
                    return object;
                };

                /**
                 * Converts this BamlValuePromptAstSimple to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValuePromptAstSimple.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValuePromptAstSimple
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimple
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValuePromptAstSimple.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValuePromptAstSimple";
                };

                return BamlValuePromptAstSimple;
            })();

            v1.BamlValuePromptAstSimpleMultiple = (function() {

                /**
                 * Properties of a BamlValuePromptAstSimpleMultiple.
                 * @memberof baml.cffi.v1
                 * @interface IBamlValuePromptAstSimpleMultiple
                 * @property {Array.<baml.cffi.v1.IBamlValuePromptAstSimple>|null} [items] BamlValuePromptAstSimpleMultiple items
                 */

                /**
                 * Constructs a new BamlValuePromptAstSimpleMultiple.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValuePromptAstSimpleMultiple.
                 * @implements IBamlValuePromptAstSimpleMultiple
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValuePromptAstSimpleMultiple=} [properties] Properties to set
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
                 * @member {Array.<baml.cffi.v1.IBamlValuePromptAstSimple>} items
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @instance
                 */
                BamlValuePromptAstSimpleMultiple.prototype.items = $util.emptyArray;

                /**
                 * Creates a new BamlValuePromptAstSimpleMultiple instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAstSimpleMultiple=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValuePromptAstSimpleMultiple} BamlValuePromptAstSimpleMultiple instance
                 */
                BamlValuePromptAstSimpleMultiple.create = function create(properties) {
                    return new BamlValuePromptAstSimpleMultiple(properties);
                };

                /**
                 * Encodes the specified BamlValuePromptAstSimpleMultiple message. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstSimpleMultiple.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAstSimpleMultiple} message BamlValuePromptAstSimpleMultiple message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstSimpleMultiple.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.items != null && message.items.length)
                        for (var i = 0; i < message.items.length; ++i)
                            $root.baml.cffi.v1.BamlValuePromptAstSimple.encode(message.items[i], writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValuePromptAstSimpleMultiple message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstSimpleMultiple.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {baml.cffi.v1.IBamlValuePromptAstSimpleMultiple} message BamlValuePromptAstSimpleMultiple message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValuePromptAstSimpleMultiple.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValuePromptAstSimpleMultiple message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValuePromptAstSimpleMultiple} BamlValuePromptAstSimpleMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValuePromptAstSimpleMultiple.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValuePromptAstSimpleMultiple();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                if (!(message.items && message.items.length))
                                    message.items = [];
                                message.items.push($root.baml.cffi.v1.BamlValuePromptAstSimple.decode(reader, reader.uint32()));
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
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValuePromptAstSimpleMultiple} BamlValuePromptAstSimpleMultiple
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
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimpleMultiple
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
                            var error = $root.baml.cffi.v1.BamlValuePromptAstSimple.verify(message.items[i]);
                            if (error)
                                return "items." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlValuePromptAstSimpleMultiple message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValuePromptAstSimpleMultiple} BamlValuePromptAstSimpleMultiple
                 */
                BamlValuePromptAstSimpleMultiple.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValuePromptAstSimpleMultiple)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlValuePromptAstSimpleMultiple();
                    if (object.items) {
                        if (!Array.isArray(object.items))
                            throw TypeError(".baml.cffi.v1.BamlValuePromptAstSimpleMultiple.items: array expected");
                        message.items = [];
                        for (var i = 0; i < object.items.length; ++i) {
                            if (typeof object.items[i] !== "object")
                                throw TypeError(".baml.cffi.v1.BamlValuePromptAstSimpleMultiple.items: object expected");
                            message.items[i] = $root.baml.cffi.v1.BamlValuePromptAstSimple.fromObject(object.items[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValuePromptAstSimpleMultiple message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {baml.cffi.v1.BamlValuePromptAstSimpleMultiple} message BamlValuePromptAstSimpleMultiple
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
                            object.items[j] = $root.baml.cffi.v1.BamlValuePromptAstSimple.toObject(message.items[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this BamlValuePromptAstSimpleMultiple to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValuePromptAstSimpleMultiple.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValuePromptAstSimpleMultiple
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValuePromptAstSimpleMultiple
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValuePromptAstSimpleMultiple.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValuePromptAstSimpleMultiple";
                };

                return BamlValuePromptAstSimpleMultiple;
            })();

            v1.BamlFieldType = (function() {

                /**
                 * Properties of a BamlFieldType.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldType
                 * @property {baml.cffi.v1.IBamlFieldTypeString|null} [stringType] BamlFieldType stringType
                 * @property {baml.cffi.v1.IBamlFieldTypeInt|null} [intType] BamlFieldType intType
                 * @property {baml.cffi.v1.IBamlFieldTypeFloat|null} [floatType] BamlFieldType floatType
                 * @property {baml.cffi.v1.IBamlFieldTypeBool|null} [boolType] BamlFieldType boolType
                 * @property {baml.cffi.v1.IBamlFieldTypeNull|null} [nullType] BamlFieldType nullType
                 * @property {baml.cffi.v1.IBamlFieldTypeLiteral|null} [literalType] BamlFieldType literalType
                 * @property {baml.cffi.v1.IBamlFieldTypeMedia|null} [mediaType] BamlFieldType mediaType
                 * @property {baml.cffi.v1.IBamlFieldTypeEnum|null} [enumType] BamlFieldType enumType
                 * @property {baml.cffi.v1.IBamlFieldTypeClass|null} [classType] BamlFieldType classType
                 * @property {baml.cffi.v1.IBamlFieldTypeTypeAlias|null} [typeAliasType] BamlFieldType typeAliasType
                 * @property {baml.cffi.v1.IBamlFieldTypeList|null} [listType] BamlFieldType listType
                 * @property {baml.cffi.v1.IBamlFieldTypeMap|null} [mapType] BamlFieldType mapType
                 * @property {baml.cffi.v1.IBamlFieldTypeUnionVariant|null} [unionVariantType] BamlFieldType unionVariantType
                 * @property {baml.cffi.v1.IBamlFieldTypeOptional|null} [optionalType] BamlFieldType optionalType
                 * @property {baml.cffi.v1.IBamlFieldTypeChecked|null} [checkedType] BamlFieldType checkedType
                 * @property {baml.cffi.v1.IBamlFieldTypeStreamState|null} [streamStateType] BamlFieldType streamStateType
                 * @property {baml.cffi.v1.IBamlFieldTypeAny|null} [anyType] BamlFieldType anyType
                 * @property {baml.cffi.v1.IBamlFieldTypeUint8Array|null} [uint8arrayType] BamlFieldType uint8arrayType
                 * @property {baml.cffi.v1.IBamlFieldTypeUnknown|null} [unknownType] BamlFieldType unknownType
                 */

                /**
                 * Constructs a new BamlFieldType.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldType.
                 * @implements IBamlFieldType
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldType=} [properties] Properties to set
                 */
                function BamlFieldType(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlFieldType stringType.
                 * @member {baml.cffi.v1.IBamlFieldTypeString|null|undefined} stringType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.stringType = null;

                /**
                 * BamlFieldType intType.
                 * @member {baml.cffi.v1.IBamlFieldTypeInt|null|undefined} intType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.intType = null;

                /**
                 * BamlFieldType floatType.
                 * @member {baml.cffi.v1.IBamlFieldTypeFloat|null|undefined} floatType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.floatType = null;

                /**
                 * BamlFieldType boolType.
                 * @member {baml.cffi.v1.IBamlFieldTypeBool|null|undefined} boolType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.boolType = null;

                /**
                 * BamlFieldType nullType.
                 * @member {baml.cffi.v1.IBamlFieldTypeNull|null|undefined} nullType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.nullType = null;

                /**
                 * BamlFieldType literalType.
                 * @member {baml.cffi.v1.IBamlFieldTypeLiteral|null|undefined} literalType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.literalType = null;

                /**
                 * BamlFieldType mediaType.
                 * @member {baml.cffi.v1.IBamlFieldTypeMedia|null|undefined} mediaType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.mediaType = null;

                /**
                 * BamlFieldType enumType.
                 * @member {baml.cffi.v1.IBamlFieldTypeEnum|null|undefined} enumType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.enumType = null;

                /**
                 * BamlFieldType classType.
                 * @member {baml.cffi.v1.IBamlFieldTypeClass|null|undefined} classType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.classType = null;

                /**
                 * BamlFieldType typeAliasType.
                 * @member {baml.cffi.v1.IBamlFieldTypeTypeAlias|null|undefined} typeAliasType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.typeAliasType = null;

                /**
                 * BamlFieldType listType.
                 * @member {baml.cffi.v1.IBamlFieldTypeList|null|undefined} listType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.listType = null;

                /**
                 * BamlFieldType mapType.
                 * @member {baml.cffi.v1.IBamlFieldTypeMap|null|undefined} mapType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.mapType = null;

                /**
                 * BamlFieldType unionVariantType.
                 * @member {baml.cffi.v1.IBamlFieldTypeUnionVariant|null|undefined} unionVariantType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.unionVariantType = null;

                /**
                 * BamlFieldType optionalType.
                 * @member {baml.cffi.v1.IBamlFieldTypeOptional|null|undefined} optionalType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.optionalType = null;

                /**
                 * BamlFieldType checkedType.
                 * @member {baml.cffi.v1.IBamlFieldTypeChecked|null|undefined} checkedType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.checkedType = null;

                /**
                 * BamlFieldType streamStateType.
                 * @member {baml.cffi.v1.IBamlFieldTypeStreamState|null|undefined} streamStateType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.streamStateType = null;

                /**
                 * BamlFieldType anyType.
                 * @member {baml.cffi.v1.IBamlFieldTypeAny|null|undefined} anyType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.anyType = null;

                /**
                 * BamlFieldType uint8arrayType.
                 * @member {baml.cffi.v1.IBamlFieldTypeUint8Array|null|undefined} uint8arrayType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.uint8arrayType = null;

                /**
                 * BamlFieldType unknownType.
                 * @member {baml.cffi.v1.IBamlFieldTypeUnknown|null|undefined} unknownType
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                BamlFieldType.prototype.unknownType = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * BamlFieldType type.
                 * @member {"stringType"|"intType"|"floatType"|"boolType"|"nullType"|"literalType"|"mediaType"|"enumType"|"classType"|"typeAliasType"|"listType"|"mapType"|"unionVariantType"|"optionalType"|"checkedType"|"streamStateType"|"anyType"|"uint8arrayType"|"unknownType"|undefined} type
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 */
                Object.defineProperty(BamlFieldType.prototype, "type", {
                    get: $util.oneOfGetter($oneOfFields = ["stringType", "intType", "floatType", "boolType", "nullType", "literalType", "mediaType", "enumType", "classType", "typeAliasType", "listType", "mapType", "unionVariantType", "optionalType", "checkedType", "streamStateType", "anyType", "uint8arrayType", "unknownType"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new BamlFieldType instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldType=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldType} BamlFieldType instance
                 */
                BamlFieldType.create = function create(properties) {
                    return new BamlFieldType(properties);
                };

                /**
                 * Encodes the specified BamlFieldType message. Does not implicitly {@link baml.cffi.v1.BamlFieldType.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldType} message BamlFieldType message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldType.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.stringType != null && Object.hasOwnProperty.call(message, "stringType"))
                        $root.baml.cffi.v1.BamlFieldTypeString.encode(message.stringType, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.intType != null && Object.hasOwnProperty.call(message, "intType"))
                        $root.baml.cffi.v1.BamlFieldTypeInt.encode(message.intType, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.floatType != null && Object.hasOwnProperty.call(message, "floatType"))
                        $root.baml.cffi.v1.BamlFieldTypeFloat.encode(message.floatType, writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    if (message.boolType != null && Object.hasOwnProperty.call(message, "boolType"))
                        $root.baml.cffi.v1.BamlFieldTypeBool.encode(message.boolType, writer.uint32(/* id 4, wireType 2 =*/34).fork()).ldelim();
                    if (message.nullType != null && Object.hasOwnProperty.call(message, "nullType"))
                        $root.baml.cffi.v1.BamlFieldTypeNull.encode(message.nullType, writer.uint32(/* id 5, wireType 2 =*/42).fork()).ldelim();
                    if (message.literalType != null && Object.hasOwnProperty.call(message, "literalType"))
                        $root.baml.cffi.v1.BamlFieldTypeLiteral.encode(message.literalType, writer.uint32(/* id 6, wireType 2 =*/50).fork()).ldelim();
                    if (message.mediaType != null && Object.hasOwnProperty.call(message, "mediaType"))
                        $root.baml.cffi.v1.BamlFieldTypeMedia.encode(message.mediaType, writer.uint32(/* id 7, wireType 2 =*/58).fork()).ldelim();
                    if (message.enumType != null && Object.hasOwnProperty.call(message, "enumType"))
                        $root.baml.cffi.v1.BamlFieldTypeEnum.encode(message.enumType, writer.uint32(/* id 8, wireType 2 =*/66).fork()).ldelim();
                    if (message.classType != null && Object.hasOwnProperty.call(message, "classType"))
                        $root.baml.cffi.v1.BamlFieldTypeClass.encode(message.classType, writer.uint32(/* id 9, wireType 2 =*/74).fork()).ldelim();
                    if (message.typeAliasType != null && Object.hasOwnProperty.call(message, "typeAliasType"))
                        $root.baml.cffi.v1.BamlFieldTypeTypeAlias.encode(message.typeAliasType, writer.uint32(/* id 10, wireType 2 =*/82).fork()).ldelim();
                    if (message.listType != null && Object.hasOwnProperty.call(message, "listType"))
                        $root.baml.cffi.v1.BamlFieldTypeList.encode(message.listType, writer.uint32(/* id 11, wireType 2 =*/90).fork()).ldelim();
                    if (message.mapType != null && Object.hasOwnProperty.call(message, "mapType"))
                        $root.baml.cffi.v1.BamlFieldTypeMap.encode(message.mapType, writer.uint32(/* id 12, wireType 2 =*/98).fork()).ldelim();
                    if (message.unionVariantType != null && Object.hasOwnProperty.call(message, "unionVariantType"))
                        $root.baml.cffi.v1.BamlFieldTypeUnionVariant.encode(message.unionVariantType, writer.uint32(/* id 14, wireType 2 =*/114).fork()).ldelim();
                    if (message.optionalType != null && Object.hasOwnProperty.call(message, "optionalType"))
                        $root.baml.cffi.v1.BamlFieldTypeOptional.encode(message.optionalType, writer.uint32(/* id 15, wireType 2 =*/122).fork()).ldelim();
                    if (message.checkedType != null && Object.hasOwnProperty.call(message, "checkedType"))
                        $root.baml.cffi.v1.BamlFieldTypeChecked.encode(message.checkedType, writer.uint32(/* id 16, wireType 2 =*/130).fork()).ldelim();
                    if (message.streamStateType != null && Object.hasOwnProperty.call(message, "streamStateType"))
                        $root.baml.cffi.v1.BamlFieldTypeStreamState.encode(message.streamStateType, writer.uint32(/* id 17, wireType 2 =*/138).fork()).ldelim();
                    if (message.anyType != null && Object.hasOwnProperty.call(message, "anyType"))
                        $root.baml.cffi.v1.BamlFieldTypeAny.encode(message.anyType, writer.uint32(/* id 18, wireType 2 =*/146).fork()).ldelim();
                    if (message.uint8arrayType != null && Object.hasOwnProperty.call(message, "uint8arrayType"))
                        $root.baml.cffi.v1.BamlFieldTypeUint8Array.encode(message.uint8arrayType, writer.uint32(/* id 19, wireType 2 =*/154).fork()).ldelim();
                    if (message.unknownType != null && Object.hasOwnProperty.call(message, "unknownType"))
                        $root.baml.cffi.v1.BamlFieldTypeUnknown.encode(message.unknownType, writer.uint32(/* id 20, wireType 2 =*/162).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldType message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldType.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldType} message BamlFieldType message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldType.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldType message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldType} BamlFieldType
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldType.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldType();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.stringType = $root.baml.cffi.v1.BamlFieldTypeString.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.intType = $root.baml.cffi.v1.BamlFieldTypeInt.decode(reader, reader.uint32());
                                break;
                            }
                        case 3: {
                                message.floatType = $root.baml.cffi.v1.BamlFieldTypeFloat.decode(reader, reader.uint32());
                                break;
                            }
                        case 4: {
                                message.boolType = $root.baml.cffi.v1.BamlFieldTypeBool.decode(reader, reader.uint32());
                                break;
                            }
                        case 5: {
                                message.nullType = $root.baml.cffi.v1.BamlFieldTypeNull.decode(reader, reader.uint32());
                                break;
                            }
                        case 6: {
                                message.literalType = $root.baml.cffi.v1.BamlFieldTypeLiteral.decode(reader, reader.uint32());
                                break;
                            }
                        case 7: {
                                message.mediaType = $root.baml.cffi.v1.BamlFieldTypeMedia.decode(reader, reader.uint32());
                                break;
                            }
                        case 8: {
                                message.enumType = $root.baml.cffi.v1.BamlFieldTypeEnum.decode(reader, reader.uint32());
                                break;
                            }
                        case 9: {
                                message.classType = $root.baml.cffi.v1.BamlFieldTypeClass.decode(reader, reader.uint32());
                                break;
                            }
                        case 10: {
                                message.typeAliasType = $root.baml.cffi.v1.BamlFieldTypeTypeAlias.decode(reader, reader.uint32());
                                break;
                            }
                        case 11: {
                                message.listType = $root.baml.cffi.v1.BamlFieldTypeList.decode(reader, reader.uint32());
                                break;
                            }
                        case 12: {
                                message.mapType = $root.baml.cffi.v1.BamlFieldTypeMap.decode(reader, reader.uint32());
                                break;
                            }
                        case 14: {
                                message.unionVariantType = $root.baml.cffi.v1.BamlFieldTypeUnionVariant.decode(reader, reader.uint32());
                                break;
                            }
                        case 15: {
                                message.optionalType = $root.baml.cffi.v1.BamlFieldTypeOptional.decode(reader, reader.uint32());
                                break;
                            }
                        case 16: {
                                message.checkedType = $root.baml.cffi.v1.BamlFieldTypeChecked.decode(reader, reader.uint32());
                                break;
                            }
                        case 17: {
                                message.streamStateType = $root.baml.cffi.v1.BamlFieldTypeStreamState.decode(reader, reader.uint32());
                                break;
                            }
                        case 18: {
                                message.anyType = $root.baml.cffi.v1.BamlFieldTypeAny.decode(reader, reader.uint32());
                                break;
                            }
                        case 19: {
                                message.uint8arrayType = $root.baml.cffi.v1.BamlFieldTypeUint8Array.decode(reader, reader.uint32());
                                break;
                            }
                        case 20: {
                                message.unknownType = $root.baml.cffi.v1.BamlFieldTypeUnknown.decode(reader, reader.uint32());
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
                 * Decodes a BamlFieldType message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldType} BamlFieldType
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldType.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldType message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldType.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    var properties = {};
                    if (message.stringType != null && message.hasOwnProperty("stringType")) {
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeString.verify(message.stringType);
                            if (error)
                                return "stringType." + error;
                        }
                    }
                    if (message.intType != null && message.hasOwnProperty("intType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeInt.verify(message.intType);
                            if (error)
                                return "intType." + error;
                        }
                    }
                    if (message.floatType != null && message.hasOwnProperty("floatType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeFloat.verify(message.floatType);
                            if (error)
                                return "floatType." + error;
                        }
                    }
                    if (message.boolType != null && message.hasOwnProperty("boolType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeBool.verify(message.boolType);
                            if (error)
                                return "boolType." + error;
                        }
                    }
                    if (message.nullType != null && message.hasOwnProperty("nullType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeNull.verify(message.nullType);
                            if (error)
                                return "nullType." + error;
                        }
                    }
                    if (message.literalType != null && message.hasOwnProperty("literalType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeLiteral.verify(message.literalType);
                            if (error)
                                return "literalType." + error;
                        }
                    }
                    if (message.mediaType != null && message.hasOwnProperty("mediaType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeMedia.verify(message.mediaType);
                            if (error)
                                return "mediaType." + error;
                        }
                    }
                    if (message.enumType != null && message.hasOwnProperty("enumType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeEnum.verify(message.enumType);
                            if (error)
                                return "enumType." + error;
                        }
                    }
                    if (message.classType != null && message.hasOwnProperty("classType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeClass.verify(message.classType);
                            if (error)
                                return "classType." + error;
                        }
                    }
                    if (message.typeAliasType != null && message.hasOwnProperty("typeAliasType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeTypeAlias.verify(message.typeAliasType);
                            if (error)
                                return "typeAliasType." + error;
                        }
                    }
                    if (message.listType != null && message.hasOwnProperty("listType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeList.verify(message.listType);
                            if (error)
                                return "listType." + error;
                        }
                    }
                    if (message.mapType != null && message.hasOwnProperty("mapType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeMap.verify(message.mapType);
                            if (error)
                                return "mapType." + error;
                        }
                    }
                    if (message.unionVariantType != null && message.hasOwnProperty("unionVariantType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeUnionVariant.verify(message.unionVariantType);
                            if (error)
                                return "unionVariantType." + error;
                        }
                    }
                    if (message.optionalType != null && message.hasOwnProperty("optionalType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeOptional.verify(message.optionalType);
                            if (error)
                                return "optionalType." + error;
                        }
                    }
                    if (message.checkedType != null && message.hasOwnProperty("checkedType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeChecked.verify(message.checkedType);
                            if (error)
                                return "checkedType." + error;
                        }
                    }
                    if (message.streamStateType != null && message.hasOwnProperty("streamStateType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeStreamState.verify(message.streamStateType);
                            if (error)
                                return "streamStateType." + error;
                        }
                    }
                    if (message.anyType != null && message.hasOwnProperty("anyType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeAny.verify(message.anyType);
                            if (error)
                                return "anyType." + error;
                        }
                    }
                    if (message.uint8arrayType != null && message.hasOwnProperty("uint8arrayType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeUint8Array.verify(message.uint8arrayType);
                            if (error)
                                return "uint8arrayType." + error;
                        }
                    }
                    if (message.unknownType != null && message.hasOwnProperty("unknownType")) {
                        if (properties.type === 1)
                            return "type: multiple values";
                        properties.type = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlFieldTypeUnknown.verify(message.unknownType);
                            if (error)
                                return "unknownType." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlFieldType message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldType} BamlFieldType
                 */
                BamlFieldType.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldType)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlFieldType();
                    if (object.stringType != null) {
                        if (typeof object.stringType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.stringType: object expected");
                        message.stringType = $root.baml.cffi.v1.BamlFieldTypeString.fromObject(object.stringType);
                    }
                    if (object.intType != null) {
                        if (typeof object.intType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.intType: object expected");
                        message.intType = $root.baml.cffi.v1.BamlFieldTypeInt.fromObject(object.intType);
                    }
                    if (object.floatType != null) {
                        if (typeof object.floatType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.floatType: object expected");
                        message.floatType = $root.baml.cffi.v1.BamlFieldTypeFloat.fromObject(object.floatType);
                    }
                    if (object.boolType != null) {
                        if (typeof object.boolType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.boolType: object expected");
                        message.boolType = $root.baml.cffi.v1.BamlFieldTypeBool.fromObject(object.boolType);
                    }
                    if (object.nullType != null) {
                        if (typeof object.nullType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.nullType: object expected");
                        message.nullType = $root.baml.cffi.v1.BamlFieldTypeNull.fromObject(object.nullType);
                    }
                    if (object.literalType != null) {
                        if (typeof object.literalType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.literalType: object expected");
                        message.literalType = $root.baml.cffi.v1.BamlFieldTypeLiteral.fromObject(object.literalType);
                    }
                    if (object.mediaType != null) {
                        if (typeof object.mediaType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.mediaType: object expected");
                        message.mediaType = $root.baml.cffi.v1.BamlFieldTypeMedia.fromObject(object.mediaType);
                    }
                    if (object.enumType != null) {
                        if (typeof object.enumType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.enumType: object expected");
                        message.enumType = $root.baml.cffi.v1.BamlFieldTypeEnum.fromObject(object.enumType);
                    }
                    if (object.classType != null) {
                        if (typeof object.classType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.classType: object expected");
                        message.classType = $root.baml.cffi.v1.BamlFieldTypeClass.fromObject(object.classType);
                    }
                    if (object.typeAliasType != null) {
                        if (typeof object.typeAliasType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.typeAliasType: object expected");
                        message.typeAliasType = $root.baml.cffi.v1.BamlFieldTypeTypeAlias.fromObject(object.typeAliasType);
                    }
                    if (object.listType != null) {
                        if (typeof object.listType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.listType: object expected");
                        message.listType = $root.baml.cffi.v1.BamlFieldTypeList.fromObject(object.listType);
                    }
                    if (object.mapType != null) {
                        if (typeof object.mapType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.mapType: object expected");
                        message.mapType = $root.baml.cffi.v1.BamlFieldTypeMap.fromObject(object.mapType);
                    }
                    if (object.unionVariantType != null) {
                        if (typeof object.unionVariantType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.unionVariantType: object expected");
                        message.unionVariantType = $root.baml.cffi.v1.BamlFieldTypeUnionVariant.fromObject(object.unionVariantType);
                    }
                    if (object.optionalType != null) {
                        if (typeof object.optionalType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.optionalType: object expected");
                        message.optionalType = $root.baml.cffi.v1.BamlFieldTypeOptional.fromObject(object.optionalType);
                    }
                    if (object.checkedType != null) {
                        if (typeof object.checkedType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.checkedType: object expected");
                        message.checkedType = $root.baml.cffi.v1.BamlFieldTypeChecked.fromObject(object.checkedType);
                    }
                    if (object.streamStateType != null) {
                        if (typeof object.streamStateType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.streamStateType: object expected");
                        message.streamStateType = $root.baml.cffi.v1.BamlFieldTypeStreamState.fromObject(object.streamStateType);
                    }
                    if (object.anyType != null) {
                        if (typeof object.anyType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.anyType: object expected");
                        message.anyType = $root.baml.cffi.v1.BamlFieldTypeAny.fromObject(object.anyType);
                    }
                    if (object.uint8arrayType != null) {
                        if (typeof object.uint8arrayType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.uint8arrayType: object expected");
                        message.uint8arrayType = $root.baml.cffi.v1.BamlFieldTypeUint8Array.fromObject(object.uint8arrayType);
                    }
                    if (object.unknownType != null) {
                        if (typeof object.unknownType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldType.unknownType: object expected");
                        message.unknownType = $root.baml.cffi.v1.BamlFieldTypeUnknown.fromObject(object.unknownType);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlFieldType message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @static
                 * @param {baml.cffi.v1.BamlFieldType} message BamlFieldType
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldType.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (message.stringType != null && message.hasOwnProperty("stringType")) {
                        object.stringType = $root.baml.cffi.v1.BamlFieldTypeString.toObject(message.stringType, options);
                        if (options.oneofs)
                            object.type = "stringType";
                    }
                    if (message.intType != null && message.hasOwnProperty("intType")) {
                        object.intType = $root.baml.cffi.v1.BamlFieldTypeInt.toObject(message.intType, options);
                        if (options.oneofs)
                            object.type = "intType";
                    }
                    if (message.floatType != null && message.hasOwnProperty("floatType")) {
                        object.floatType = $root.baml.cffi.v1.BamlFieldTypeFloat.toObject(message.floatType, options);
                        if (options.oneofs)
                            object.type = "floatType";
                    }
                    if (message.boolType != null && message.hasOwnProperty("boolType")) {
                        object.boolType = $root.baml.cffi.v1.BamlFieldTypeBool.toObject(message.boolType, options);
                        if (options.oneofs)
                            object.type = "boolType";
                    }
                    if (message.nullType != null && message.hasOwnProperty("nullType")) {
                        object.nullType = $root.baml.cffi.v1.BamlFieldTypeNull.toObject(message.nullType, options);
                        if (options.oneofs)
                            object.type = "nullType";
                    }
                    if (message.literalType != null && message.hasOwnProperty("literalType")) {
                        object.literalType = $root.baml.cffi.v1.BamlFieldTypeLiteral.toObject(message.literalType, options);
                        if (options.oneofs)
                            object.type = "literalType";
                    }
                    if (message.mediaType != null && message.hasOwnProperty("mediaType")) {
                        object.mediaType = $root.baml.cffi.v1.BamlFieldTypeMedia.toObject(message.mediaType, options);
                        if (options.oneofs)
                            object.type = "mediaType";
                    }
                    if (message.enumType != null && message.hasOwnProperty("enumType")) {
                        object.enumType = $root.baml.cffi.v1.BamlFieldTypeEnum.toObject(message.enumType, options);
                        if (options.oneofs)
                            object.type = "enumType";
                    }
                    if (message.classType != null && message.hasOwnProperty("classType")) {
                        object.classType = $root.baml.cffi.v1.BamlFieldTypeClass.toObject(message.classType, options);
                        if (options.oneofs)
                            object.type = "classType";
                    }
                    if (message.typeAliasType != null && message.hasOwnProperty("typeAliasType")) {
                        object.typeAliasType = $root.baml.cffi.v1.BamlFieldTypeTypeAlias.toObject(message.typeAliasType, options);
                        if (options.oneofs)
                            object.type = "typeAliasType";
                    }
                    if (message.listType != null && message.hasOwnProperty("listType")) {
                        object.listType = $root.baml.cffi.v1.BamlFieldTypeList.toObject(message.listType, options);
                        if (options.oneofs)
                            object.type = "listType";
                    }
                    if (message.mapType != null && message.hasOwnProperty("mapType")) {
                        object.mapType = $root.baml.cffi.v1.BamlFieldTypeMap.toObject(message.mapType, options);
                        if (options.oneofs)
                            object.type = "mapType";
                    }
                    if (message.unionVariantType != null && message.hasOwnProperty("unionVariantType")) {
                        object.unionVariantType = $root.baml.cffi.v1.BamlFieldTypeUnionVariant.toObject(message.unionVariantType, options);
                        if (options.oneofs)
                            object.type = "unionVariantType";
                    }
                    if (message.optionalType != null && message.hasOwnProperty("optionalType")) {
                        object.optionalType = $root.baml.cffi.v1.BamlFieldTypeOptional.toObject(message.optionalType, options);
                        if (options.oneofs)
                            object.type = "optionalType";
                    }
                    if (message.checkedType != null && message.hasOwnProperty("checkedType")) {
                        object.checkedType = $root.baml.cffi.v1.BamlFieldTypeChecked.toObject(message.checkedType, options);
                        if (options.oneofs)
                            object.type = "checkedType";
                    }
                    if (message.streamStateType != null && message.hasOwnProperty("streamStateType")) {
                        object.streamStateType = $root.baml.cffi.v1.BamlFieldTypeStreamState.toObject(message.streamStateType, options);
                        if (options.oneofs)
                            object.type = "streamStateType";
                    }
                    if (message.anyType != null && message.hasOwnProperty("anyType")) {
                        object.anyType = $root.baml.cffi.v1.BamlFieldTypeAny.toObject(message.anyType, options);
                        if (options.oneofs)
                            object.type = "anyType";
                    }
                    if (message.uint8arrayType != null && message.hasOwnProperty("uint8arrayType")) {
                        object.uint8arrayType = $root.baml.cffi.v1.BamlFieldTypeUint8Array.toObject(message.uint8arrayType, options);
                        if (options.oneofs)
                            object.type = "uint8arrayType";
                    }
                    if (message.unknownType != null && message.hasOwnProperty("unknownType")) {
                        object.unknownType = $root.baml.cffi.v1.BamlFieldTypeUnknown.toObject(message.unknownType, options);
                        if (options.oneofs)
                            object.type = "unknownType";
                    }
                    return object;
                };

                /**
                 * Converts this BamlFieldType to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldType.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldType
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldType
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldType.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldType";
                };

                return BamlFieldType;
            })();

            v1.BamlFieldTypeString = (function() {

                /**
                 * Properties of a BamlFieldTypeString.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeString
                 */

                /**
                 * Constructs a new BamlFieldTypeString.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeString.
                 * @implements IBamlFieldTypeString
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeString=} [properties] Properties to set
                 */
                function BamlFieldTypeString(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlFieldTypeString instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeString
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeString=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeString} BamlFieldTypeString instance
                 */
                BamlFieldTypeString.create = function create(properties) {
                    return new BamlFieldTypeString(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeString message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeString.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeString
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeString} message BamlFieldTypeString message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeString.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeString message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeString.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeString
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeString} message BamlFieldTypeString message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeString.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeString message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeString
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeString} BamlFieldTypeString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeString.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeString();
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
                 * Decodes a BamlFieldTypeString message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeString
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeString} BamlFieldTypeString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeString.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeString message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeString
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeString.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeString message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeString
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeString} BamlFieldTypeString
                 */
                BamlFieldTypeString.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeString)
                        return object;
                    return new $root.baml.cffi.v1.BamlFieldTypeString();
                };

                /**
                 * Creates a plain object from a BamlFieldTypeString message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeString
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeString} message BamlFieldTypeString
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeString.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlFieldTypeString to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeString
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeString.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeString
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeString
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeString.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeString";
                };

                return BamlFieldTypeString;
            })();

            v1.BamlFieldTypeInt = (function() {

                /**
                 * Properties of a BamlFieldTypeInt.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeInt
                 */

                /**
                 * Constructs a new BamlFieldTypeInt.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeInt.
                 * @implements IBamlFieldTypeInt
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeInt=} [properties] Properties to set
                 */
                function BamlFieldTypeInt(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlFieldTypeInt instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeInt
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeInt=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeInt} BamlFieldTypeInt instance
                 */
                BamlFieldTypeInt.create = function create(properties) {
                    return new BamlFieldTypeInt(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeInt message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeInt.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeInt
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeInt} message BamlFieldTypeInt message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeInt.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeInt message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeInt.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeInt
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeInt} message BamlFieldTypeInt message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeInt.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeInt message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeInt
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeInt} BamlFieldTypeInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeInt.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeInt();
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
                 * Decodes a BamlFieldTypeInt message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeInt
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeInt} BamlFieldTypeInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeInt.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeInt message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeInt
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeInt.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeInt message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeInt
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeInt} BamlFieldTypeInt
                 */
                BamlFieldTypeInt.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeInt)
                        return object;
                    return new $root.baml.cffi.v1.BamlFieldTypeInt();
                };

                /**
                 * Creates a plain object from a BamlFieldTypeInt message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeInt
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeInt} message BamlFieldTypeInt
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeInt.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlFieldTypeInt to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeInt
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeInt.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeInt
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeInt
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeInt.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeInt";
                };

                return BamlFieldTypeInt;
            })();

            v1.BamlFieldTypeFloat = (function() {

                /**
                 * Properties of a BamlFieldTypeFloat.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeFloat
                 */

                /**
                 * Constructs a new BamlFieldTypeFloat.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeFloat.
                 * @implements IBamlFieldTypeFloat
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeFloat=} [properties] Properties to set
                 */
                function BamlFieldTypeFloat(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlFieldTypeFloat instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeFloat
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeFloat=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeFloat} BamlFieldTypeFloat instance
                 */
                BamlFieldTypeFloat.create = function create(properties) {
                    return new BamlFieldTypeFloat(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeFloat message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeFloat.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeFloat
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeFloat} message BamlFieldTypeFloat message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeFloat.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeFloat message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeFloat.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeFloat
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeFloat} message BamlFieldTypeFloat message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeFloat.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeFloat message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeFloat
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeFloat} BamlFieldTypeFloat
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeFloat.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeFloat();
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
                 * Decodes a BamlFieldTypeFloat message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeFloat
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeFloat} BamlFieldTypeFloat
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeFloat.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeFloat message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeFloat
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeFloat.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeFloat message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeFloat
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeFloat} BamlFieldTypeFloat
                 */
                BamlFieldTypeFloat.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeFloat)
                        return object;
                    return new $root.baml.cffi.v1.BamlFieldTypeFloat();
                };

                /**
                 * Creates a plain object from a BamlFieldTypeFloat message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeFloat
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeFloat} message BamlFieldTypeFloat
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeFloat.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlFieldTypeFloat to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeFloat
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeFloat.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeFloat
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeFloat
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeFloat.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeFloat";
                };

                return BamlFieldTypeFloat;
            })();

            v1.BamlFieldTypeBool = (function() {

                /**
                 * Properties of a BamlFieldTypeBool.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeBool
                 */

                /**
                 * Constructs a new BamlFieldTypeBool.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeBool.
                 * @implements IBamlFieldTypeBool
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeBool=} [properties] Properties to set
                 */
                function BamlFieldTypeBool(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlFieldTypeBool instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeBool
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeBool=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeBool} BamlFieldTypeBool instance
                 */
                BamlFieldTypeBool.create = function create(properties) {
                    return new BamlFieldTypeBool(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeBool message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeBool.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeBool
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeBool} message BamlFieldTypeBool message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeBool.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeBool message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeBool.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeBool
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeBool} message BamlFieldTypeBool message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeBool.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeBool message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeBool
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeBool} BamlFieldTypeBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeBool.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeBool();
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
                 * Decodes a BamlFieldTypeBool message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeBool
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeBool} BamlFieldTypeBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeBool.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeBool message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeBool
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeBool.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeBool message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeBool
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeBool} BamlFieldTypeBool
                 */
                BamlFieldTypeBool.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeBool)
                        return object;
                    return new $root.baml.cffi.v1.BamlFieldTypeBool();
                };

                /**
                 * Creates a plain object from a BamlFieldTypeBool message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeBool
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeBool} message BamlFieldTypeBool
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeBool.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlFieldTypeBool to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeBool
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeBool.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeBool
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeBool
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeBool.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeBool";
                };

                return BamlFieldTypeBool;
            })();

            v1.BamlFieldTypeNull = (function() {

                /**
                 * Properties of a BamlFieldTypeNull.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeNull
                 */

                /**
                 * Constructs a new BamlFieldTypeNull.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeNull.
                 * @implements IBamlFieldTypeNull
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeNull=} [properties] Properties to set
                 */
                function BamlFieldTypeNull(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlFieldTypeNull instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeNull
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeNull=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeNull} BamlFieldTypeNull instance
                 */
                BamlFieldTypeNull.create = function create(properties) {
                    return new BamlFieldTypeNull(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeNull message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeNull.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeNull
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeNull} message BamlFieldTypeNull message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeNull.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeNull message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeNull.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeNull
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeNull} message BamlFieldTypeNull message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeNull.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeNull message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeNull
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeNull} BamlFieldTypeNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeNull.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeNull();
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
                 * Decodes a BamlFieldTypeNull message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeNull
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeNull} BamlFieldTypeNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeNull.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeNull message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeNull
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeNull.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeNull message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeNull
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeNull} BamlFieldTypeNull
                 */
                BamlFieldTypeNull.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeNull)
                        return object;
                    return new $root.baml.cffi.v1.BamlFieldTypeNull();
                };

                /**
                 * Creates a plain object from a BamlFieldTypeNull message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeNull
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeNull} message BamlFieldTypeNull
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeNull.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlFieldTypeNull to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeNull
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeNull.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeNull
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeNull
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeNull.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeNull";
                };

                return BamlFieldTypeNull;
            })();

            v1.BamlFieldTypeUint8Array = (function() {

                /**
                 * Properties of a BamlFieldTypeUint8Array.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeUint8Array
                 */

                /**
                 * Constructs a new BamlFieldTypeUint8Array.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeUint8Array.
                 * @implements IBamlFieldTypeUint8Array
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeUint8Array=} [properties] Properties to set
                 */
                function BamlFieldTypeUint8Array(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlFieldTypeUint8Array instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeUint8Array
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeUint8Array=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeUint8Array} BamlFieldTypeUint8Array instance
                 */
                BamlFieldTypeUint8Array.create = function create(properties) {
                    return new BamlFieldTypeUint8Array(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeUint8Array message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeUint8Array.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeUint8Array
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeUint8Array} message BamlFieldTypeUint8Array message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeUint8Array.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeUint8Array message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeUint8Array.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeUint8Array
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeUint8Array} message BamlFieldTypeUint8Array message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeUint8Array.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeUint8Array message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeUint8Array
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeUint8Array} BamlFieldTypeUint8Array
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeUint8Array.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeUint8Array();
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
                 * Decodes a BamlFieldTypeUint8Array message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeUint8Array
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeUint8Array} BamlFieldTypeUint8Array
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeUint8Array.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeUint8Array message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeUint8Array
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeUint8Array.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeUint8Array message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeUint8Array
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeUint8Array} BamlFieldTypeUint8Array
                 */
                BamlFieldTypeUint8Array.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeUint8Array)
                        return object;
                    return new $root.baml.cffi.v1.BamlFieldTypeUint8Array();
                };

                /**
                 * Creates a plain object from a BamlFieldTypeUint8Array message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeUint8Array
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeUint8Array} message BamlFieldTypeUint8Array
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeUint8Array.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlFieldTypeUint8Array to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeUint8Array
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeUint8Array.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeUint8Array
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeUint8Array
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeUint8Array.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeUint8Array";
                };

                return BamlFieldTypeUint8Array;
            })();

            v1.BamlFieldTypeAny = (function() {

                /**
                 * Properties of a BamlFieldTypeAny.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeAny
                 */

                /**
                 * Constructs a new BamlFieldTypeAny.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeAny.
                 * @implements IBamlFieldTypeAny
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeAny=} [properties] Properties to set
                 */
                function BamlFieldTypeAny(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlFieldTypeAny instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeAny
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeAny=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeAny} BamlFieldTypeAny instance
                 */
                BamlFieldTypeAny.create = function create(properties) {
                    return new BamlFieldTypeAny(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeAny message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeAny.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeAny
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeAny} message BamlFieldTypeAny message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeAny.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeAny message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeAny.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeAny
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeAny} message BamlFieldTypeAny message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeAny.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeAny message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeAny
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeAny} BamlFieldTypeAny
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeAny.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeAny();
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
                 * Decodes a BamlFieldTypeAny message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeAny
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeAny} BamlFieldTypeAny
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeAny.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeAny message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeAny
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeAny.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeAny message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeAny
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeAny} BamlFieldTypeAny
                 */
                BamlFieldTypeAny.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeAny)
                        return object;
                    return new $root.baml.cffi.v1.BamlFieldTypeAny();
                };

                /**
                 * Creates a plain object from a BamlFieldTypeAny message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeAny
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeAny} message BamlFieldTypeAny
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeAny.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlFieldTypeAny to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeAny
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeAny.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeAny
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeAny
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeAny.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeAny";
                };

                return BamlFieldTypeAny;
            })();

            v1.BamlFieldTypeUnknown = (function() {

                /**
                 * Properties of a BamlFieldTypeUnknown.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeUnknown
                 */

                /**
                 * Constructs a new BamlFieldTypeUnknown.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeUnknown.
                 * @implements IBamlFieldTypeUnknown
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeUnknown=} [properties] Properties to set
                 */
                function BamlFieldTypeUnknown(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Creates a new BamlFieldTypeUnknown instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeUnknown
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeUnknown=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeUnknown} BamlFieldTypeUnknown instance
                 */
                BamlFieldTypeUnknown.create = function create(properties) {
                    return new BamlFieldTypeUnknown(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeUnknown message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeUnknown.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeUnknown
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeUnknown} message BamlFieldTypeUnknown message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeUnknown.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeUnknown message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeUnknown.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeUnknown
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeUnknown} message BamlFieldTypeUnknown message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeUnknown.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeUnknown message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeUnknown
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeUnknown} BamlFieldTypeUnknown
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeUnknown.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeUnknown();
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
                 * Decodes a BamlFieldTypeUnknown message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeUnknown
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeUnknown} BamlFieldTypeUnknown
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeUnknown.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeUnknown message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeUnknown
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeUnknown.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeUnknown message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeUnknown
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeUnknown} BamlFieldTypeUnknown
                 */
                BamlFieldTypeUnknown.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeUnknown)
                        return object;
                    return new $root.baml.cffi.v1.BamlFieldTypeUnknown();
                };

                /**
                 * Creates a plain object from a BamlFieldTypeUnknown message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeUnknown
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeUnknown} message BamlFieldTypeUnknown
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeUnknown.toObject = function toObject() {
                    return {};
                };

                /**
                 * Converts this BamlFieldTypeUnknown to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeUnknown
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeUnknown.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeUnknown
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeUnknown
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeUnknown.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeUnknown";
                };

                return BamlFieldTypeUnknown;
            })();

            v1.BamlLiteralString = (function() {

                /**
                 * Properties of a BamlLiteralString.
                 * @memberof baml.cffi.v1
                 * @interface IBamlLiteralString
                 * @property {string|null} [value] BamlLiteralString value
                 */

                /**
                 * Constructs a new BamlLiteralString.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlLiteralString.
                 * @implements IBamlLiteralString
                 * @constructor
                 * @param {baml.cffi.v1.IBamlLiteralString=} [properties] Properties to set
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
                 * @memberof baml.cffi.v1.BamlLiteralString
                 * @instance
                 */
                BamlLiteralString.prototype.value = "";

                /**
                 * Creates a new BamlLiteralString instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlLiteralString
                 * @static
                 * @param {baml.cffi.v1.IBamlLiteralString=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlLiteralString} BamlLiteralString instance
                 */
                BamlLiteralString.create = function create(properties) {
                    return new BamlLiteralString(properties);
                };

                /**
                 * Encodes the specified BamlLiteralString message. Does not implicitly {@link baml.cffi.v1.BamlLiteralString.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlLiteralString
                 * @static
                 * @param {baml.cffi.v1.IBamlLiteralString} message BamlLiteralString message or plain object to encode
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
                 * Encodes the specified BamlLiteralString message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlLiteralString.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlLiteralString
                 * @static
                 * @param {baml.cffi.v1.IBamlLiteralString} message BamlLiteralString message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlLiteralString.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlLiteralString message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlLiteralString
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlLiteralString} BamlLiteralString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlLiteralString.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlLiteralString();
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
                 * @memberof baml.cffi.v1.BamlLiteralString
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlLiteralString} BamlLiteralString
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
                 * @memberof baml.cffi.v1.BamlLiteralString
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
                 * @memberof baml.cffi.v1.BamlLiteralString
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlLiteralString} BamlLiteralString
                 */
                BamlLiteralString.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlLiteralString)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlLiteralString();
                    if (object.value != null)
                        message.value = String(object.value);
                    return message;
                };

                /**
                 * Creates a plain object from a BamlLiteralString message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlLiteralString
                 * @static
                 * @param {baml.cffi.v1.BamlLiteralString} message BamlLiteralString
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
                 * @memberof baml.cffi.v1.BamlLiteralString
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlLiteralString.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlLiteralString
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlLiteralString
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlLiteralString.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlLiteralString";
                };

                return BamlLiteralString;
            })();

            v1.BamlLiteralInt = (function() {

                /**
                 * Properties of a BamlLiteralInt.
                 * @memberof baml.cffi.v1
                 * @interface IBamlLiteralInt
                 * @property {number|Long|null} [value] BamlLiteralInt value
                 */

                /**
                 * Constructs a new BamlLiteralInt.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlLiteralInt.
                 * @implements IBamlLiteralInt
                 * @constructor
                 * @param {baml.cffi.v1.IBamlLiteralInt=} [properties] Properties to set
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
                 * @memberof baml.cffi.v1.BamlLiteralInt
                 * @instance
                 */
                BamlLiteralInt.prototype.value = $util.Long ? $util.Long.fromBits(0,0,false) : 0;

                /**
                 * Creates a new BamlLiteralInt instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {baml.cffi.v1.IBamlLiteralInt=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlLiteralInt} BamlLiteralInt instance
                 */
                BamlLiteralInt.create = function create(properties) {
                    return new BamlLiteralInt(properties);
                };

                /**
                 * Encodes the specified BamlLiteralInt message. Does not implicitly {@link baml.cffi.v1.BamlLiteralInt.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {baml.cffi.v1.IBamlLiteralInt} message BamlLiteralInt message or plain object to encode
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
                 * Encodes the specified BamlLiteralInt message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlLiteralInt.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {baml.cffi.v1.IBamlLiteralInt} message BamlLiteralInt message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlLiteralInt.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlLiteralInt message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlLiteralInt} BamlLiteralInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlLiteralInt.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlLiteralInt();
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
                 * @memberof baml.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlLiteralInt} BamlLiteralInt
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
                 * @memberof baml.cffi.v1.BamlLiteralInt
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
                 * @memberof baml.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlLiteralInt} BamlLiteralInt
                 */
                BamlLiteralInt.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlLiteralInt)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlLiteralInt();
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
                 * @memberof baml.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {baml.cffi.v1.BamlLiteralInt} message BamlLiteralInt
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
                 * @memberof baml.cffi.v1.BamlLiteralInt
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlLiteralInt.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlLiteralInt
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlLiteralInt
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlLiteralInt.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlLiteralInt";
                };

                return BamlLiteralInt;
            })();

            v1.BamlLiteralBool = (function() {

                /**
                 * Properties of a BamlLiteralBool.
                 * @memberof baml.cffi.v1
                 * @interface IBamlLiteralBool
                 * @property {boolean|null} [value] BamlLiteralBool value
                 */

                /**
                 * Constructs a new BamlLiteralBool.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlLiteralBool.
                 * @implements IBamlLiteralBool
                 * @constructor
                 * @param {baml.cffi.v1.IBamlLiteralBool=} [properties] Properties to set
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
                 * @memberof baml.cffi.v1.BamlLiteralBool
                 * @instance
                 */
                BamlLiteralBool.prototype.value = false;

                /**
                 * Creates a new BamlLiteralBool instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {baml.cffi.v1.IBamlLiteralBool=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlLiteralBool} BamlLiteralBool instance
                 */
                BamlLiteralBool.create = function create(properties) {
                    return new BamlLiteralBool(properties);
                };

                /**
                 * Encodes the specified BamlLiteralBool message. Does not implicitly {@link baml.cffi.v1.BamlLiteralBool.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {baml.cffi.v1.IBamlLiteralBool} message BamlLiteralBool message or plain object to encode
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
                 * Encodes the specified BamlLiteralBool message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlLiteralBool.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {baml.cffi.v1.IBamlLiteralBool} message BamlLiteralBool message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlLiteralBool.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlLiteralBool message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlLiteralBool} BamlLiteralBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlLiteralBool.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlLiteralBool();
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
                 * @memberof baml.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlLiteralBool} BamlLiteralBool
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
                 * @memberof baml.cffi.v1.BamlLiteralBool
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
                 * @memberof baml.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlLiteralBool} BamlLiteralBool
                 */
                BamlLiteralBool.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlLiteralBool)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlLiteralBool();
                    if (object.value != null)
                        message.value = Boolean(object.value);
                    return message;
                };

                /**
                 * Creates a plain object from a BamlLiteralBool message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {baml.cffi.v1.BamlLiteralBool} message BamlLiteralBool
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
                 * @memberof baml.cffi.v1.BamlLiteralBool
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlLiteralBool.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlLiteralBool
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlLiteralBool
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlLiteralBool.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlLiteralBool";
                };

                return BamlLiteralBool;
            })();

            v1.BamlFieldTypeLiteral = (function() {

                /**
                 * Properties of a BamlFieldTypeLiteral.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeLiteral
                 * @property {baml.cffi.v1.IBamlLiteralString|null} [stringLiteral] BamlFieldTypeLiteral stringLiteral
                 * @property {baml.cffi.v1.IBamlLiteralInt|null} [intLiteral] BamlFieldTypeLiteral intLiteral
                 * @property {baml.cffi.v1.IBamlLiteralBool|null} [boolLiteral] BamlFieldTypeLiteral boolLiteral
                 */

                /**
                 * Constructs a new BamlFieldTypeLiteral.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeLiteral.
                 * @implements IBamlFieldTypeLiteral
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeLiteral=} [properties] Properties to set
                 */
                function BamlFieldTypeLiteral(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlFieldTypeLiteral stringLiteral.
                 * @member {baml.cffi.v1.IBamlLiteralString|null|undefined} stringLiteral
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @instance
                 */
                BamlFieldTypeLiteral.prototype.stringLiteral = null;

                /**
                 * BamlFieldTypeLiteral intLiteral.
                 * @member {baml.cffi.v1.IBamlLiteralInt|null|undefined} intLiteral
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @instance
                 */
                BamlFieldTypeLiteral.prototype.intLiteral = null;

                /**
                 * BamlFieldTypeLiteral boolLiteral.
                 * @member {baml.cffi.v1.IBamlLiteralBool|null|undefined} boolLiteral
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @instance
                 */
                BamlFieldTypeLiteral.prototype.boolLiteral = null;

                // OneOf field names bound to virtual getters and setters
                var $oneOfFields;

                /**
                 * BamlFieldTypeLiteral literal.
                 * @member {"stringLiteral"|"intLiteral"|"boolLiteral"|undefined} literal
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @instance
                 */
                Object.defineProperty(BamlFieldTypeLiteral.prototype, "literal", {
                    get: $util.oneOfGetter($oneOfFields = ["stringLiteral", "intLiteral", "boolLiteral"]),
                    set: $util.oneOfSetter($oneOfFields)
                });

                /**
                 * Creates a new BamlFieldTypeLiteral instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeLiteral=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeLiteral} BamlFieldTypeLiteral instance
                 */
                BamlFieldTypeLiteral.create = function create(properties) {
                    return new BamlFieldTypeLiteral(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeLiteral message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeLiteral.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeLiteral} message BamlFieldTypeLiteral message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeLiteral.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.stringLiteral != null && Object.hasOwnProperty.call(message, "stringLiteral"))
                        $root.baml.cffi.v1.BamlLiteralString.encode(message.stringLiteral, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.intLiteral != null && Object.hasOwnProperty.call(message, "intLiteral"))
                        $root.baml.cffi.v1.BamlLiteralInt.encode(message.intLiteral, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    if (message.boolLiteral != null && Object.hasOwnProperty.call(message, "boolLiteral"))
                        $root.baml.cffi.v1.BamlLiteralBool.encode(message.boolLiteral, writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeLiteral message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeLiteral.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeLiteral} message BamlFieldTypeLiteral message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeLiteral.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeLiteral message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeLiteral} BamlFieldTypeLiteral
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeLiteral.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeLiteral();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.stringLiteral = $root.baml.cffi.v1.BamlLiteralString.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.intLiteral = $root.baml.cffi.v1.BamlLiteralInt.decode(reader, reader.uint32());
                                break;
                            }
                        case 3: {
                                message.boolLiteral = $root.baml.cffi.v1.BamlLiteralBool.decode(reader, reader.uint32());
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
                 * Decodes a BamlFieldTypeLiteral message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeLiteral} BamlFieldTypeLiteral
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeLiteral.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeLiteral message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeLiteral.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    var properties = {};
                    if (message.stringLiteral != null && message.hasOwnProperty("stringLiteral")) {
                        properties.literal = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlLiteralString.verify(message.stringLiteral);
                            if (error)
                                return "stringLiteral." + error;
                        }
                    }
                    if (message.intLiteral != null && message.hasOwnProperty("intLiteral")) {
                        if (properties.literal === 1)
                            return "literal: multiple values";
                        properties.literal = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlLiteralInt.verify(message.intLiteral);
                            if (error)
                                return "intLiteral." + error;
                        }
                    }
                    if (message.boolLiteral != null && message.hasOwnProperty("boolLiteral")) {
                        if (properties.literal === 1)
                            return "literal: multiple values";
                        properties.literal = 1;
                        {
                            var error = $root.baml.cffi.v1.BamlLiteralBool.verify(message.boolLiteral);
                            if (error)
                                return "boolLiteral." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeLiteral message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeLiteral} BamlFieldTypeLiteral
                 */
                BamlFieldTypeLiteral.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeLiteral)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlFieldTypeLiteral();
                    if (object.stringLiteral != null) {
                        if (typeof object.stringLiteral !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldTypeLiteral.stringLiteral: object expected");
                        message.stringLiteral = $root.baml.cffi.v1.BamlLiteralString.fromObject(object.stringLiteral);
                    }
                    if (object.intLiteral != null) {
                        if (typeof object.intLiteral !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldTypeLiteral.intLiteral: object expected");
                        message.intLiteral = $root.baml.cffi.v1.BamlLiteralInt.fromObject(object.intLiteral);
                    }
                    if (object.boolLiteral != null) {
                        if (typeof object.boolLiteral !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldTypeLiteral.boolLiteral: object expected");
                        message.boolLiteral = $root.baml.cffi.v1.BamlLiteralBool.fromObject(object.boolLiteral);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlFieldTypeLiteral message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeLiteral} message BamlFieldTypeLiteral
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeLiteral.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (message.stringLiteral != null && message.hasOwnProperty("stringLiteral")) {
                        object.stringLiteral = $root.baml.cffi.v1.BamlLiteralString.toObject(message.stringLiteral, options);
                        if (options.oneofs)
                            object.literal = "stringLiteral";
                    }
                    if (message.intLiteral != null && message.hasOwnProperty("intLiteral")) {
                        object.intLiteral = $root.baml.cffi.v1.BamlLiteralInt.toObject(message.intLiteral, options);
                        if (options.oneofs)
                            object.literal = "intLiteral";
                    }
                    if (message.boolLiteral != null && message.hasOwnProperty("boolLiteral")) {
                        object.boolLiteral = $root.baml.cffi.v1.BamlLiteralBool.toObject(message.boolLiteral, options);
                        if (options.oneofs)
                            object.literal = "boolLiteral";
                    }
                    return object;
                };

                /**
                 * Converts this BamlFieldTypeLiteral to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeLiteral.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeLiteral
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeLiteral
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeLiteral.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeLiteral";
                };

                return BamlFieldTypeLiteral;
            })();

            v1.BamlFieldTypeMedia = (function() {

                /**
                 * Properties of a BamlFieldTypeMedia.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeMedia
                 * @property {baml.cffi.v1.MediaTypeEnum|null} [media] BamlFieldTypeMedia media
                 */

                /**
                 * Constructs a new BamlFieldTypeMedia.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeMedia.
                 * @implements IBamlFieldTypeMedia
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeMedia=} [properties] Properties to set
                 */
                function BamlFieldTypeMedia(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlFieldTypeMedia media.
                 * @member {baml.cffi.v1.MediaTypeEnum} media
                 * @memberof baml.cffi.v1.BamlFieldTypeMedia
                 * @instance
                 */
                BamlFieldTypeMedia.prototype.media = 0;

                /**
                 * Creates a new BamlFieldTypeMedia instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeMedia
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeMedia=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeMedia} BamlFieldTypeMedia instance
                 */
                BamlFieldTypeMedia.create = function create(properties) {
                    return new BamlFieldTypeMedia(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeMedia message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeMedia.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeMedia
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeMedia} message BamlFieldTypeMedia message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeMedia.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.media != null && Object.hasOwnProperty.call(message, "media"))
                        writer.uint32(/* id 1, wireType 0 =*/8).int32(message.media);
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeMedia message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeMedia.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeMedia
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeMedia} message BamlFieldTypeMedia message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeMedia.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeMedia message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeMedia
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeMedia} BamlFieldTypeMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeMedia.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeMedia();
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
                 * Decodes a BamlFieldTypeMedia message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeMedia
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeMedia} BamlFieldTypeMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeMedia.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeMedia message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeMedia
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeMedia.verify = function verify(message) {
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
                 * Creates a BamlFieldTypeMedia message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeMedia
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeMedia} BamlFieldTypeMedia
                 */
                BamlFieldTypeMedia.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeMedia)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlFieldTypeMedia();
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
                 * Creates a plain object from a BamlFieldTypeMedia message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeMedia
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeMedia} message BamlFieldTypeMedia
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeMedia.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.media = options.enums === String ? "MEDIA_TYPE_UNSPECIFIED" : 0;
                    if (message.media != null && message.hasOwnProperty("media"))
                        object.media = options.enums === String ? $root.baml.cffi.v1.MediaTypeEnum[message.media] === undefined ? message.media : $root.baml.cffi.v1.MediaTypeEnum[message.media] : message.media;
                    return object;
                };

                /**
                 * Converts this BamlFieldTypeMedia to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeMedia
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeMedia.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeMedia
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeMedia
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeMedia.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeMedia";
                };

                return BamlFieldTypeMedia;
            })();

            v1.BamlFieldTypeEnum = (function() {

                /**
                 * Properties of a BamlFieldTypeEnum.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeEnum
                 * @property {string|null} [name] BamlFieldTypeEnum name
                 */

                /**
                 * Constructs a new BamlFieldTypeEnum.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeEnum.
                 * @implements IBamlFieldTypeEnum
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeEnum=} [properties] Properties to set
                 */
                function BamlFieldTypeEnum(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlFieldTypeEnum name.
                 * @member {string} name
                 * @memberof baml.cffi.v1.BamlFieldTypeEnum
                 * @instance
                 */
                BamlFieldTypeEnum.prototype.name = "";

                /**
                 * Creates a new BamlFieldTypeEnum instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeEnum
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeEnum=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeEnum} BamlFieldTypeEnum instance
                 */
                BamlFieldTypeEnum.create = function create(properties) {
                    return new BamlFieldTypeEnum(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeEnum message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeEnum.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeEnum
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeEnum} message BamlFieldTypeEnum message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeEnum.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.name);
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeEnum message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeEnum.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeEnum
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeEnum} message BamlFieldTypeEnum message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeEnum.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeEnum message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeEnum
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeEnum} BamlFieldTypeEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeEnum.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeEnum();
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
                 * Decodes a BamlFieldTypeEnum message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeEnum
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeEnum} BamlFieldTypeEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeEnum.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeEnum message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeEnum
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeEnum.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name"))
                        if (!$util.isString(message.name))
                            return "name: string expected";
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeEnum message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeEnum
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeEnum} BamlFieldTypeEnum
                 */
                BamlFieldTypeEnum.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeEnum)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlFieldTypeEnum();
                    if (object.name != null)
                        message.name = String(object.name);
                    return message;
                };

                /**
                 * Creates a plain object from a BamlFieldTypeEnum message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeEnum
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeEnum} message BamlFieldTypeEnum
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeEnum.toObject = function toObject(message, options) {
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
                 * Converts this BamlFieldTypeEnum to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeEnum
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeEnum.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeEnum
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeEnum
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeEnum.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeEnum";
                };

                return BamlFieldTypeEnum;
            })();

            v1.BamlFieldTypeClass = (function() {

                /**
                 * Properties of a BamlFieldTypeClass.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeClass
                 * @property {baml.cffi.v1.IBamlTypeName|null} [name] BamlFieldTypeClass name
                 */

                /**
                 * Constructs a new BamlFieldTypeClass.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeClass.
                 * @implements IBamlFieldTypeClass
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeClass=} [properties] Properties to set
                 */
                function BamlFieldTypeClass(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlFieldTypeClass name.
                 * @member {baml.cffi.v1.IBamlTypeName|null|undefined} name
                 * @memberof baml.cffi.v1.BamlFieldTypeClass
                 * @instance
                 */
                BamlFieldTypeClass.prototype.name = null;

                /**
                 * Creates a new BamlFieldTypeClass instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeClass
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeClass=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeClass} BamlFieldTypeClass instance
                 */
                BamlFieldTypeClass.create = function create(properties) {
                    return new BamlFieldTypeClass(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeClass message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeClass.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeClass
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeClass} message BamlFieldTypeClass message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeClass.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml.cffi.v1.BamlTypeName.encode(message.name, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeClass message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeClass.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeClass
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeClass} message BamlFieldTypeClass message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeClass.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeClass message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeClass
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeClass} BamlFieldTypeClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeClass.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeClass();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = $root.baml.cffi.v1.BamlTypeName.decode(reader, reader.uint32());
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
                 * Decodes a BamlFieldTypeClass message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeClass
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeClass} BamlFieldTypeClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeClass.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeClass message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeClass
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeClass.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml.cffi.v1.BamlTypeName.verify(message.name);
                        if (error)
                            return "name." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeClass message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeClass
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeClass} BamlFieldTypeClass
                 */
                BamlFieldTypeClass.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeClass)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlFieldTypeClass();
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldTypeClass.name: object expected");
                        message.name = $root.baml.cffi.v1.BamlTypeName.fromObject(object.name);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlFieldTypeClass message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeClass
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeClass} message BamlFieldTypeClass
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeClass.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.name = null;
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = $root.baml.cffi.v1.BamlTypeName.toObject(message.name, options);
                    return object;
                };

                /**
                 * Converts this BamlFieldTypeClass to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeClass
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeClass.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeClass
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeClass
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeClass.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeClass";
                };

                return BamlFieldTypeClass;
            })();

            v1.BamlFieldTypeTypeAlias = (function() {

                /**
                 * Properties of a BamlFieldTypeTypeAlias.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeTypeAlias
                 * @property {baml.cffi.v1.IBamlTypeName|null} [name] BamlFieldTypeTypeAlias name
                 */

                /**
                 * Constructs a new BamlFieldTypeTypeAlias.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeTypeAlias.
                 * @implements IBamlFieldTypeTypeAlias
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeTypeAlias=} [properties] Properties to set
                 */
                function BamlFieldTypeTypeAlias(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlFieldTypeTypeAlias name.
                 * @member {baml.cffi.v1.IBamlTypeName|null|undefined} name
                 * @memberof baml.cffi.v1.BamlFieldTypeTypeAlias
                 * @instance
                 */
                BamlFieldTypeTypeAlias.prototype.name = null;

                /**
                 * Creates a new BamlFieldTypeTypeAlias instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeTypeAlias
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeTypeAlias=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeTypeAlias} BamlFieldTypeTypeAlias instance
                 */
                BamlFieldTypeTypeAlias.create = function create(properties) {
                    return new BamlFieldTypeTypeAlias(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeTypeAlias message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeTypeAlias.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeTypeAlias
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeTypeAlias} message BamlFieldTypeTypeAlias message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeTypeAlias.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml.cffi.v1.BamlTypeName.encode(message.name, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeTypeAlias message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeTypeAlias.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeTypeAlias
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeTypeAlias} message BamlFieldTypeTypeAlias message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeTypeAlias.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeTypeAlias message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeTypeAlias
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeTypeAlias} BamlFieldTypeTypeAlias
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeTypeAlias.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeTypeAlias();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = $root.baml.cffi.v1.BamlTypeName.decode(reader, reader.uint32());
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
                 * Decodes a BamlFieldTypeTypeAlias message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeTypeAlias
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeTypeAlias} BamlFieldTypeTypeAlias
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeTypeAlias.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeTypeAlias message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeTypeAlias
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeTypeAlias.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml.cffi.v1.BamlTypeName.verify(message.name);
                        if (error)
                            return "name." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeTypeAlias message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeTypeAlias
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeTypeAlias} BamlFieldTypeTypeAlias
                 */
                BamlFieldTypeTypeAlias.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeTypeAlias)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlFieldTypeTypeAlias();
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldTypeTypeAlias.name: object expected");
                        message.name = $root.baml.cffi.v1.BamlTypeName.fromObject(object.name);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlFieldTypeTypeAlias message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeTypeAlias
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeTypeAlias} message BamlFieldTypeTypeAlias
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeTypeAlias.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.name = null;
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = $root.baml.cffi.v1.BamlTypeName.toObject(message.name, options);
                    return object;
                };

                /**
                 * Converts this BamlFieldTypeTypeAlias to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeTypeAlias
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeTypeAlias.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeTypeAlias
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeTypeAlias
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeTypeAlias.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeTypeAlias";
                };

                return BamlFieldTypeTypeAlias;
            })();

            v1.BamlFieldTypeList = (function() {

                /**
                 * Properties of a BamlFieldTypeList.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeList
                 * @property {baml.cffi.v1.IBamlFieldType|null} [itemType] BamlFieldTypeList itemType
                 */

                /**
                 * Constructs a new BamlFieldTypeList.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeList.
                 * @implements IBamlFieldTypeList
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeList=} [properties] Properties to set
                 */
                function BamlFieldTypeList(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlFieldTypeList itemType.
                 * @member {baml.cffi.v1.IBamlFieldType|null|undefined} itemType
                 * @memberof baml.cffi.v1.BamlFieldTypeList
                 * @instance
                 */
                BamlFieldTypeList.prototype.itemType = null;

                /**
                 * Creates a new BamlFieldTypeList instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeList
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeList=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeList} BamlFieldTypeList instance
                 */
                BamlFieldTypeList.create = function create(properties) {
                    return new BamlFieldTypeList(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeList message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeList.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeList
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeList} message BamlFieldTypeList message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeList.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.itemType != null && Object.hasOwnProperty.call(message, "itemType"))
                        $root.baml.cffi.v1.BamlFieldType.encode(message.itemType, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeList message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeList.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeList
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeList} message BamlFieldTypeList message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeList.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeList message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeList
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeList} BamlFieldTypeList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeList.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeList();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.itemType = $root.baml.cffi.v1.BamlFieldType.decode(reader, reader.uint32());
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
                 * Decodes a BamlFieldTypeList message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeList
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeList} BamlFieldTypeList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeList.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeList message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeList
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeList.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.itemType != null && message.hasOwnProperty("itemType")) {
                        var error = $root.baml.cffi.v1.BamlFieldType.verify(message.itemType);
                        if (error)
                            return "itemType." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeList message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeList
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeList} BamlFieldTypeList
                 */
                BamlFieldTypeList.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeList)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlFieldTypeList();
                    if (object.itemType != null) {
                        if (typeof object.itemType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldTypeList.itemType: object expected");
                        message.itemType = $root.baml.cffi.v1.BamlFieldType.fromObject(object.itemType);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlFieldTypeList message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeList
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeList} message BamlFieldTypeList
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeList.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.itemType = null;
                    if (message.itemType != null && message.hasOwnProperty("itemType"))
                        object.itemType = $root.baml.cffi.v1.BamlFieldType.toObject(message.itemType, options);
                    return object;
                };

                /**
                 * Converts this BamlFieldTypeList to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeList
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeList.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeList
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeList
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeList.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeList";
                };

                return BamlFieldTypeList;
            })();

            v1.BamlFieldTypeMap = (function() {

                /**
                 * Properties of a BamlFieldTypeMap.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeMap
                 * @property {baml.cffi.v1.IBamlFieldType|null} [keyType] BamlFieldTypeMap keyType
                 * @property {baml.cffi.v1.IBamlFieldType|null} [valueType] BamlFieldTypeMap valueType
                 */

                /**
                 * Constructs a new BamlFieldTypeMap.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeMap.
                 * @implements IBamlFieldTypeMap
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeMap=} [properties] Properties to set
                 */
                function BamlFieldTypeMap(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlFieldTypeMap keyType.
                 * @member {baml.cffi.v1.IBamlFieldType|null|undefined} keyType
                 * @memberof baml.cffi.v1.BamlFieldTypeMap
                 * @instance
                 */
                BamlFieldTypeMap.prototype.keyType = null;

                /**
                 * BamlFieldTypeMap valueType.
                 * @member {baml.cffi.v1.IBamlFieldType|null|undefined} valueType
                 * @memberof baml.cffi.v1.BamlFieldTypeMap
                 * @instance
                 */
                BamlFieldTypeMap.prototype.valueType = null;

                /**
                 * Creates a new BamlFieldTypeMap instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeMap
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeMap=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeMap} BamlFieldTypeMap instance
                 */
                BamlFieldTypeMap.create = function create(properties) {
                    return new BamlFieldTypeMap(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeMap message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeMap.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeMap
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeMap} message BamlFieldTypeMap message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeMap.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.keyType != null && Object.hasOwnProperty.call(message, "keyType"))
                        $root.baml.cffi.v1.BamlFieldType.encode(message.keyType, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.valueType != null && Object.hasOwnProperty.call(message, "valueType"))
                        $root.baml.cffi.v1.BamlFieldType.encode(message.valueType, writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeMap message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeMap.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeMap
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeMap} message BamlFieldTypeMap message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeMap.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeMap message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeMap
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeMap} BamlFieldTypeMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeMap.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeMap();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.keyType = $root.baml.cffi.v1.BamlFieldType.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.valueType = $root.baml.cffi.v1.BamlFieldType.decode(reader, reader.uint32());
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
                 * Decodes a BamlFieldTypeMap message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeMap
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeMap} BamlFieldTypeMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeMap.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeMap message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeMap
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeMap.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.keyType != null && message.hasOwnProperty("keyType")) {
                        var error = $root.baml.cffi.v1.BamlFieldType.verify(message.keyType);
                        if (error)
                            return "keyType." + error;
                    }
                    if (message.valueType != null && message.hasOwnProperty("valueType")) {
                        var error = $root.baml.cffi.v1.BamlFieldType.verify(message.valueType);
                        if (error)
                            return "valueType." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeMap message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeMap
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeMap} BamlFieldTypeMap
                 */
                BamlFieldTypeMap.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeMap)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlFieldTypeMap();
                    if (object.keyType != null) {
                        if (typeof object.keyType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldTypeMap.keyType: object expected");
                        message.keyType = $root.baml.cffi.v1.BamlFieldType.fromObject(object.keyType);
                    }
                    if (object.valueType != null) {
                        if (typeof object.valueType !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldTypeMap.valueType: object expected");
                        message.valueType = $root.baml.cffi.v1.BamlFieldType.fromObject(object.valueType);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlFieldTypeMap message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeMap
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeMap} message BamlFieldTypeMap
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeMap.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults) {
                        object.keyType = null;
                        object.valueType = null;
                    }
                    if (message.keyType != null && message.hasOwnProperty("keyType"))
                        object.keyType = $root.baml.cffi.v1.BamlFieldType.toObject(message.keyType, options);
                    if (message.valueType != null && message.hasOwnProperty("valueType"))
                        object.valueType = $root.baml.cffi.v1.BamlFieldType.toObject(message.valueType, options);
                    return object;
                };

                /**
                 * Converts this BamlFieldTypeMap to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeMap
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeMap.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeMap
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeMap
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeMap.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeMap";
                };

                return BamlFieldTypeMap;
            })();

            v1.BamlFieldTypeUnionVariant = (function() {

                /**
                 * Properties of a BamlFieldTypeUnionVariant.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeUnionVariant
                 * @property {baml.cffi.v1.IBamlTypeName|null} [name] BamlFieldTypeUnionVariant name
                 */

                /**
                 * Constructs a new BamlFieldTypeUnionVariant.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeUnionVariant.
                 * @implements IBamlFieldTypeUnionVariant
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeUnionVariant=} [properties] Properties to set
                 */
                function BamlFieldTypeUnionVariant(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlFieldTypeUnionVariant name.
                 * @member {baml.cffi.v1.IBamlTypeName|null|undefined} name
                 * @memberof baml.cffi.v1.BamlFieldTypeUnionVariant
                 * @instance
                 */
                BamlFieldTypeUnionVariant.prototype.name = null;

                /**
                 * Creates a new BamlFieldTypeUnionVariant instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeUnionVariant
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeUnionVariant=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeUnionVariant} BamlFieldTypeUnionVariant instance
                 */
                BamlFieldTypeUnionVariant.create = function create(properties) {
                    return new BamlFieldTypeUnionVariant(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeUnionVariant message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeUnionVariant.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeUnionVariant
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeUnionVariant} message BamlFieldTypeUnionVariant message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeUnionVariant.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml.cffi.v1.BamlTypeName.encode(message.name, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeUnionVariant message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeUnionVariant.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeUnionVariant
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeUnionVariant} message BamlFieldTypeUnionVariant message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeUnionVariant.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeUnionVariant message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeUnionVariant
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeUnionVariant} BamlFieldTypeUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeUnionVariant.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeUnionVariant();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = $root.baml.cffi.v1.BamlTypeName.decode(reader, reader.uint32());
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
                 * Decodes a BamlFieldTypeUnionVariant message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeUnionVariant
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeUnionVariant} BamlFieldTypeUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeUnionVariant.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeUnionVariant message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeUnionVariant
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeUnionVariant.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml.cffi.v1.BamlTypeName.verify(message.name);
                        if (error)
                            return "name." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeUnionVariant message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeUnionVariant
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeUnionVariant} BamlFieldTypeUnionVariant
                 */
                BamlFieldTypeUnionVariant.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeUnionVariant)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlFieldTypeUnionVariant();
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldTypeUnionVariant.name: object expected");
                        message.name = $root.baml.cffi.v1.BamlTypeName.fromObject(object.name);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlFieldTypeUnionVariant message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeUnionVariant
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeUnionVariant} message BamlFieldTypeUnionVariant
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeUnionVariant.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.name = null;
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = $root.baml.cffi.v1.BamlTypeName.toObject(message.name, options);
                    return object;
                };

                /**
                 * Converts this BamlFieldTypeUnionVariant to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeUnionVariant
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeUnionVariant.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeUnionVariant
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeUnionVariant
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeUnionVariant.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeUnionVariant";
                };

                return BamlFieldTypeUnionVariant;
            })();

            v1.BamlFieldTypeOptional = (function() {

                /**
                 * Properties of a BamlFieldTypeOptional.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeOptional
                 * @property {baml.cffi.v1.IBamlFieldType|null} [value] BamlFieldTypeOptional value
                 */

                /**
                 * Constructs a new BamlFieldTypeOptional.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeOptional.
                 * @implements IBamlFieldTypeOptional
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeOptional=} [properties] Properties to set
                 */
                function BamlFieldTypeOptional(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlFieldTypeOptional value.
                 * @member {baml.cffi.v1.IBamlFieldType|null|undefined} value
                 * @memberof baml.cffi.v1.BamlFieldTypeOptional
                 * @instance
                 */
                BamlFieldTypeOptional.prototype.value = null;

                /**
                 * Creates a new BamlFieldTypeOptional instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeOptional
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeOptional=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeOptional} BamlFieldTypeOptional instance
                 */
                BamlFieldTypeOptional.create = function create(properties) {
                    return new BamlFieldTypeOptional(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeOptional message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeOptional.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeOptional
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeOptional} message BamlFieldTypeOptional message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeOptional.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        $root.baml.cffi.v1.BamlFieldType.encode(message.value, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeOptional message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeOptional.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeOptional
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeOptional} message BamlFieldTypeOptional message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeOptional.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeOptional message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeOptional
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeOptional} BamlFieldTypeOptional
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeOptional.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeOptional();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.value = $root.baml.cffi.v1.BamlFieldType.decode(reader, reader.uint32());
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
                 * Decodes a BamlFieldTypeOptional message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeOptional
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeOptional} BamlFieldTypeOptional
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeOptional.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeOptional message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeOptional
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeOptional.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.value != null && message.hasOwnProperty("value")) {
                        var error = $root.baml.cffi.v1.BamlFieldType.verify(message.value);
                        if (error)
                            return "value." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeOptional message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeOptional
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeOptional} BamlFieldTypeOptional
                 */
                BamlFieldTypeOptional.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeOptional)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlFieldTypeOptional();
                    if (object.value != null) {
                        if (typeof object.value !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldTypeOptional.value: object expected");
                        message.value = $root.baml.cffi.v1.BamlFieldType.fromObject(object.value);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlFieldTypeOptional message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeOptional
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeOptional} message BamlFieldTypeOptional
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeOptional.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.value = null;
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = $root.baml.cffi.v1.BamlFieldType.toObject(message.value, options);
                    return object;
                };

                /**
                 * Converts this BamlFieldTypeOptional to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeOptional
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeOptional.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeOptional
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeOptional
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeOptional.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeOptional";
                };

                return BamlFieldTypeOptional;
            })();

            v1.BamlFieldTypeChecked = (function() {

                /**
                 * Properties of a BamlFieldTypeChecked.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeChecked
                 * @property {baml.cffi.v1.IBamlFieldType|null} [value] BamlFieldTypeChecked value
                 * @property {Array.<baml.cffi.v1.IBamlCheckType>|null} [checks] BamlFieldTypeChecked checks
                 */

                /**
                 * Constructs a new BamlFieldTypeChecked.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeChecked.
                 * @implements IBamlFieldTypeChecked
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeChecked=} [properties] Properties to set
                 */
                function BamlFieldTypeChecked(properties) {
                    this.checks = [];
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlFieldTypeChecked value.
                 * @member {baml.cffi.v1.IBamlFieldType|null|undefined} value
                 * @memberof baml.cffi.v1.BamlFieldTypeChecked
                 * @instance
                 */
                BamlFieldTypeChecked.prototype.value = null;

                /**
                 * BamlFieldTypeChecked checks.
                 * @member {Array.<baml.cffi.v1.IBamlCheckType>} checks
                 * @memberof baml.cffi.v1.BamlFieldTypeChecked
                 * @instance
                 */
                BamlFieldTypeChecked.prototype.checks = $util.emptyArray;

                /**
                 * Creates a new BamlFieldTypeChecked instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeChecked
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeChecked=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeChecked} BamlFieldTypeChecked instance
                 */
                BamlFieldTypeChecked.create = function create(properties) {
                    return new BamlFieldTypeChecked(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeChecked message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeChecked.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeChecked
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeChecked} message BamlFieldTypeChecked message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeChecked.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        $root.baml.cffi.v1.BamlFieldType.encode(message.value, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.checks != null && message.checks.length)
                        for (var i = 0; i < message.checks.length; ++i)
                            $root.baml.cffi.v1.BamlCheckType.encode(message.checks[i], writer.uint32(/* id 2, wireType 2 =*/18).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeChecked message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeChecked.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeChecked
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeChecked} message BamlFieldTypeChecked message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeChecked.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeChecked message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeChecked
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeChecked} BamlFieldTypeChecked
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeChecked.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeChecked();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.value = $root.baml.cffi.v1.BamlFieldType.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                if (!(message.checks && message.checks.length))
                                    message.checks = [];
                                message.checks.push($root.baml.cffi.v1.BamlCheckType.decode(reader, reader.uint32()));
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
                 * Decodes a BamlFieldTypeChecked message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeChecked
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeChecked} BamlFieldTypeChecked
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeChecked.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeChecked message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeChecked
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeChecked.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.value != null && message.hasOwnProperty("value")) {
                        var error = $root.baml.cffi.v1.BamlFieldType.verify(message.value);
                        if (error)
                            return "value." + error;
                    }
                    if (message.checks != null && message.hasOwnProperty("checks")) {
                        if (!Array.isArray(message.checks))
                            return "checks: array expected";
                        for (var i = 0; i < message.checks.length; ++i) {
                            var error = $root.baml.cffi.v1.BamlCheckType.verify(message.checks[i]);
                            if (error)
                                return "checks." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeChecked message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeChecked
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeChecked} BamlFieldTypeChecked
                 */
                BamlFieldTypeChecked.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeChecked)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlFieldTypeChecked();
                    if (object.value != null) {
                        if (typeof object.value !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldTypeChecked.value: object expected");
                        message.value = $root.baml.cffi.v1.BamlFieldType.fromObject(object.value);
                    }
                    if (object.checks) {
                        if (!Array.isArray(object.checks))
                            throw TypeError(".baml.cffi.v1.BamlFieldTypeChecked.checks: array expected");
                        message.checks = [];
                        for (var i = 0; i < object.checks.length; ++i) {
                            if (typeof object.checks[i] !== "object")
                                throw TypeError(".baml.cffi.v1.BamlFieldTypeChecked.checks: object expected");
                            message.checks[i] = $root.baml.cffi.v1.BamlCheckType.fromObject(object.checks[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlFieldTypeChecked message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeChecked
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeChecked} message BamlFieldTypeChecked
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeChecked.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.arrays || options.defaults)
                        object.checks = [];
                    if (options.defaults)
                        object.value = null;
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = $root.baml.cffi.v1.BamlFieldType.toObject(message.value, options);
                    if (message.checks && message.checks.length) {
                        object.checks = [];
                        for (var j = 0; j < message.checks.length; ++j)
                            object.checks[j] = $root.baml.cffi.v1.BamlCheckType.toObject(message.checks[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this BamlFieldTypeChecked to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeChecked
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeChecked.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeChecked
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeChecked
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeChecked.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeChecked";
                };

                return BamlFieldTypeChecked;
            })();

            v1.BamlFieldTypeStreamState = (function() {

                /**
                 * Properties of a BamlFieldTypeStreamState.
                 * @memberof baml.cffi.v1
                 * @interface IBamlFieldTypeStreamState
                 * @property {baml.cffi.v1.IBamlFieldType|null} [value] BamlFieldTypeStreamState value
                 */

                /**
                 * Constructs a new BamlFieldTypeStreamState.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlFieldTypeStreamState.
                 * @implements IBamlFieldTypeStreamState
                 * @constructor
                 * @param {baml.cffi.v1.IBamlFieldTypeStreamState=} [properties] Properties to set
                 */
                function BamlFieldTypeStreamState(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlFieldTypeStreamState value.
                 * @member {baml.cffi.v1.IBamlFieldType|null|undefined} value
                 * @memberof baml.cffi.v1.BamlFieldTypeStreamState
                 * @instance
                 */
                BamlFieldTypeStreamState.prototype.value = null;

                /**
                 * Creates a new BamlFieldTypeStreamState instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlFieldTypeStreamState
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeStreamState=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlFieldTypeStreamState} BamlFieldTypeStreamState instance
                 */
                BamlFieldTypeStreamState.create = function create(properties) {
                    return new BamlFieldTypeStreamState(properties);
                };

                /**
                 * Encodes the specified BamlFieldTypeStreamState message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeStreamState.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlFieldTypeStreamState
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeStreamState} message BamlFieldTypeStreamState message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeStreamState.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        $root.baml.cffi.v1.BamlFieldType.encode(message.value, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlFieldTypeStreamState message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeStreamState.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeStreamState
                 * @static
                 * @param {baml.cffi.v1.IBamlFieldTypeStreamState} message BamlFieldTypeStreamState message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlFieldTypeStreamState.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlFieldTypeStreamState message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlFieldTypeStreamState
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlFieldTypeStreamState} BamlFieldTypeStreamState
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeStreamState.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlFieldTypeStreamState();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.value = $root.baml.cffi.v1.BamlFieldType.decode(reader, reader.uint32());
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
                 * Decodes a BamlFieldTypeStreamState message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlFieldTypeStreamState
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlFieldTypeStreamState} BamlFieldTypeStreamState
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlFieldTypeStreamState.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlFieldTypeStreamState message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlFieldTypeStreamState
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlFieldTypeStreamState.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.value != null && message.hasOwnProperty("value")) {
                        var error = $root.baml.cffi.v1.BamlFieldType.verify(message.value);
                        if (error)
                            return "value." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlFieldTypeStreamState message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlFieldTypeStreamState
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlFieldTypeStreamState} BamlFieldTypeStreamState
                 */
                BamlFieldTypeStreamState.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlFieldTypeStreamState)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlFieldTypeStreamState();
                    if (object.value != null) {
                        if (typeof object.value !== "object")
                            throw TypeError(".baml.cffi.v1.BamlFieldTypeStreamState.value: object expected");
                        message.value = $root.baml.cffi.v1.BamlFieldType.fromObject(object.value);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlFieldTypeStreamState message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlFieldTypeStreamState
                 * @static
                 * @param {baml.cffi.v1.BamlFieldTypeStreamState} message BamlFieldTypeStreamState
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlFieldTypeStreamState.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults)
                        object.value = null;
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = $root.baml.cffi.v1.BamlFieldType.toObject(message.value, options);
                    return object;
                };

                /**
                 * Converts this BamlFieldTypeStreamState to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlFieldTypeStreamState
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlFieldTypeStreamState.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlFieldTypeStreamState
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlFieldTypeStreamState
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlFieldTypeStreamState.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlFieldTypeStreamState";
                };

                return BamlFieldTypeStreamState;
            })();

            v1.BamlCheckType = (function() {

                /**
                 * Properties of a BamlCheckType.
                 * @memberof baml.cffi.v1
                 * @interface IBamlCheckType
                 * @property {string|null} [name] BamlCheckType name
                 */

                /**
                 * Constructs a new BamlCheckType.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlCheckType.
                 * @implements IBamlCheckType
                 * @constructor
                 * @param {baml.cffi.v1.IBamlCheckType=} [properties] Properties to set
                 */
                function BamlCheckType(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlCheckType name.
                 * @member {string} name
                 * @memberof baml.cffi.v1.BamlCheckType
                 * @instance
                 */
                BamlCheckType.prototype.name = "";

                /**
                 * Creates a new BamlCheckType instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlCheckType
                 * @static
                 * @param {baml.cffi.v1.IBamlCheckType=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlCheckType} BamlCheckType instance
                 */
                BamlCheckType.create = function create(properties) {
                    return new BamlCheckType(properties);
                };

                /**
                 * Encodes the specified BamlCheckType message. Does not implicitly {@link baml.cffi.v1.BamlCheckType.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlCheckType
                 * @static
                 * @param {baml.cffi.v1.IBamlCheckType} message BamlCheckType message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlCheckType.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.name);
                    return writer;
                };

                /**
                 * Encodes the specified BamlCheckType message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlCheckType.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlCheckType
                 * @static
                 * @param {baml.cffi.v1.IBamlCheckType} message BamlCheckType message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlCheckType.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlCheckType message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlCheckType
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlCheckType} BamlCheckType
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlCheckType.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlCheckType();
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
                 * Decodes a BamlCheckType message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlCheckType
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlCheckType} BamlCheckType
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlCheckType.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlCheckType message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlCheckType
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlCheckType.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name"))
                        if (!$util.isString(message.name))
                            return "name: string expected";
                    return null;
                };

                /**
                 * Creates a BamlCheckType message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlCheckType
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlCheckType} BamlCheckType
                 */
                BamlCheckType.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlCheckType)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlCheckType();
                    if (object.name != null)
                        message.name = String(object.name);
                    return message;
                };

                /**
                 * Creates a plain object from a BamlCheckType message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlCheckType
                 * @static
                 * @param {baml.cffi.v1.BamlCheckType} message BamlCheckType
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlCheckType.toObject = function toObject(message, options) {
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
                 * Converts this BamlCheckType to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlCheckType
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlCheckType.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlCheckType
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlCheckType
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlCheckType.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlCheckType";
                };

                return BamlCheckType;
            })();

            v1.BamlCheckValue = (function() {

                /**
                 * Properties of a BamlCheckValue.
                 * @memberof baml.cffi.v1
                 * @interface IBamlCheckValue
                 * @property {string|null} [name] BamlCheckValue name
                 * @property {string|null} [expression] BamlCheckValue expression
                 * @property {string|null} [status] BamlCheckValue status
                 * @property {baml.cffi.v1.IBamlOutboundValue|null} [value] BamlCheckValue value
                 */

                /**
                 * Constructs a new BamlCheckValue.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlCheckValue.
                 * @implements IBamlCheckValue
                 * @constructor
                 * @param {baml.cffi.v1.IBamlCheckValue=} [properties] Properties to set
                 */
                function BamlCheckValue(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlCheckValue name.
                 * @member {string} name
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @instance
                 */
                BamlCheckValue.prototype.name = "";

                /**
                 * BamlCheckValue expression.
                 * @member {string} expression
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @instance
                 */
                BamlCheckValue.prototype.expression = "";

                /**
                 * BamlCheckValue status.
                 * @member {string} status
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @instance
                 */
                BamlCheckValue.prototype.status = "";

                /**
                 * BamlCheckValue value.
                 * @member {baml.cffi.v1.IBamlOutboundValue|null|undefined} value
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @instance
                 */
                BamlCheckValue.prototype.value = null;

                /**
                 * Creates a new BamlCheckValue instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @static
                 * @param {baml.cffi.v1.IBamlCheckValue=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlCheckValue} BamlCheckValue instance
                 */
                BamlCheckValue.create = function create(properties) {
                    return new BamlCheckValue(properties);
                };

                /**
                 * Encodes the specified BamlCheckValue message. Does not implicitly {@link baml.cffi.v1.BamlCheckValue.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @static
                 * @param {baml.cffi.v1.IBamlCheckValue} message BamlCheckValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlCheckValue.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.name);
                    if (message.expression != null && Object.hasOwnProperty.call(message, "expression"))
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.expression);
                    if (message.status != null && Object.hasOwnProperty.call(message, "status"))
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.status);
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        $root.baml.cffi.v1.BamlOutboundValue.encode(message.value, writer.uint32(/* id 4, wireType 2 =*/34).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlCheckValue message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlCheckValue.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @static
                 * @param {baml.cffi.v1.IBamlCheckValue} message BamlCheckValue message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlCheckValue.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlCheckValue message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlCheckValue} BamlCheckValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlCheckValue.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlCheckValue();
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
                                message.expression = reader.string();
                                break;
                            }
                        case 3: {
                                message.status = reader.string();
                                break;
                            }
                        case 4: {
                                message.value = $root.baml.cffi.v1.BamlOutboundValue.decode(reader, reader.uint32());
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
                 * Decodes a BamlCheckValue message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlCheckValue} BamlCheckValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlCheckValue.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlCheckValue message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlCheckValue.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name"))
                        if (!$util.isString(message.name))
                            return "name: string expected";
                    if (message.expression != null && message.hasOwnProperty("expression"))
                        if (!$util.isString(message.expression))
                            return "expression: string expected";
                    if (message.status != null && message.hasOwnProperty("status"))
                        if (!$util.isString(message.status))
                            return "status: string expected";
                    if (message.value != null && message.hasOwnProperty("value")) {
                        var error = $root.baml.cffi.v1.BamlOutboundValue.verify(message.value);
                        if (error)
                            return "value." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlCheckValue message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlCheckValue} BamlCheckValue
                 */
                BamlCheckValue.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlCheckValue)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlCheckValue();
                    if (object.name != null)
                        message.name = String(object.name);
                    if (object.expression != null)
                        message.expression = String(object.expression);
                    if (object.status != null)
                        message.status = String(object.status);
                    if (object.value != null) {
                        if (typeof object.value !== "object")
                            throw TypeError(".baml.cffi.v1.BamlCheckValue.value: object expected");
                        message.value = $root.baml.cffi.v1.BamlOutboundValue.fromObject(object.value);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlCheckValue message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @static
                 * @param {baml.cffi.v1.BamlCheckValue} message BamlCheckValue
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlCheckValue.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults) {
                        object.name = "";
                        object.expression = "";
                        object.status = "";
                        object.value = null;
                    }
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = message.name;
                    if (message.expression != null && message.hasOwnProperty("expression"))
                        object.expression = message.expression;
                    if (message.status != null && message.hasOwnProperty("status"))
                        object.status = message.status;
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = $root.baml.cffi.v1.BamlOutboundValue.toObject(message.value, options);
                    return object;
                };

                /**
                 * Converts this BamlCheckValue to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlCheckValue.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlCheckValue
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlCheckValue
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlCheckValue.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlCheckValue";
                };

                return BamlCheckValue;
            })();

            /**
             * BamlStreamState enum.
             * @name baml.cffi.v1.BamlStreamState
             * @enum {number}
             * @property {number} PENDING=0 PENDING value
             * @property {number} STARTED=1 STARTED value
             * @property {number} DONE=2 DONE value
             */
            v1.BamlStreamState = (function() {
                var valuesById = {}, values = Object.create(valuesById);
                values[valuesById[0] = "PENDING"] = 0;
                values[valuesById[1] = "STARTED"] = 1;
                values[valuesById[2] = "DONE"] = 2;
                return values;
            })();

            v1.BamlValueStreamingState = (function() {

                /**
                 * Properties of a BamlValueStreamingState.
                 * @memberof baml.cffi.v1
                 * @interface IBamlValueStreamingState
                 * @property {baml.cffi.v1.IBamlOutboundValue|null} [value] BamlValueStreamingState value
                 * @property {baml.cffi.v1.BamlStreamState|null} [state] BamlValueStreamingState state
                 * @property {baml.cffi.v1.IBamlTypeName|null} [name] BamlValueStreamingState name
                 */

                /**
                 * Constructs a new BamlValueStreamingState.
                 * @memberof baml.cffi.v1
                 * @classdesc Represents a BamlValueStreamingState.
                 * @implements IBamlValueStreamingState
                 * @constructor
                 * @param {baml.cffi.v1.IBamlValueStreamingState=} [properties] Properties to set
                 */
                function BamlValueStreamingState(properties) {
                    if (properties)
                        for (var keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * BamlValueStreamingState value.
                 * @member {baml.cffi.v1.IBamlOutboundValue|null|undefined} value
                 * @memberof baml.cffi.v1.BamlValueStreamingState
                 * @instance
                 */
                BamlValueStreamingState.prototype.value = null;

                /**
                 * BamlValueStreamingState state.
                 * @member {baml.cffi.v1.BamlStreamState} state
                 * @memberof baml.cffi.v1.BamlValueStreamingState
                 * @instance
                 */
                BamlValueStreamingState.prototype.state = 0;

                /**
                 * BamlValueStreamingState name.
                 * @member {baml.cffi.v1.IBamlTypeName|null|undefined} name
                 * @memberof baml.cffi.v1.BamlValueStreamingState
                 * @instance
                 */
                BamlValueStreamingState.prototype.name = null;

                /**
                 * Creates a new BamlValueStreamingState instance using the specified properties.
                 * @function create
                 * @memberof baml.cffi.v1.BamlValueStreamingState
                 * @static
                 * @param {baml.cffi.v1.IBamlValueStreamingState=} [properties] Properties to set
                 * @returns {baml.cffi.v1.BamlValueStreamingState} BamlValueStreamingState instance
                 */
                BamlValueStreamingState.create = function create(properties) {
                    return new BamlValueStreamingState(properties);
                };

                /**
                 * Encodes the specified BamlValueStreamingState message. Does not implicitly {@link baml.cffi.v1.BamlValueStreamingState.verify|verify} messages.
                 * @function encode
                 * @memberof baml.cffi.v1.BamlValueStreamingState
                 * @static
                 * @param {baml.cffi.v1.IBamlValueStreamingState} message BamlValueStreamingState message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueStreamingState.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.value != null && Object.hasOwnProperty.call(message, "value"))
                        $root.baml.cffi.v1.BamlOutboundValue.encode(message.value, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.state != null && Object.hasOwnProperty.call(message, "state"))
                        writer.uint32(/* id 2, wireType 0 =*/16).int32(message.state);
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        $root.baml.cffi.v1.BamlTypeName.encode(message.name, writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified BamlValueStreamingState message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueStreamingState.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof baml.cffi.v1.BamlValueStreamingState
                 * @static
                 * @param {baml.cffi.v1.IBamlValueStreamingState} message BamlValueStreamingState message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                BamlValueStreamingState.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a BamlValueStreamingState message from the specified reader or buffer.
                 * @function decode
                 * @memberof baml.cffi.v1.BamlValueStreamingState
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {baml.cffi.v1.BamlValueStreamingState} BamlValueStreamingState
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueStreamingState.decode = function decode(reader, length, error) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    var end = length === undefined ? reader.len : reader.pos + length, message = new $root.baml.cffi.v1.BamlValueStreamingState();
                    while (reader.pos < end) {
                        var tag = reader.uint32();
                        if (tag === error)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.value = $root.baml.cffi.v1.BamlOutboundValue.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.state = reader.int32();
                                break;
                            }
                        case 3: {
                                message.name = $root.baml.cffi.v1.BamlTypeName.decode(reader, reader.uint32());
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
                 * Decodes a BamlValueStreamingState message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof baml.cffi.v1.BamlValueStreamingState
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {baml.cffi.v1.BamlValueStreamingState} BamlValueStreamingState
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                BamlValueStreamingState.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a BamlValueStreamingState message.
                 * @function verify
                 * @memberof baml.cffi.v1.BamlValueStreamingState
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                BamlValueStreamingState.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.value != null && message.hasOwnProperty("value")) {
                        var error = $root.baml.cffi.v1.BamlOutboundValue.verify(message.value);
                        if (error)
                            return "value." + error;
                    }
                    if (message.state != null && message.hasOwnProperty("state"))
                        switch (message.state) {
                        default:
                            return "state: enum value expected";
                        case 0:
                        case 1:
                        case 2:
                            break;
                        }
                    if (message.name != null && message.hasOwnProperty("name")) {
                        var error = $root.baml.cffi.v1.BamlTypeName.verify(message.name);
                        if (error)
                            return "name." + error;
                    }
                    return null;
                };

                /**
                 * Creates a BamlValueStreamingState message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof baml.cffi.v1.BamlValueStreamingState
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {baml.cffi.v1.BamlValueStreamingState} BamlValueStreamingState
                 */
                BamlValueStreamingState.fromObject = function fromObject(object) {
                    if (object instanceof $root.baml.cffi.v1.BamlValueStreamingState)
                        return object;
                    var message = new $root.baml.cffi.v1.BamlValueStreamingState();
                    if (object.value != null) {
                        if (typeof object.value !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValueStreamingState.value: object expected");
                        message.value = $root.baml.cffi.v1.BamlOutboundValue.fromObject(object.value);
                    }
                    switch (object.state) {
                    default:
                        if (typeof object.state === "number") {
                            message.state = object.state;
                            break;
                        }
                        break;
                    case "PENDING":
                    case 0:
                        message.state = 0;
                        break;
                    case "STARTED":
                    case 1:
                        message.state = 1;
                        break;
                    case "DONE":
                    case 2:
                        message.state = 2;
                        break;
                    }
                    if (object.name != null) {
                        if (typeof object.name !== "object")
                            throw TypeError(".baml.cffi.v1.BamlValueStreamingState.name: object expected");
                        message.name = $root.baml.cffi.v1.BamlTypeName.fromObject(object.name);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a BamlValueStreamingState message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof baml.cffi.v1.BamlValueStreamingState
                 * @static
                 * @param {baml.cffi.v1.BamlValueStreamingState} message BamlValueStreamingState
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                BamlValueStreamingState.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    var object = {};
                    if (options.defaults) {
                        object.value = null;
                        object.state = options.enums === String ? "PENDING" : 0;
                        object.name = null;
                    }
                    if (message.value != null && message.hasOwnProperty("value"))
                        object.value = $root.baml.cffi.v1.BamlOutboundValue.toObject(message.value, options);
                    if (message.state != null && message.hasOwnProperty("state"))
                        object.state = options.enums === String ? $root.baml.cffi.v1.BamlStreamState[message.state] === undefined ? message.state : $root.baml.cffi.v1.BamlStreamState[message.state] : message.state;
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = $root.baml.cffi.v1.BamlTypeName.toObject(message.name, options);
                    return object;
                };

                /**
                 * Converts this BamlValueStreamingState to JSON.
                 * @function toJSON
                 * @memberof baml.cffi.v1.BamlValueStreamingState
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                BamlValueStreamingState.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for BamlValueStreamingState
                 * @function getTypeUrl
                 * @memberof baml.cffi.v1.BamlValueStreamingState
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                BamlValueStreamingState.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/baml.cffi.v1.BamlValueStreamingState";
                };

                return BamlValueStreamingState;
            })();

            return v1;
        })();

        return cffi;
    })();

    return baml;
})();

module.exports = $root;
