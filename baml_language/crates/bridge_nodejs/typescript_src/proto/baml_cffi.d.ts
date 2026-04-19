/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
import * as $protobuf from "protobufjs";
import Long = require("long");
/** Namespace baml. */
export namespace baml {

    /** Namespace cffi. */
    namespace cffi {

        /** Namespace v1. */
        namespace v1 {

            /** BamlHandleType enum. */
            enum BamlHandleType {
                HANDLE_UNSPECIFIED = 0,
                HANDLE_UNKNOWN = 1,
                RESOURCE_FILE = 2,
                RESOURCE_SOCKET = 3,
                RESOURCE_HTTP_RESPONSE = 4,
                FUNCTION_REF = 5,
                ADT_MEDIA_IMAGE = 6,
                ADT_MEDIA_AUDIO = 7,
                ADT_MEDIA_VIDEO = 8,
                ADT_MEDIA_PDF = 9,
                ADT_MEDIA_GENERIC = 10,
                ADT_PROMPT_AST = 11,
                ADT_COLLECTOR = 12,
                ADT_TYPE = 13
            }

            /** Properties of a BamlHandle. */
            interface IBamlHandle {

                /** BamlHandle key */
                key?: (number|Long|null);

                /** BamlHandle handleType */
                handleType?: (baml.cffi.v1.BamlHandleType|null);
            }

            /** Represents a BamlHandle. */
            class BamlHandle implements IBamlHandle {

                /**
                 * Constructs a new BamlHandle.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlHandle);

                /** BamlHandle key. */
                public key: (number|Long);

                /** BamlHandle handleType. */
                public handleType: baml.cffi.v1.BamlHandleType;

                /**
                 * Creates a new BamlHandle instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlHandle instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlHandle): baml.cffi.v1.BamlHandle;

                /**
                 * Encodes the specified BamlHandle message. Does not implicitly {@link baml.cffi.v1.BamlHandle.verify|verify} messages.
                 * @param message BamlHandle message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlHandle, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlHandle message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlHandle.verify|verify} messages.
                 * @param message BamlHandle message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlHandle, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlHandle message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlHandle;

                /**
                 * Decodes a BamlHandle message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlHandle;

                /**
                 * Verifies a BamlHandle message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlHandle message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlHandle
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlHandle;

                /**
                 * Creates a plain object from a BamlHandle message. Also converts values to other types if specified.
                 * @param message BamlHandle
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlHandle, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlHandle to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlHandle
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of an InboundValue. */
            interface IInboundValue {

                /** InboundValue stringValue */
                stringValue?: (string|null);

                /** InboundValue intValue */
                intValue?: (number|Long|null);

                /** InboundValue floatValue */
                floatValue?: (number|null);

                /** InboundValue boolValue */
                boolValue?: (boolean|null);

                /** InboundValue listValue */
                listValue?: (baml.cffi.v1.IInboundListValue|null);

                /** InboundValue mapValue */
                mapValue?: (baml.cffi.v1.IInboundMapValue|null);

                /** InboundValue classValue */
                classValue?: (baml.cffi.v1.IInboundClassValue|null);

                /** InboundValue enumValue */
                enumValue?: (baml.cffi.v1.IInboundEnumValue|null);

                /** InboundValue handle */
                handle?: (baml.cffi.v1.IBamlHandle|null);

                /** InboundValue uint8arrayValue */
                uint8arrayValue?: (Uint8Array|null);
            }

            /** Represents an InboundValue. */
            class InboundValue implements IInboundValue {

                /**
                 * Constructs a new InboundValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IInboundValue);

                /** InboundValue stringValue. */
                public stringValue?: (string|null);

                /** InboundValue intValue. */
                public intValue?: (number|Long|null);

                /** InboundValue floatValue. */
                public floatValue?: (number|null);

                /** InboundValue boolValue. */
                public boolValue?: (boolean|null);

                /** InboundValue listValue. */
                public listValue?: (baml.cffi.v1.IInboundListValue|null);

                /** InboundValue mapValue. */
                public mapValue?: (baml.cffi.v1.IInboundMapValue|null);

                /** InboundValue classValue. */
                public classValue?: (baml.cffi.v1.IInboundClassValue|null);

                /** InboundValue enumValue. */
                public enumValue?: (baml.cffi.v1.IInboundEnumValue|null);

                /** InboundValue handle. */
                public handle?: (baml.cffi.v1.IBamlHandle|null);

                /** InboundValue uint8arrayValue. */
                public uint8arrayValue?: (Uint8Array|null);

                /** InboundValue value. */
                public value?: ("stringValue"|"intValue"|"floatValue"|"boolValue"|"listValue"|"mapValue"|"classValue"|"enumValue"|"handle"|"uint8arrayValue");

                /**
                 * Creates a new InboundValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundValue instance
                 */
                public static create(properties?: baml.cffi.v1.IInboundValue): baml.cffi.v1.InboundValue;

                /**
                 * Encodes the specified InboundValue message. Does not implicitly {@link baml.cffi.v1.InboundValue.verify|verify} messages.
                 * @param message InboundValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IInboundValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundValue message, length delimited. Does not implicitly {@link baml.cffi.v1.InboundValue.verify|verify} messages.
                 * @param message InboundValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IInboundValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.InboundValue;

                /**
                 * Decodes an InboundValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.InboundValue;

                /**
                 * Verifies an InboundValue message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates an InboundValue message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns InboundValue
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.InboundValue;

                /**
                 * Creates a plain object from an InboundValue message. Also converts values to other types if specified.
                 * @param message InboundValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.InboundValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this InboundValue to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for InboundValue
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of an InboundListValue. */
            interface IInboundListValue {

                /** InboundListValue values */
                values?: (baml.cffi.v1.IInboundValue[]|null);
            }

            /** Represents an InboundListValue. */
            class InboundListValue implements IInboundListValue {

                /**
                 * Constructs a new InboundListValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IInboundListValue);

                /** InboundListValue values. */
                public values: baml.cffi.v1.IInboundValue[];

                /**
                 * Creates a new InboundListValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundListValue instance
                 */
                public static create(properties?: baml.cffi.v1.IInboundListValue): baml.cffi.v1.InboundListValue;

                /**
                 * Encodes the specified InboundListValue message. Does not implicitly {@link baml.cffi.v1.InboundListValue.verify|verify} messages.
                 * @param message InboundListValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IInboundListValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundListValue message, length delimited. Does not implicitly {@link baml.cffi.v1.InboundListValue.verify|verify} messages.
                 * @param message InboundListValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IInboundListValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundListValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundListValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.InboundListValue;

                /**
                 * Decodes an InboundListValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundListValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.InboundListValue;

                /**
                 * Verifies an InboundListValue message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates an InboundListValue message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns InboundListValue
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.InboundListValue;

                /**
                 * Creates a plain object from an InboundListValue message. Also converts values to other types if specified.
                 * @param message InboundListValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.InboundListValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this InboundListValue to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for InboundListValue
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of an InboundMapValue. */
            interface IInboundMapValue {

                /** InboundMapValue entries */
                entries?: (baml.cffi.v1.IInboundMapEntry[]|null);
            }

            /** Represents an InboundMapValue. */
            class InboundMapValue implements IInboundMapValue {

                /**
                 * Constructs a new InboundMapValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IInboundMapValue);

                /** InboundMapValue entries. */
                public entries: baml.cffi.v1.IInboundMapEntry[];

                /**
                 * Creates a new InboundMapValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundMapValue instance
                 */
                public static create(properties?: baml.cffi.v1.IInboundMapValue): baml.cffi.v1.InboundMapValue;

                /**
                 * Encodes the specified InboundMapValue message. Does not implicitly {@link baml.cffi.v1.InboundMapValue.verify|verify} messages.
                 * @param message InboundMapValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IInboundMapValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundMapValue message, length delimited. Does not implicitly {@link baml.cffi.v1.InboundMapValue.verify|verify} messages.
                 * @param message InboundMapValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IInboundMapValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundMapValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundMapValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.InboundMapValue;

                /**
                 * Decodes an InboundMapValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundMapValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.InboundMapValue;

                /**
                 * Verifies an InboundMapValue message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates an InboundMapValue message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns InboundMapValue
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.InboundMapValue;

                /**
                 * Creates a plain object from an InboundMapValue message. Also converts values to other types if specified.
                 * @param message InboundMapValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.InboundMapValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this InboundMapValue to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for InboundMapValue
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of an InboundMapEntry. */
            interface IInboundMapEntry {

                /** InboundMapEntry stringKey */
                stringKey?: (string|null);

                /** InboundMapEntry intKey */
                intKey?: (number|Long|null);

                /** InboundMapEntry boolKey */
                boolKey?: (boolean|null);

                /** InboundMapEntry enumKey */
                enumKey?: (baml.cffi.v1.IInboundEnumValue|null);

                /** InboundMapEntry value */
                value?: (baml.cffi.v1.IInboundValue|null);
            }

            /** Represents an InboundMapEntry. */
            class InboundMapEntry implements IInboundMapEntry {

                /**
                 * Constructs a new InboundMapEntry.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IInboundMapEntry);

                /** InboundMapEntry stringKey. */
                public stringKey?: (string|null);

                /** InboundMapEntry intKey. */
                public intKey?: (number|Long|null);

                /** InboundMapEntry boolKey. */
                public boolKey?: (boolean|null);

                /** InboundMapEntry enumKey. */
                public enumKey?: (baml.cffi.v1.IInboundEnumValue|null);

                /** InboundMapEntry value. */
                public value?: (baml.cffi.v1.IInboundValue|null);

                /** InboundMapEntry key. */
                public key?: ("stringKey"|"intKey"|"boolKey"|"enumKey");

                /**
                 * Creates a new InboundMapEntry instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundMapEntry instance
                 */
                public static create(properties?: baml.cffi.v1.IInboundMapEntry): baml.cffi.v1.InboundMapEntry;

                /**
                 * Encodes the specified InboundMapEntry message. Does not implicitly {@link baml.cffi.v1.InboundMapEntry.verify|verify} messages.
                 * @param message InboundMapEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IInboundMapEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundMapEntry message, length delimited. Does not implicitly {@link baml.cffi.v1.InboundMapEntry.verify|verify} messages.
                 * @param message InboundMapEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IInboundMapEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundMapEntry message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.InboundMapEntry;

                /**
                 * Decodes an InboundMapEntry message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.InboundMapEntry;

                /**
                 * Verifies an InboundMapEntry message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates an InboundMapEntry message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns InboundMapEntry
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.InboundMapEntry;

                /**
                 * Creates a plain object from an InboundMapEntry message. Also converts values to other types if specified.
                 * @param message InboundMapEntry
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.InboundMapEntry, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this InboundMapEntry to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for InboundMapEntry
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of an InboundClassValue. */
            interface IInboundClassValue {

                /** InboundClassValue name */
                name?: (string|null);

                /** InboundClassValue fields */
                fields?: (baml.cffi.v1.IInboundMapEntry[]|null);
            }

            /** Represents an InboundClassValue. */
            class InboundClassValue implements IInboundClassValue {

                /**
                 * Constructs a new InboundClassValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IInboundClassValue);

                /** InboundClassValue name. */
                public name: string;

                /** InboundClassValue fields. */
                public fields: baml.cffi.v1.IInboundMapEntry[];

                /**
                 * Creates a new InboundClassValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundClassValue instance
                 */
                public static create(properties?: baml.cffi.v1.IInboundClassValue): baml.cffi.v1.InboundClassValue;

                /**
                 * Encodes the specified InboundClassValue message. Does not implicitly {@link baml.cffi.v1.InboundClassValue.verify|verify} messages.
                 * @param message InboundClassValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IInboundClassValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundClassValue message, length delimited. Does not implicitly {@link baml.cffi.v1.InboundClassValue.verify|verify} messages.
                 * @param message InboundClassValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IInboundClassValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundClassValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundClassValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.InboundClassValue;

                /**
                 * Decodes an InboundClassValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundClassValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.InboundClassValue;

                /**
                 * Verifies an InboundClassValue message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates an InboundClassValue message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns InboundClassValue
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.InboundClassValue;

                /**
                 * Creates a plain object from an InboundClassValue message. Also converts values to other types if specified.
                 * @param message InboundClassValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.InboundClassValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this InboundClassValue to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for InboundClassValue
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of an InboundEnumValue. */
            interface IInboundEnumValue {

                /** InboundEnumValue name */
                name?: (string|null);

                /** InboundEnumValue value */
                value?: (string|null);
            }

            /** Represents an InboundEnumValue. */
            class InboundEnumValue implements IInboundEnumValue {

                /**
                 * Constructs a new InboundEnumValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IInboundEnumValue);

                /** InboundEnumValue name. */
                public name: string;

                /** InboundEnumValue value. */
                public value: string;

                /**
                 * Creates a new InboundEnumValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundEnumValue instance
                 */
                public static create(properties?: baml.cffi.v1.IInboundEnumValue): baml.cffi.v1.InboundEnumValue;

                /**
                 * Encodes the specified InboundEnumValue message. Does not implicitly {@link baml.cffi.v1.InboundEnumValue.verify|verify} messages.
                 * @param message InboundEnumValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IInboundEnumValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundEnumValue message, length delimited. Does not implicitly {@link baml.cffi.v1.InboundEnumValue.verify|verify} messages.
                 * @param message InboundEnumValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IInboundEnumValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundEnumValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundEnumValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.InboundEnumValue;

                /**
                 * Decodes an InboundEnumValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundEnumValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.InboundEnumValue;

                /**
                 * Verifies an InboundEnumValue message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates an InboundEnumValue message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns InboundEnumValue
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.InboundEnumValue;

                /**
                 * Creates a plain object from an InboundEnumValue message. Also converts values to other types if specified.
                 * @param message InboundEnumValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.InboundEnumValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this InboundEnumValue to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for InboundEnumValue
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a CallFunctionArgs. */
            interface ICallFunctionArgs {

                /** CallFunctionArgs kwargs */
                kwargs?: (baml.cffi.v1.IInboundMapEntry[]|null);
            }

            /** Represents a CallFunctionArgs. */
            class CallFunctionArgs implements ICallFunctionArgs {

                /**
                 * Constructs a new CallFunctionArgs.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.ICallFunctionArgs);

                /** CallFunctionArgs kwargs. */
                public kwargs: baml.cffi.v1.IInboundMapEntry[];

                /**
                 * Creates a new CallFunctionArgs instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns CallFunctionArgs instance
                 */
                public static create(properties?: baml.cffi.v1.ICallFunctionArgs): baml.cffi.v1.CallFunctionArgs;

                /**
                 * Encodes the specified CallFunctionArgs message. Does not implicitly {@link baml.cffi.v1.CallFunctionArgs.verify|verify} messages.
                 * @param message CallFunctionArgs message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.ICallFunctionArgs, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified CallFunctionArgs message, length delimited. Does not implicitly {@link baml.cffi.v1.CallFunctionArgs.verify|verify} messages.
                 * @param message CallFunctionArgs message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.ICallFunctionArgs, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CallFunctionArgs message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns CallFunctionArgs
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.CallFunctionArgs;

                /**
                 * Decodes a CallFunctionArgs message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns CallFunctionArgs
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.CallFunctionArgs;

                /**
                 * Verifies a CallFunctionArgs message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a CallFunctionArgs message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns CallFunctionArgs
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.CallFunctionArgs;

                /**
                 * Creates a plain object from a CallFunctionArgs message. Also converts values to other types if specified.
                 * @param message CallFunctionArgs
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.CallFunctionArgs, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this CallFunctionArgs to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for CallFunctionArgs
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a CallAck. */
            interface ICallAck {

                /** CallAck error */
                error?: (string|null);
            }

            /** Represents a CallAck. */
            class CallAck implements ICallAck {

                /**
                 * Constructs a new CallAck.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.ICallAck);

                /** CallAck error. */
                public error?: (string|null);

                /** CallAck response. */
                public response?: "error";

                /**
                 * Creates a new CallAck instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns CallAck instance
                 */
                public static create(properties?: baml.cffi.v1.ICallAck): baml.cffi.v1.CallAck;

                /**
                 * Encodes the specified CallAck message. Does not implicitly {@link baml.cffi.v1.CallAck.verify|verify} messages.
                 * @param message CallAck message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.ICallAck, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified CallAck message, length delimited. Does not implicitly {@link baml.cffi.v1.CallAck.verify|verify} messages.
                 * @param message CallAck message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.ICallAck, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CallAck message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns CallAck
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.CallAck;

                /**
                 * Decodes a CallAck message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns CallAck
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.CallAck;

                /**
                 * Verifies a CallAck message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a CallAck message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns CallAck
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.CallAck;

                /**
                 * Creates a plain object from a CallAck message. Also converts values to other types if specified.
                 * @param message CallAck
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.CallAck, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this CallAck to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for CallAck
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlOutboundValue. */
            interface IBamlOutboundValue {

                /** BamlOutboundValue nullValue */
                nullValue?: (baml.cffi.v1.IBamlValueNull|null);

                /** BamlOutboundValue stringValue */
                stringValue?: (string|null);

                /** BamlOutboundValue intValue */
                intValue?: (number|Long|null);

                /** BamlOutboundValue floatValue */
                floatValue?: (number|null);

                /** BamlOutboundValue boolValue */
                boolValue?: (boolean|null);

                /** BamlOutboundValue classValue */
                classValue?: (baml.cffi.v1.IBamlValueClass|null);

                /** BamlOutboundValue enumValue */
                enumValue?: (baml.cffi.v1.IBamlValueEnum|null);

                /** BamlOutboundValue literalValue */
                literalValue?: (baml.cffi.v1.IBamlFieldTypeLiteral|null);

                /** BamlOutboundValue listValue */
                listValue?: (baml.cffi.v1.IBamlValueList|null);

                /** BamlOutboundValue mapValue */
                mapValue?: (baml.cffi.v1.IBamlValueMap|null);

                /** BamlOutboundValue unionVariantValue */
                unionVariantValue?: (baml.cffi.v1.IBamlValueUnionVariant|null);

                /** BamlOutboundValue checkedValue */
                checkedValue?: (baml.cffi.v1.IBamlValueChecked|null);

                /** BamlOutboundValue streamingStateValue */
                streamingStateValue?: (baml.cffi.v1.IBamlValueStreamingState|null);

                /** BamlOutboundValue handleValue */
                handleValue?: (baml.cffi.v1.IBamlHandle|null);

                /** BamlOutboundValue mediaValue */
                mediaValue?: (baml.cffi.v1.IBamlValueMedia|null);

                /** BamlOutboundValue promptAstValue */
                promptAstValue?: (baml.cffi.v1.IBamlValuePromptAst|null);

                /** BamlOutboundValue uint8arrayValue */
                uint8arrayValue?: (Uint8Array|null);
            }

            /** Represents a BamlOutboundValue. */
            class BamlOutboundValue implements IBamlOutboundValue {

                /**
                 * Constructs a new BamlOutboundValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlOutboundValue);

                /** BamlOutboundValue nullValue. */
                public nullValue?: (baml.cffi.v1.IBamlValueNull|null);

                /** BamlOutboundValue stringValue. */
                public stringValue?: (string|null);

                /** BamlOutboundValue intValue. */
                public intValue?: (number|Long|null);

                /** BamlOutboundValue floatValue. */
                public floatValue?: (number|null);

                /** BamlOutboundValue boolValue. */
                public boolValue?: (boolean|null);

                /** BamlOutboundValue classValue. */
                public classValue?: (baml.cffi.v1.IBamlValueClass|null);

                /** BamlOutboundValue enumValue. */
                public enumValue?: (baml.cffi.v1.IBamlValueEnum|null);

                /** BamlOutboundValue literalValue. */
                public literalValue?: (baml.cffi.v1.IBamlFieldTypeLiteral|null);

                /** BamlOutboundValue listValue. */
                public listValue?: (baml.cffi.v1.IBamlValueList|null);

                /** BamlOutboundValue mapValue. */
                public mapValue?: (baml.cffi.v1.IBamlValueMap|null);

                /** BamlOutboundValue unionVariantValue. */
                public unionVariantValue?: (baml.cffi.v1.IBamlValueUnionVariant|null);

                /** BamlOutboundValue checkedValue. */
                public checkedValue?: (baml.cffi.v1.IBamlValueChecked|null);

                /** BamlOutboundValue streamingStateValue. */
                public streamingStateValue?: (baml.cffi.v1.IBamlValueStreamingState|null);

                /** BamlOutboundValue handleValue. */
                public handleValue?: (baml.cffi.v1.IBamlHandle|null);

                /** BamlOutboundValue mediaValue. */
                public mediaValue?: (baml.cffi.v1.IBamlValueMedia|null);

                /** BamlOutboundValue promptAstValue. */
                public promptAstValue?: (baml.cffi.v1.IBamlValuePromptAst|null);

                /** BamlOutboundValue uint8arrayValue. */
                public uint8arrayValue?: (Uint8Array|null);

                /** BamlOutboundValue value. */
                public value?: ("nullValue"|"stringValue"|"intValue"|"floatValue"|"boolValue"|"classValue"|"enumValue"|"literalValue"|"listValue"|"mapValue"|"unionVariantValue"|"checkedValue"|"streamingStateValue"|"handleValue"|"mediaValue"|"promptAstValue"|"uint8arrayValue");

                /**
                 * Creates a new BamlOutboundValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlOutboundValue instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlOutboundValue): baml.cffi.v1.BamlOutboundValue;

                /**
                 * Encodes the specified BamlOutboundValue message. Does not implicitly {@link baml.cffi.v1.BamlOutboundValue.verify|verify} messages.
                 * @param message BamlOutboundValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlOutboundValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlOutboundValue message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlOutboundValue.verify|verify} messages.
                 * @param message BamlOutboundValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlOutboundValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlOutboundValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlOutboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlOutboundValue;

                /**
                 * Decodes a BamlOutboundValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlOutboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlOutboundValue;

                /**
                 * Verifies a BamlOutboundValue message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlOutboundValue message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlOutboundValue
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlOutboundValue;

                /**
                 * Creates a plain object from a BamlOutboundValue message. Also converts values to other types if specified.
                 * @param message BamlOutboundValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlOutboundValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlOutboundValue to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlOutboundValue
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** BamlTypeNamespace enum. */
            enum BamlTypeNamespace {
                INTERNAL = 0,
                TYPES = 1,
                STREAM_TYPES = 2,
                STREAM_STATE_TYPES = 3,
                CHECKED_TYPES = 4
            }

            /** Properties of a BamlTypeName. */
            interface IBamlTypeName {

                /** BamlTypeName namespace */
                namespace?: (baml.cffi.v1.BamlTypeNamespace|null);

                /** BamlTypeName name */
                name?: (string|null);
            }

            /** Represents a BamlTypeName. */
            class BamlTypeName implements IBamlTypeName {

                /**
                 * Constructs a new BamlTypeName.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlTypeName);

                /** BamlTypeName namespace. */
                public namespace: baml.cffi.v1.BamlTypeNamespace;

                /** BamlTypeName name. */
                public name: string;

                /**
                 * Creates a new BamlTypeName instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTypeName instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlTypeName): baml.cffi.v1.BamlTypeName;

                /**
                 * Encodes the specified BamlTypeName message. Does not implicitly {@link baml.cffi.v1.BamlTypeName.verify|verify} messages.
                 * @param message BamlTypeName message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlTypeName, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTypeName message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlTypeName.verify|verify} messages.
                 * @param message BamlTypeName message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlTypeName, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTypeName message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTypeName
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlTypeName;

                /**
                 * Decodes a BamlTypeName message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTypeName
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlTypeName;

                /**
                 * Verifies a BamlTypeName message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTypeName message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTypeName
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlTypeName;

                /**
                 * Creates a plain object from a BamlTypeName message. Also converts values to other types if specified.
                 * @param message BamlTypeName
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlTypeName, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTypeName to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTypeName
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlValueNull. */
            interface IBamlValueNull {
            }

            /** Represents a BamlValueNull. */
            class BamlValueNull implements IBamlValueNull {

                /**
                 * Constructs a new BamlValueNull.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValueNull);

                /**
                 * Creates a new BamlValueNull instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueNull instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValueNull): baml.cffi.v1.BamlValueNull;

                /**
                 * Encodes the specified BamlValueNull message. Does not implicitly {@link baml.cffi.v1.BamlValueNull.verify|verify} messages.
                 * @param message BamlValueNull message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValueNull, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueNull message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueNull.verify|verify} messages.
                 * @param message BamlValueNull message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValueNull, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueNull message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValueNull;

                /**
                 * Decodes a BamlValueNull message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValueNull;

                /**
                 * Verifies a BamlValueNull message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValueNull message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValueNull
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValueNull;

                /**
                 * Creates a plain object from a BamlValueNull message. Also converts values to other types if specified.
                 * @param message BamlValueNull
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValueNull, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValueNull to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValueNull
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlValueList. */
            interface IBamlValueList {

                /** BamlValueList itemType */
                itemType?: (baml.cffi.v1.IBamlFieldType|null);

                /** BamlValueList items */
                items?: (baml.cffi.v1.IBamlOutboundValue[]|null);
            }

            /** Represents a BamlValueList. */
            class BamlValueList implements IBamlValueList {

                /**
                 * Constructs a new BamlValueList.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValueList);

                /** BamlValueList itemType. */
                public itemType?: (baml.cffi.v1.IBamlFieldType|null);

                /** BamlValueList items. */
                public items: baml.cffi.v1.IBamlOutboundValue[];

                /**
                 * Creates a new BamlValueList instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueList instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValueList): baml.cffi.v1.BamlValueList;

                /**
                 * Encodes the specified BamlValueList message. Does not implicitly {@link baml.cffi.v1.BamlValueList.verify|verify} messages.
                 * @param message BamlValueList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValueList, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueList message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueList.verify|verify} messages.
                 * @param message BamlValueList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValueList, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueList message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValueList;

                /**
                 * Decodes a BamlValueList message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValueList;

                /**
                 * Verifies a BamlValueList message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValueList message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValueList
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValueList;

                /**
                 * Creates a plain object from a BamlValueList message. Also converts values to other types if specified.
                 * @param message BamlValueList
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValueList, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValueList to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValueList
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlOutboundMapEntry. */
            interface IBamlOutboundMapEntry {

                /** BamlOutboundMapEntry key */
                key?: (string|null);

                /** BamlOutboundMapEntry value */
                value?: (baml.cffi.v1.IBamlOutboundValue|null);
            }

            /** Represents a BamlOutboundMapEntry. */
            class BamlOutboundMapEntry implements IBamlOutboundMapEntry {

                /**
                 * Constructs a new BamlOutboundMapEntry.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlOutboundMapEntry);

                /** BamlOutboundMapEntry key. */
                public key: string;

                /** BamlOutboundMapEntry value. */
                public value?: (baml.cffi.v1.IBamlOutboundValue|null);

                /**
                 * Creates a new BamlOutboundMapEntry instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlOutboundMapEntry instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlOutboundMapEntry): baml.cffi.v1.BamlOutboundMapEntry;

                /**
                 * Encodes the specified BamlOutboundMapEntry message. Does not implicitly {@link baml.cffi.v1.BamlOutboundMapEntry.verify|verify} messages.
                 * @param message BamlOutboundMapEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlOutboundMapEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlOutboundMapEntry message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlOutboundMapEntry.verify|verify} messages.
                 * @param message BamlOutboundMapEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlOutboundMapEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlOutboundMapEntry message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlOutboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlOutboundMapEntry;

                /**
                 * Decodes a BamlOutboundMapEntry message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlOutboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlOutboundMapEntry;

                /**
                 * Verifies a BamlOutboundMapEntry message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlOutboundMapEntry message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlOutboundMapEntry
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlOutboundMapEntry;

                /**
                 * Creates a plain object from a BamlOutboundMapEntry message. Also converts values to other types if specified.
                 * @param message BamlOutboundMapEntry
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlOutboundMapEntry, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlOutboundMapEntry to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlOutboundMapEntry
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlValueMap. */
            interface IBamlValueMap {

                /** BamlValueMap keyType */
                keyType?: (baml.cffi.v1.IBamlFieldType|null);

                /** BamlValueMap valueType */
                valueType?: (baml.cffi.v1.IBamlFieldType|null);

                /** BamlValueMap entries */
                entries?: (baml.cffi.v1.IBamlOutboundMapEntry[]|null);
            }

            /** Represents a BamlValueMap. */
            class BamlValueMap implements IBamlValueMap {

                /**
                 * Constructs a new BamlValueMap.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValueMap);

                /** BamlValueMap keyType. */
                public keyType?: (baml.cffi.v1.IBamlFieldType|null);

                /** BamlValueMap valueType. */
                public valueType?: (baml.cffi.v1.IBamlFieldType|null);

                /** BamlValueMap entries. */
                public entries: baml.cffi.v1.IBamlOutboundMapEntry[];

                /**
                 * Creates a new BamlValueMap instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueMap instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValueMap): baml.cffi.v1.BamlValueMap;

                /**
                 * Encodes the specified BamlValueMap message. Does not implicitly {@link baml.cffi.v1.BamlValueMap.verify|verify} messages.
                 * @param message BamlValueMap message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValueMap, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueMap message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueMap.verify|verify} messages.
                 * @param message BamlValueMap message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValueMap, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueMap message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValueMap;

                /**
                 * Decodes a BamlValueMap message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValueMap;

                /**
                 * Verifies a BamlValueMap message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValueMap message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValueMap
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValueMap;

                /**
                 * Creates a plain object from a BamlValueMap message. Also converts values to other types if specified.
                 * @param message BamlValueMap
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValueMap, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValueMap to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValueMap
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlValueClass. */
            interface IBamlValueClass {

                /** BamlValueClass name */
                name?: (baml.cffi.v1.IBamlTypeName|null);

                /** BamlValueClass fields */
                fields?: (baml.cffi.v1.IBamlOutboundMapEntry[]|null);
            }

            /** Represents a BamlValueClass. */
            class BamlValueClass implements IBamlValueClass {

                /**
                 * Constructs a new BamlValueClass.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValueClass);

                /** BamlValueClass name. */
                public name?: (baml.cffi.v1.IBamlTypeName|null);

                /** BamlValueClass fields. */
                public fields: baml.cffi.v1.IBamlOutboundMapEntry[];

                /**
                 * Creates a new BamlValueClass instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueClass instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValueClass): baml.cffi.v1.BamlValueClass;

                /**
                 * Encodes the specified BamlValueClass message. Does not implicitly {@link baml.cffi.v1.BamlValueClass.verify|verify} messages.
                 * @param message BamlValueClass message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValueClass, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueClass message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueClass.verify|verify} messages.
                 * @param message BamlValueClass message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValueClass, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueClass message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValueClass;

                /**
                 * Decodes a BamlValueClass message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValueClass;

                /**
                 * Verifies a BamlValueClass message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValueClass message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValueClass
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValueClass;

                /**
                 * Creates a plain object from a BamlValueClass message. Also converts values to other types if specified.
                 * @param message BamlValueClass
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValueClass, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValueClass to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValueClass
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlValueEnum. */
            interface IBamlValueEnum {

                /** BamlValueEnum name */
                name?: (baml.cffi.v1.IBamlTypeName|null);

                /** BamlValueEnum value */
                value?: (string|null);

                /** BamlValueEnum isDynamic */
                isDynamic?: (boolean|null);
            }

            /** Represents a BamlValueEnum. */
            class BamlValueEnum implements IBamlValueEnum {

                /**
                 * Constructs a new BamlValueEnum.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValueEnum);

                /** BamlValueEnum name. */
                public name?: (baml.cffi.v1.IBamlTypeName|null);

                /** BamlValueEnum value. */
                public value: string;

                /** BamlValueEnum isDynamic. */
                public isDynamic: boolean;

                /**
                 * Creates a new BamlValueEnum instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueEnum instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValueEnum): baml.cffi.v1.BamlValueEnum;

                /**
                 * Encodes the specified BamlValueEnum message. Does not implicitly {@link baml.cffi.v1.BamlValueEnum.verify|verify} messages.
                 * @param message BamlValueEnum message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValueEnum, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueEnum message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueEnum.verify|verify} messages.
                 * @param message BamlValueEnum message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValueEnum, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueEnum message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValueEnum;

                /**
                 * Decodes a BamlValueEnum message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValueEnum;

                /**
                 * Verifies a BamlValueEnum message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValueEnum message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValueEnum
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValueEnum;

                /**
                 * Creates a plain object from a BamlValueEnum message. Also converts values to other types if specified.
                 * @param message BamlValueEnum
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValueEnum, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValueEnum to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValueEnum
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlValueUnionVariant. */
            interface IBamlValueUnionVariant {

                /** BamlValueUnionVariant name */
                name?: (baml.cffi.v1.IBamlTypeName|null);

                /** BamlValueUnionVariant isOptional */
                isOptional?: (boolean|null);

                /** BamlValueUnionVariant isSinglePattern */
                isSinglePattern?: (boolean|null);

                /** BamlValueUnionVariant selfType */
                selfType?: (baml.cffi.v1.IBamlFieldType|null);

                /** BamlValueUnionVariant valueOptionName */
                valueOptionName?: (string|null);

                /** BamlValueUnionVariant value */
                value?: (baml.cffi.v1.IBamlOutboundValue|null);
            }

            /** Represents a BamlValueUnionVariant. */
            class BamlValueUnionVariant implements IBamlValueUnionVariant {

                /**
                 * Constructs a new BamlValueUnionVariant.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValueUnionVariant);

                /** BamlValueUnionVariant name. */
                public name?: (baml.cffi.v1.IBamlTypeName|null);

                /** BamlValueUnionVariant isOptional. */
                public isOptional: boolean;

                /** BamlValueUnionVariant isSinglePattern. */
                public isSinglePattern: boolean;

                /** BamlValueUnionVariant selfType. */
                public selfType?: (baml.cffi.v1.IBamlFieldType|null);

                /** BamlValueUnionVariant valueOptionName. */
                public valueOptionName: string;

                /** BamlValueUnionVariant value. */
                public value?: (baml.cffi.v1.IBamlOutboundValue|null);

                /**
                 * Creates a new BamlValueUnionVariant instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueUnionVariant instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValueUnionVariant): baml.cffi.v1.BamlValueUnionVariant;

                /**
                 * Encodes the specified BamlValueUnionVariant message. Does not implicitly {@link baml.cffi.v1.BamlValueUnionVariant.verify|verify} messages.
                 * @param message BamlValueUnionVariant message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValueUnionVariant, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueUnionVariant message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueUnionVariant.verify|verify} messages.
                 * @param message BamlValueUnionVariant message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValueUnionVariant, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueUnionVariant message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValueUnionVariant;

                /**
                 * Decodes a BamlValueUnionVariant message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValueUnionVariant;

                /**
                 * Verifies a BamlValueUnionVariant message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValueUnionVariant message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValueUnionVariant
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValueUnionVariant;

                /**
                 * Creates a plain object from a BamlValueUnionVariant message. Also converts values to other types if specified.
                 * @param message BamlValueUnionVariant
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValueUnionVariant, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValueUnionVariant to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValueUnionVariant
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlValueChecked. */
            interface IBamlValueChecked {

                /** BamlValueChecked name */
                name?: (baml.cffi.v1.IBamlTypeName|null);

                /** BamlValueChecked value */
                value?: (baml.cffi.v1.IBamlOutboundValue|null);

                /** BamlValueChecked checks */
                checks?: (baml.cffi.v1.IBamlCheckValue[]|null);
            }

            /** Represents a BamlValueChecked. */
            class BamlValueChecked implements IBamlValueChecked {

                /**
                 * Constructs a new BamlValueChecked.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValueChecked);

                /** BamlValueChecked name. */
                public name?: (baml.cffi.v1.IBamlTypeName|null);

                /** BamlValueChecked value. */
                public value?: (baml.cffi.v1.IBamlOutboundValue|null);

                /** BamlValueChecked checks. */
                public checks: baml.cffi.v1.IBamlCheckValue[];

                /**
                 * Creates a new BamlValueChecked instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueChecked instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValueChecked): baml.cffi.v1.BamlValueChecked;

                /**
                 * Encodes the specified BamlValueChecked message. Does not implicitly {@link baml.cffi.v1.BamlValueChecked.verify|verify} messages.
                 * @param message BamlValueChecked message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValueChecked, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueChecked message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueChecked.verify|verify} messages.
                 * @param message BamlValueChecked message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValueChecked, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueChecked message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueChecked
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValueChecked;

                /**
                 * Decodes a BamlValueChecked message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueChecked
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValueChecked;

                /**
                 * Verifies a BamlValueChecked message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValueChecked message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValueChecked
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValueChecked;

                /**
                 * Creates a plain object from a BamlValueChecked message. Also converts values to other types if specified.
                 * @param message BamlValueChecked
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValueChecked, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValueChecked to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValueChecked
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** MediaTypeEnum enum. */
            enum MediaTypeEnum {
                MEDIA_TYPE_UNSPECIFIED = 0,
                IMAGE = 1,
                AUDIO = 2,
                PDF = 3,
                VIDEO = 4,
                OTHER = 5
            }

            /** Properties of a BamlValueMedia. */
            interface IBamlValueMedia {

                /** BamlValueMedia media */
                media?: (baml.cffi.v1.MediaTypeEnum|null);

                /** BamlValueMedia mimeType */
                mimeType?: (string|null);

                /** BamlValueMedia url */
                url?: (string|null);

                /** BamlValueMedia base64 */
                base64?: (string|null);

                /** BamlValueMedia file */
                file?: (string|null);
            }

            /** Represents a BamlValueMedia. */
            class BamlValueMedia implements IBamlValueMedia {

                /**
                 * Constructs a new BamlValueMedia.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValueMedia);

                /** BamlValueMedia media. */
                public media: baml.cffi.v1.MediaTypeEnum;

                /** BamlValueMedia mimeType. */
                public mimeType?: (string|null);

                /** BamlValueMedia url. */
                public url?: (string|null);

                /** BamlValueMedia base64. */
                public base64?: (string|null);

                /** BamlValueMedia file. */
                public file?: (string|null);

                /** BamlValueMedia value. */
                public value?: ("url"|"base64"|"file");

                /**
                 * Creates a new BamlValueMedia instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueMedia instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValueMedia): baml.cffi.v1.BamlValueMedia;

                /**
                 * Encodes the specified BamlValueMedia message. Does not implicitly {@link baml.cffi.v1.BamlValueMedia.verify|verify} messages.
                 * @param message BamlValueMedia message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValueMedia, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueMedia message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueMedia.verify|verify} messages.
                 * @param message BamlValueMedia message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValueMedia, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueMedia message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValueMedia;

                /**
                 * Decodes a BamlValueMedia message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValueMedia;

                /**
                 * Verifies a BamlValueMedia message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValueMedia message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValueMedia
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValueMedia;

                /**
                 * Creates a plain object from a BamlValueMedia message. Also converts values to other types if specified.
                 * @param message BamlValueMedia
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValueMedia, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValueMedia to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValueMedia
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlValuePromptAst. */
            interface IBamlValuePromptAst {

                /** BamlValuePromptAst simple */
                simple?: (baml.cffi.v1.IBamlValuePromptAstSimple|null);

                /** BamlValuePromptAst message */
                message?: (baml.cffi.v1.IBamlValuePromptAstMessage|null);

                /** BamlValuePromptAst multiple */
                multiple?: (baml.cffi.v1.IBamlValuePromptAstMultiple|null);
            }

            /** Represents a BamlValuePromptAst. */
            class BamlValuePromptAst implements IBamlValuePromptAst {

                /**
                 * Constructs a new BamlValuePromptAst.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValuePromptAst);

                /** BamlValuePromptAst simple. */
                public simple?: (baml.cffi.v1.IBamlValuePromptAstSimple|null);

                /** BamlValuePromptAst message. */
                public message?: (baml.cffi.v1.IBamlValuePromptAstMessage|null);

                /** BamlValuePromptAst multiple. */
                public multiple?: (baml.cffi.v1.IBamlValuePromptAstMultiple|null);

                /** BamlValuePromptAst value. */
                public value?: ("simple"|"message"|"multiple");

                /**
                 * Creates a new BamlValuePromptAst instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAst instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValuePromptAst): baml.cffi.v1.BamlValuePromptAst;

                /**
                 * Encodes the specified BamlValuePromptAst message. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAst.verify|verify} messages.
                 * @param message BamlValuePromptAst message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValuePromptAst, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAst message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAst.verify|verify} messages.
                 * @param message BamlValuePromptAst message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValuePromptAst, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAst message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAst
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValuePromptAst;

                /**
                 * Decodes a BamlValuePromptAst message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAst
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValuePromptAst;

                /**
                 * Verifies a BamlValuePromptAst message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValuePromptAst message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValuePromptAst
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValuePromptAst;

                /**
                 * Creates a plain object from a BamlValuePromptAst message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAst
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValuePromptAst, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValuePromptAst to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValuePromptAst
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlValuePromptAstMessage. */
            interface IBamlValuePromptAstMessage {

                /** BamlValuePromptAstMessage role */
                role?: (string|null);

                /** BamlValuePromptAstMessage content */
                content?: (baml.cffi.v1.IBamlValuePromptAstSimple|null);

                /** BamlValuePromptAstMessage metadataAsJson */
                metadataAsJson?: (string|null);
            }

            /** Represents a BamlValuePromptAstMessage. */
            class BamlValuePromptAstMessage implements IBamlValuePromptAstMessage {

                /**
                 * Constructs a new BamlValuePromptAstMessage.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValuePromptAstMessage);

                /** BamlValuePromptAstMessage role. */
                public role: string;

                /** BamlValuePromptAstMessage content. */
                public content?: (baml.cffi.v1.IBamlValuePromptAstSimple|null);

                /** BamlValuePromptAstMessage metadataAsJson. */
                public metadataAsJson: string;

                /**
                 * Creates a new BamlValuePromptAstMessage instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAstMessage instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValuePromptAstMessage): baml.cffi.v1.BamlValuePromptAstMessage;

                /**
                 * Encodes the specified BamlValuePromptAstMessage message. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstMessage.verify|verify} messages.
                 * @param message BamlValuePromptAstMessage message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValuePromptAstMessage, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAstMessage message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstMessage.verify|verify} messages.
                 * @param message BamlValuePromptAstMessage message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValuePromptAstMessage, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAstMessage message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAstMessage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValuePromptAstMessage;

                /**
                 * Decodes a BamlValuePromptAstMessage message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAstMessage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValuePromptAstMessage;

                /**
                 * Verifies a BamlValuePromptAstMessage message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValuePromptAstMessage message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValuePromptAstMessage
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValuePromptAstMessage;

                /**
                 * Creates a plain object from a BamlValuePromptAstMessage message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAstMessage
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValuePromptAstMessage, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValuePromptAstMessage to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValuePromptAstMessage
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlValuePromptAstMultiple. */
            interface IBamlValuePromptAstMultiple {

                /** BamlValuePromptAstMultiple items */
                items?: (baml.cffi.v1.IBamlValuePromptAst[]|null);
            }

            /** Represents a BamlValuePromptAstMultiple. */
            class BamlValuePromptAstMultiple implements IBamlValuePromptAstMultiple {

                /**
                 * Constructs a new BamlValuePromptAstMultiple.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValuePromptAstMultiple);

                /** BamlValuePromptAstMultiple items. */
                public items: baml.cffi.v1.IBamlValuePromptAst[];

                /**
                 * Creates a new BamlValuePromptAstMultiple instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAstMultiple instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValuePromptAstMultiple): baml.cffi.v1.BamlValuePromptAstMultiple;

                /**
                 * Encodes the specified BamlValuePromptAstMultiple message. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstMultiple.verify|verify} messages.
                 * @param message BamlValuePromptAstMultiple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValuePromptAstMultiple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAstMultiple message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstMultiple.verify|verify} messages.
                 * @param message BamlValuePromptAstMultiple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValuePromptAstMultiple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAstMultiple message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAstMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValuePromptAstMultiple;

                /**
                 * Decodes a BamlValuePromptAstMultiple message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAstMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValuePromptAstMultiple;

                /**
                 * Verifies a BamlValuePromptAstMultiple message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValuePromptAstMultiple message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValuePromptAstMultiple
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValuePromptAstMultiple;

                /**
                 * Creates a plain object from a BamlValuePromptAstMultiple message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAstMultiple
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValuePromptAstMultiple, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValuePromptAstMultiple to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValuePromptAstMultiple
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlValuePromptAstSimple. */
            interface IBamlValuePromptAstSimple {

                /** BamlValuePromptAstSimple string */
                string?: (string|null);

                /** BamlValuePromptAstSimple media */
                media?: (baml.cffi.v1.IBamlValueMedia|null);

                /** BamlValuePromptAstSimple multiple */
                multiple?: (baml.cffi.v1.IBamlValuePromptAstSimpleMultiple|null);
            }

            /** Represents a BamlValuePromptAstSimple. */
            class BamlValuePromptAstSimple implements IBamlValuePromptAstSimple {

                /**
                 * Constructs a new BamlValuePromptAstSimple.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValuePromptAstSimple);

                /** BamlValuePromptAstSimple string. */
                public string?: (string|null);

                /** BamlValuePromptAstSimple media. */
                public media?: (baml.cffi.v1.IBamlValueMedia|null);

                /** BamlValuePromptAstSimple multiple. */
                public multiple?: (baml.cffi.v1.IBamlValuePromptAstSimpleMultiple|null);

                /** BamlValuePromptAstSimple value. */
                public value?: ("string"|"media"|"multiple");

                /**
                 * Creates a new BamlValuePromptAstSimple instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAstSimple instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValuePromptAstSimple): baml.cffi.v1.BamlValuePromptAstSimple;

                /**
                 * Encodes the specified BamlValuePromptAstSimple message. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstSimple.verify|verify} messages.
                 * @param message BamlValuePromptAstSimple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValuePromptAstSimple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAstSimple message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstSimple.verify|verify} messages.
                 * @param message BamlValuePromptAstSimple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValuePromptAstSimple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAstSimple message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAstSimple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValuePromptAstSimple;

                /**
                 * Decodes a BamlValuePromptAstSimple message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAstSimple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValuePromptAstSimple;

                /**
                 * Verifies a BamlValuePromptAstSimple message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValuePromptAstSimple message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValuePromptAstSimple
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValuePromptAstSimple;

                /**
                 * Creates a plain object from a BamlValuePromptAstSimple message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAstSimple
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValuePromptAstSimple, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValuePromptAstSimple to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValuePromptAstSimple
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlValuePromptAstSimpleMultiple. */
            interface IBamlValuePromptAstSimpleMultiple {

                /** BamlValuePromptAstSimpleMultiple items */
                items?: (baml.cffi.v1.IBamlValuePromptAstSimple[]|null);
            }

            /** Represents a BamlValuePromptAstSimpleMultiple. */
            class BamlValuePromptAstSimpleMultiple implements IBamlValuePromptAstSimpleMultiple {

                /**
                 * Constructs a new BamlValuePromptAstSimpleMultiple.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValuePromptAstSimpleMultiple);

                /** BamlValuePromptAstSimpleMultiple items. */
                public items: baml.cffi.v1.IBamlValuePromptAstSimple[];

                /**
                 * Creates a new BamlValuePromptAstSimpleMultiple instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAstSimpleMultiple instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValuePromptAstSimpleMultiple): baml.cffi.v1.BamlValuePromptAstSimpleMultiple;

                /**
                 * Encodes the specified BamlValuePromptAstSimpleMultiple message. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstSimpleMultiple.verify|verify} messages.
                 * @param message BamlValuePromptAstSimpleMultiple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValuePromptAstSimpleMultiple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAstSimpleMultiple message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValuePromptAstSimpleMultiple.verify|verify} messages.
                 * @param message BamlValuePromptAstSimpleMultiple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValuePromptAstSimpleMultiple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAstSimpleMultiple message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAstSimpleMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValuePromptAstSimpleMultiple;

                /**
                 * Decodes a BamlValuePromptAstSimpleMultiple message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAstSimpleMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValuePromptAstSimpleMultiple;

                /**
                 * Verifies a BamlValuePromptAstSimpleMultiple message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValuePromptAstSimpleMultiple message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValuePromptAstSimpleMultiple
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValuePromptAstSimpleMultiple;

                /**
                 * Creates a plain object from a BamlValuePromptAstSimpleMultiple message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAstSimpleMultiple
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValuePromptAstSimpleMultiple, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValuePromptAstSimpleMultiple to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValuePromptAstSimpleMultiple
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldType. */
            interface IBamlFieldType {

                /** BamlFieldType stringType */
                stringType?: (baml.cffi.v1.IBamlFieldTypeString|null);

                /** BamlFieldType intType */
                intType?: (baml.cffi.v1.IBamlFieldTypeInt|null);

                /** BamlFieldType floatType */
                floatType?: (baml.cffi.v1.IBamlFieldTypeFloat|null);

                /** BamlFieldType boolType */
                boolType?: (baml.cffi.v1.IBamlFieldTypeBool|null);

                /** BamlFieldType nullType */
                nullType?: (baml.cffi.v1.IBamlFieldTypeNull|null);

                /** BamlFieldType literalType */
                literalType?: (baml.cffi.v1.IBamlFieldTypeLiteral|null);

                /** BamlFieldType mediaType */
                mediaType?: (baml.cffi.v1.IBamlFieldTypeMedia|null);

                /** BamlFieldType enumType */
                enumType?: (baml.cffi.v1.IBamlFieldTypeEnum|null);

                /** BamlFieldType classType */
                classType?: (baml.cffi.v1.IBamlFieldTypeClass|null);

                /** BamlFieldType typeAliasType */
                typeAliasType?: (baml.cffi.v1.IBamlFieldTypeTypeAlias|null);

                /** BamlFieldType listType */
                listType?: (baml.cffi.v1.IBamlFieldTypeList|null);

                /** BamlFieldType mapType */
                mapType?: (baml.cffi.v1.IBamlFieldTypeMap|null);

                /** BamlFieldType unionVariantType */
                unionVariantType?: (baml.cffi.v1.IBamlFieldTypeUnionVariant|null);

                /** BamlFieldType optionalType */
                optionalType?: (baml.cffi.v1.IBamlFieldTypeOptional|null);

                /** BamlFieldType checkedType */
                checkedType?: (baml.cffi.v1.IBamlFieldTypeChecked|null);

                /** BamlFieldType streamStateType */
                streamStateType?: (baml.cffi.v1.IBamlFieldTypeStreamState|null);

                /** BamlFieldType anyType */
                anyType?: (baml.cffi.v1.IBamlFieldTypeAny|null);

                /** BamlFieldType uint8arrayType */
                uint8arrayType?: (baml.cffi.v1.IBamlFieldTypeUint8Array|null);

                /** BamlFieldType unknownType */
                unknownType?: (baml.cffi.v1.IBamlFieldTypeUnknown|null);
            }

            /** Represents a BamlFieldType. */
            class BamlFieldType implements IBamlFieldType {

                /**
                 * Constructs a new BamlFieldType.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldType);

                /** BamlFieldType stringType. */
                public stringType?: (baml.cffi.v1.IBamlFieldTypeString|null);

                /** BamlFieldType intType. */
                public intType?: (baml.cffi.v1.IBamlFieldTypeInt|null);

                /** BamlFieldType floatType. */
                public floatType?: (baml.cffi.v1.IBamlFieldTypeFloat|null);

                /** BamlFieldType boolType. */
                public boolType?: (baml.cffi.v1.IBamlFieldTypeBool|null);

                /** BamlFieldType nullType. */
                public nullType?: (baml.cffi.v1.IBamlFieldTypeNull|null);

                /** BamlFieldType literalType. */
                public literalType?: (baml.cffi.v1.IBamlFieldTypeLiteral|null);

                /** BamlFieldType mediaType. */
                public mediaType?: (baml.cffi.v1.IBamlFieldTypeMedia|null);

                /** BamlFieldType enumType. */
                public enumType?: (baml.cffi.v1.IBamlFieldTypeEnum|null);

                /** BamlFieldType classType. */
                public classType?: (baml.cffi.v1.IBamlFieldTypeClass|null);

                /** BamlFieldType typeAliasType. */
                public typeAliasType?: (baml.cffi.v1.IBamlFieldTypeTypeAlias|null);

                /** BamlFieldType listType. */
                public listType?: (baml.cffi.v1.IBamlFieldTypeList|null);

                /** BamlFieldType mapType. */
                public mapType?: (baml.cffi.v1.IBamlFieldTypeMap|null);

                /** BamlFieldType unionVariantType. */
                public unionVariantType?: (baml.cffi.v1.IBamlFieldTypeUnionVariant|null);

                /** BamlFieldType optionalType. */
                public optionalType?: (baml.cffi.v1.IBamlFieldTypeOptional|null);

                /** BamlFieldType checkedType. */
                public checkedType?: (baml.cffi.v1.IBamlFieldTypeChecked|null);

                /** BamlFieldType streamStateType. */
                public streamStateType?: (baml.cffi.v1.IBamlFieldTypeStreamState|null);

                /** BamlFieldType anyType. */
                public anyType?: (baml.cffi.v1.IBamlFieldTypeAny|null);

                /** BamlFieldType uint8arrayType. */
                public uint8arrayType?: (baml.cffi.v1.IBamlFieldTypeUint8Array|null);

                /** BamlFieldType unknownType. */
                public unknownType?: (baml.cffi.v1.IBamlFieldTypeUnknown|null);

                /** BamlFieldType type. */
                public type?: ("stringType"|"intType"|"floatType"|"boolType"|"nullType"|"literalType"|"mediaType"|"enumType"|"classType"|"typeAliasType"|"listType"|"mapType"|"unionVariantType"|"optionalType"|"checkedType"|"streamStateType"|"anyType"|"uint8arrayType"|"unknownType");

                /**
                 * Creates a new BamlFieldType instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldType instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldType): baml.cffi.v1.BamlFieldType;

                /**
                 * Encodes the specified BamlFieldType message. Does not implicitly {@link baml.cffi.v1.BamlFieldType.verify|verify} messages.
                 * @param message BamlFieldType message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldType, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldType message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldType.verify|verify} messages.
                 * @param message BamlFieldType message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldType, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldType message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldType
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldType;

                /**
                 * Decodes a BamlFieldType message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldType
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldType;

                /**
                 * Verifies a BamlFieldType message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldType message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldType
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldType;

                /**
                 * Creates a plain object from a BamlFieldType message. Also converts values to other types if specified.
                 * @param message BamlFieldType
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldType, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldType to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldType
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeString. */
            interface IBamlFieldTypeString {
            }

            /** Represents a BamlFieldTypeString. */
            class BamlFieldTypeString implements IBamlFieldTypeString {

                /**
                 * Constructs a new BamlFieldTypeString.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeString);

                /**
                 * Creates a new BamlFieldTypeString instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeString instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeString): baml.cffi.v1.BamlFieldTypeString;

                /**
                 * Encodes the specified BamlFieldTypeString message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeString.verify|verify} messages.
                 * @param message BamlFieldTypeString message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeString, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeString message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeString.verify|verify} messages.
                 * @param message BamlFieldTypeString message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeString, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeString message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeString;

                /**
                 * Decodes a BamlFieldTypeString message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeString;

                /**
                 * Verifies a BamlFieldTypeString message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeString message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeString
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeString;

                /**
                 * Creates a plain object from a BamlFieldTypeString message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeString
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeString, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeString to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeString
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeInt. */
            interface IBamlFieldTypeInt {
            }

            /** Represents a BamlFieldTypeInt. */
            class BamlFieldTypeInt implements IBamlFieldTypeInt {

                /**
                 * Constructs a new BamlFieldTypeInt.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeInt);

                /**
                 * Creates a new BamlFieldTypeInt instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeInt instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeInt): baml.cffi.v1.BamlFieldTypeInt;

                /**
                 * Encodes the specified BamlFieldTypeInt message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeInt.verify|verify} messages.
                 * @param message BamlFieldTypeInt message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeInt, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeInt message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeInt.verify|verify} messages.
                 * @param message BamlFieldTypeInt message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeInt, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeInt message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeInt;

                /**
                 * Decodes a BamlFieldTypeInt message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeInt;

                /**
                 * Verifies a BamlFieldTypeInt message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeInt message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeInt
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeInt;

                /**
                 * Creates a plain object from a BamlFieldTypeInt message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeInt
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeInt, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeInt to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeInt
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeFloat. */
            interface IBamlFieldTypeFloat {
            }

            /** Represents a BamlFieldTypeFloat. */
            class BamlFieldTypeFloat implements IBamlFieldTypeFloat {

                /**
                 * Constructs a new BamlFieldTypeFloat.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeFloat);

                /**
                 * Creates a new BamlFieldTypeFloat instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeFloat instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeFloat): baml.cffi.v1.BamlFieldTypeFloat;

                /**
                 * Encodes the specified BamlFieldTypeFloat message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeFloat.verify|verify} messages.
                 * @param message BamlFieldTypeFloat message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeFloat, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeFloat message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeFloat.verify|verify} messages.
                 * @param message BamlFieldTypeFloat message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeFloat, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeFloat message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeFloat
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeFloat;

                /**
                 * Decodes a BamlFieldTypeFloat message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeFloat
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeFloat;

                /**
                 * Verifies a BamlFieldTypeFloat message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeFloat message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeFloat
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeFloat;

                /**
                 * Creates a plain object from a BamlFieldTypeFloat message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeFloat
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeFloat, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeFloat to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeFloat
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeBool. */
            interface IBamlFieldTypeBool {
            }

            /** Represents a BamlFieldTypeBool. */
            class BamlFieldTypeBool implements IBamlFieldTypeBool {

                /**
                 * Constructs a new BamlFieldTypeBool.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeBool);

                /**
                 * Creates a new BamlFieldTypeBool instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeBool instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeBool): baml.cffi.v1.BamlFieldTypeBool;

                /**
                 * Encodes the specified BamlFieldTypeBool message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeBool.verify|verify} messages.
                 * @param message BamlFieldTypeBool message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeBool, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeBool message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeBool.verify|verify} messages.
                 * @param message BamlFieldTypeBool message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeBool, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeBool message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeBool;

                /**
                 * Decodes a BamlFieldTypeBool message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeBool;

                /**
                 * Verifies a BamlFieldTypeBool message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeBool message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeBool
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeBool;

                /**
                 * Creates a plain object from a BamlFieldTypeBool message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeBool
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeBool, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeBool to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeBool
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeNull. */
            interface IBamlFieldTypeNull {
            }

            /** Represents a BamlFieldTypeNull. */
            class BamlFieldTypeNull implements IBamlFieldTypeNull {

                /**
                 * Constructs a new BamlFieldTypeNull.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeNull);

                /**
                 * Creates a new BamlFieldTypeNull instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeNull instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeNull): baml.cffi.v1.BamlFieldTypeNull;

                /**
                 * Encodes the specified BamlFieldTypeNull message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeNull.verify|verify} messages.
                 * @param message BamlFieldTypeNull message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeNull, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeNull message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeNull.verify|verify} messages.
                 * @param message BamlFieldTypeNull message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeNull, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeNull message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeNull;

                /**
                 * Decodes a BamlFieldTypeNull message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeNull;

                /**
                 * Verifies a BamlFieldTypeNull message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeNull message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeNull
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeNull;

                /**
                 * Creates a plain object from a BamlFieldTypeNull message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeNull
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeNull, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeNull to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeNull
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeUint8Array. */
            interface IBamlFieldTypeUint8Array {
            }

            /** Represents a BamlFieldTypeUint8Array. */
            class BamlFieldTypeUint8Array implements IBamlFieldTypeUint8Array {

                /**
                 * Constructs a new BamlFieldTypeUint8Array.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeUint8Array);

                /**
                 * Creates a new BamlFieldTypeUint8Array instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeUint8Array instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeUint8Array): baml.cffi.v1.BamlFieldTypeUint8Array;

                /**
                 * Encodes the specified BamlFieldTypeUint8Array message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeUint8Array.verify|verify} messages.
                 * @param message BamlFieldTypeUint8Array message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeUint8Array, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeUint8Array message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeUint8Array.verify|verify} messages.
                 * @param message BamlFieldTypeUint8Array message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeUint8Array, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeUint8Array message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeUint8Array
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeUint8Array;

                /**
                 * Decodes a BamlFieldTypeUint8Array message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeUint8Array
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeUint8Array;

                /**
                 * Verifies a BamlFieldTypeUint8Array message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeUint8Array message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeUint8Array
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeUint8Array;

                /**
                 * Creates a plain object from a BamlFieldTypeUint8Array message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeUint8Array
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeUint8Array, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeUint8Array to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeUint8Array
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeAny. */
            interface IBamlFieldTypeAny {
            }

            /** Represents a BamlFieldTypeAny. */
            class BamlFieldTypeAny implements IBamlFieldTypeAny {

                /**
                 * Constructs a new BamlFieldTypeAny.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeAny);

                /**
                 * Creates a new BamlFieldTypeAny instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeAny instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeAny): baml.cffi.v1.BamlFieldTypeAny;

                /**
                 * Encodes the specified BamlFieldTypeAny message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeAny.verify|verify} messages.
                 * @param message BamlFieldTypeAny message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeAny, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeAny message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeAny.verify|verify} messages.
                 * @param message BamlFieldTypeAny message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeAny, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeAny message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeAny
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeAny;

                /**
                 * Decodes a BamlFieldTypeAny message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeAny
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeAny;

                /**
                 * Verifies a BamlFieldTypeAny message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeAny message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeAny
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeAny;

                /**
                 * Creates a plain object from a BamlFieldTypeAny message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeAny
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeAny, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeAny to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeAny
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeUnknown. */
            interface IBamlFieldTypeUnknown {
            }

            /** Represents a BamlFieldTypeUnknown. */
            class BamlFieldTypeUnknown implements IBamlFieldTypeUnknown {

                /**
                 * Constructs a new BamlFieldTypeUnknown.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeUnknown);

                /**
                 * Creates a new BamlFieldTypeUnknown instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeUnknown instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeUnknown): baml.cffi.v1.BamlFieldTypeUnknown;

                /**
                 * Encodes the specified BamlFieldTypeUnknown message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeUnknown.verify|verify} messages.
                 * @param message BamlFieldTypeUnknown message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeUnknown, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeUnknown message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeUnknown.verify|verify} messages.
                 * @param message BamlFieldTypeUnknown message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeUnknown, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeUnknown message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeUnknown
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeUnknown;

                /**
                 * Decodes a BamlFieldTypeUnknown message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeUnknown
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeUnknown;

                /**
                 * Verifies a BamlFieldTypeUnknown message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeUnknown message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeUnknown
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeUnknown;

                /**
                 * Creates a plain object from a BamlFieldTypeUnknown message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeUnknown
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeUnknown, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeUnknown to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeUnknown
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlLiteralString. */
            interface IBamlLiteralString {

                /** BamlLiteralString value */
                value?: (string|null);
            }

            /** Represents a BamlLiteralString. */
            class BamlLiteralString implements IBamlLiteralString {

                /**
                 * Constructs a new BamlLiteralString.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlLiteralString);

                /** BamlLiteralString value. */
                public value: string;

                /**
                 * Creates a new BamlLiteralString instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlLiteralString instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlLiteralString): baml.cffi.v1.BamlLiteralString;

                /**
                 * Encodes the specified BamlLiteralString message. Does not implicitly {@link baml.cffi.v1.BamlLiteralString.verify|verify} messages.
                 * @param message BamlLiteralString message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlLiteralString, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlLiteralString message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlLiteralString.verify|verify} messages.
                 * @param message BamlLiteralString message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlLiteralString, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlLiteralString message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlLiteralString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlLiteralString;

                /**
                 * Decodes a BamlLiteralString message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlLiteralString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlLiteralString;

                /**
                 * Verifies a BamlLiteralString message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlLiteralString message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlLiteralString
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlLiteralString;

                /**
                 * Creates a plain object from a BamlLiteralString message. Also converts values to other types if specified.
                 * @param message BamlLiteralString
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlLiteralString, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlLiteralString to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlLiteralString
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlLiteralInt. */
            interface IBamlLiteralInt {

                /** BamlLiteralInt value */
                value?: (number|Long|null);
            }

            /** Represents a BamlLiteralInt. */
            class BamlLiteralInt implements IBamlLiteralInt {

                /**
                 * Constructs a new BamlLiteralInt.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlLiteralInt);

                /** BamlLiteralInt value. */
                public value: (number|Long);

                /**
                 * Creates a new BamlLiteralInt instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlLiteralInt instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlLiteralInt): baml.cffi.v1.BamlLiteralInt;

                /**
                 * Encodes the specified BamlLiteralInt message. Does not implicitly {@link baml.cffi.v1.BamlLiteralInt.verify|verify} messages.
                 * @param message BamlLiteralInt message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlLiteralInt, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlLiteralInt message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlLiteralInt.verify|verify} messages.
                 * @param message BamlLiteralInt message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlLiteralInt, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlLiteralInt message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlLiteralInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlLiteralInt;

                /**
                 * Decodes a BamlLiteralInt message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlLiteralInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlLiteralInt;

                /**
                 * Verifies a BamlLiteralInt message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlLiteralInt message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlLiteralInt
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlLiteralInt;

                /**
                 * Creates a plain object from a BamlLiteralInt message. Also converts values to other types if specified.
                 * @param message BamlLiteralInt
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlLiteralInt, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlLiteralInt to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlLiteralInt
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlLiteralBool. */
            interface IBamlLiteralBool {

                /** BamlLiteralBool value */
                value?: (boolean|null);
            }

            /** Represents a BamlLiteralBool. */
            class BamlLiteralBool implements IBamlLiteralBool {

                /**
                 * Constructs a new BamlLiteralBool.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlLiteralBool);

                /** BamlLiteralBool value. */
                public value: boolean;

                /**
                 * Creates a new BamlLiteralBool instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlLiteralBool instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlLiteralBool): baml.cffi.v1.BamlLiteralBool;

                /**
                 * Encodes the specified BamlLiteralBool message. Does not implicitly {@link baml.cffi.v1.BamlLiteralBool.verify|verify} messages.
                 * @param message BamlLiteralBool message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlLiteralBool, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlLiteralBool message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlLiteralBool.verify|verify} messages.
                 * @param message BamlLiteralBool message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlLiteralBool, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlLiteralBool message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlLiteralBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlLiteralBool;

                /**
                 * Decodes a BamlLiteralBool message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlLiteralBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlLiteralBool;

                /**
                 * Verifies a BamlLiteralBool message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlLiteralBool message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlLiteralBool
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlLiteralBool;

                /**
                 * Creates a plain object from a BamlLiteralBool message. Also converts values to other types if specified.
                 * @param message BamlLiteralBool
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlLiteralBool, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlLiteralBool to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlLiteralBool
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeLiteral. */
            interface IBamlFieldTypeLiteral {

                /** BamlFieldTypeLiteral stringLiteral */
                stringLiteral?: (baml.cffi.v1.IBamlLiteralString|null);

                /** BamlFieldTypeLiteral intLiteral */
                intLiteral?: (baml.cffi.v1.IBamlLiteralInt|null);

                /** BamlFieldTypeLiteral boolLiteral */
                boolLiteral?: (baml.cffi.v1.IBamlLiteralBool|null);
            }

            /** Represents a BamlFieldTypeLiteral. */
            class BamlFieldTypeLiteral implements IBamlFieldTypeLiteral {

                /**
                 * Constructs a new BamlFieldTypeLiteral.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeLiteral);

                /** BamlFieldTypeLiteral stringLiteral. */
                public stringLiteral?: (baml.cffi.v1.IBamlLiteralString|null);

                /** BamlFieldTypeLiteral intLiteral. */
                public intLiteral?: (baml.cffi.v1.IBamlLiteralInt|null);

                /** BamlFieldTypeLiteral boolLiteral. */
                public boolLiteral?: (baml.cffi.v1.IBamlLiteralBool|null);

                /** BamlFieldTypeLiteral literal. */
                public literal?: ("stringLiteral"|"intLiteral"|"boolLiteral");

                /**
                 * Creates a new BamlFieldTypeLiteral instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeLiteral instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeLiteral): baml.cffi.v1.BamlFieldTypeLiteral;

                /**
                 * Encodes the specified BamlFieldTypeLiteral message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeLiteral.verify|verify} messages.
                 * @param message BamlFieldTypeLiteral message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeLiteral, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeLiteral message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeLiteral.verify|verify} messages.
                 * @param message BamlFieldTypeLiteral message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeLiteral, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeLiteral message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeLiteral
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeLiteral;

                /**
                 * Decodes a BamlFieldTypeLiteral message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeLiteral
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeLiteral;

                /**
                 * Verifies a BamlFieldTypeLiteral message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeLiteral message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeLiteral
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeLiteral;

                /**
                 * Creates a plain object from a BamlFieldTypeLiteral message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeLiteral
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeLiteral, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeLiteral to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeLiteral
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeMedia. */
            interface IBamlFieldTypeMedia {

                /** BamlFieldTypeMedia media */
                media?: (baml.cffi.v1.MediaTypeEnum|null);
            }

            /** Represents a BamlFieldTypeMedia. */
            class BamlFieldTypeMedia implements IBamlFieldTypeMedia {

                /**
                 * Constructs a new BamlFieldTypeMedia.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeMedia);

                /** BamlFieldTypeMedia media. */
                public media: baml.cffi.v1.MediaTypeEnum;

                /**
                 * Creates a new BamlFieldTypeMedia instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeMedia instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeMedia): baml.cffi.v1.BamlFieldTypeMedia;

                /**
                 * Encodes the specified BamlFieldTypeMedia message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeMedia.verify|verify} messages.
                 * @param message BamlFieldTypeMedia message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeMedia, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeMedia message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeMedia.verify|verify} messages.
                 * @param message BamlFieldTypeMedia message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeMedia, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeMedia message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeMedia;

                /**
                 * Decodes a BamlFieldTypeMedia message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeMedia;

                /**
                 * Verifies a BamlFieldTypeMedia message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeMedia message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeMedia
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeMedia;

                /**
                 * Creates a plain object from a BamlFieldTypeMedia message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeMedia
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeMedia, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeMedia to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeMedia
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeEnum. */
            interface IBamlFieldTypeEnum {

                /** BamlFieldTypeEnum name */
                name?: (string|null);
            }

            /** Represents a BamlFieldTypeEnum. */
            class BamlFieldTypeEnum implements IBamlFieldTypeEnum {

                /**
                 * Constructs a new BamlFieldTypeEnum.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeEnum);

                /** BamlFieldTypeEnum name. */
                public name: string;

                /**
                 * Creates a new BamlFieldTypeEnum instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeEnum instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeEnum): baml.cffi.v1.BamlFieldTypeEnum;

                /**
                 * Encodes the specified BamlFieldTypeEnum message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeEnum.verify|verify} messages.
                 * @param message BamlFieldTypeEnum message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeEnum, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeEnum message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeEnum.verify|verify} messages.
                 * @param message BamlFieldTypeEnum message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeEnum, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeEnum message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeEnum;

                /**
                 * Decodes a BamlFieldTypeEnum message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeEnum;

                /**
                 * Verifies a BamlFieldTypeEnum message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeEnum message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeEnum
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeEnum;

                /**
                 * Creates a plain object from a BamlFieldTypeEnum message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeEnum
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeEnum, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeEnum to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeEnum
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeClass. */
            interface IBamlFieldTypeClass {

                /** BamlFieldTypeClass name */
                name?: (baml.cffi.v1.IBamlTypeName|null);
            }

            /** Represents a BamlFieldTypeClass. */
            class BamlFieldTypeClass implements IBamlFieldTypeClass {

                /**
                 * Constructs a new BamlFieldTypeClass.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeClass);

                /** BamlFieldTypeClass name. */
                public name?: (baml.cffi.v1.IBamlTypeName|null);

                /**
                 * Creates a new BamlFieldTypeClass instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeClass instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeClass): baml.cffi.v1.BamlFieldTypeClass;

                /**
                 * Encodes the specified BamlFieldTypeClass message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeClass.verify|verify} messages.
                 * @param message BamlFieldTypeClass message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeClass, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeClass message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeClass.verify|verify} messages.
                 * @param message BamlFieldTypeClass message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeClass, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeClass message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeClass;

                /**
                 * Decodes a BamlFieldTypeClass message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeClass;

                /**
                 * Verifies a BamlFieldTypeClass message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeClass message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeClass
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeClass;

                /**
                 * Creates a plain object from a BamlFieldTypeClass message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeClass
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeClass, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeClass to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeClass
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeTypeAlias. */
            interface IBamlFieldTypeTypeAlias {

                /** BamlFieldTypeTypeAlias name */
                name?: (baml.cffi.v1.IBamlTypeName|null);
            }

            /** Represents a BamlFieldTypeTypeAlias. */
            class BamlFieldTypeTypeAlias implements IBamlFieldTypeTypeAlias {

                /**
                 * Constructs a new BamlFieldTypeTypeAlias.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeTypeAlias);

                /** BamlFieldTypeTypeAlias name. */
                public name?: (baml.cffi.v1.IBamlTypeName|null);

                /**
                 * Creates a new BamlFieldTypeTypeAlias instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeTypeAlias instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeTypeAlias): baml.cffi.v1.BamlFieldTypeTypeAlias;

                /**
                 * Encodes the specified BamlFieldTypeTypeAlias message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeTypeAlias.verify|verify} messages.
                 * @param message BamlFieldTypeTypeAlias message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeTypeAlias, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeTypeAlias message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeTypeAlias.verify|verify} messages.
                 * @param message BamlFieldTypeTypeAlias message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeTypeAlias, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeTypeAlias message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeTypeAlias
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeTypeAlias;

                /**
                 * Decodes a BamlFieldTypeTypeAlias message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeTypeAlias
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeTypeAlias;

                /**
                 * Verifies a BamlFieldTypeTypeAlias message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeTypeAlias message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeTypeAlias
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeTypeAlias;

                /**
                 * Creates a plain object from a BamlFieldTypeTypeAlias message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeTypeAlias
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeTypeAlias, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeTypeAlias to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeTypeAlias
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeList. */
            interface IBamlFieldTypeList {

                /** BamlFieldTypeList itemType */
                itemType?: (baml.cffi.v1.IBamlFieldType|null);
            }

            /** Represents a BamlFieldTypeList. */
            class BamlFieldTypeList implements IBamlFieldTypeList {

                /**
                 * Constructs a new BamlFieldTypeList.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeList);

                /** BamlFieldTypeList itemType. */
                public itemType?: (baml.cffi.v1.IBamlFieldType|null);

                /**
                 * Creates a new BamlFieldTypeList instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeList instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeList): baml.cffi.v1.BamlFieldTypeList;

                /**
                 * Encodes the specified BamlFieldTypeList message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeList.verify|verify} messages.
                 * @param message BamlFieldTypeList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeList, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeList message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeList.verify|verify} messages.
                 * @param message BamlFieldTypeList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeList, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeList message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeList;

                /**
                 * Decodes a BamlFieldTypeList message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeList;

                /**
                 * Verifies a BamlFieldTypeList message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeList message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeList
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeList;

                /**
                 * Creates a plain object from a BamlFieldTypeList message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeList
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeList, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeList to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeList
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeMap. */
            interface IBamlFieldTypeMap {

                /** BamlFieldTypeMap keyType */
                keyType?: (baml.cffi.v1.IBamlFieldType|null);

                /** BamlFieldTypeMap valueType */
                valueType?: (baml.cffi.v1.IBamlFieldType|null);
            }

            /** Represents a BamlFieldTypeMap. */
            class BamlFieldTypeMap implements IBamlFieldTypeMap {

                /**
                 * Constructs a new BamlFieldTypeMap.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeMap);

                /** BamlFieldTypeMap keyType. */
                public keyType?: (baml.cffi.v1.IBamlFieldType|null);

                /** BamlFieldTypeMap valueType. */
                public valueType?: (baml.cffi.v1.IBamlFieldType|null);

                /**
                 * Creates a new BamlFieldTypeMap instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeMap instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeMap): baml.cffi.v1.BamlFieldTypeMap;

                /**
                 * Encodes the specified BamlFieldTypeMap message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeMap.verify|verify} messages.
                 * @param message BamlFieldTypeMap message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeMap, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeMap message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeMap.verify|verify} messages.
                 * @param message BamlFieldTypeMap message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeMap, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeMap message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeMap;

                /**
                 * Decodes a BamlFieldTypeMap message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeMap;

                /**
                 * Verifies a BamlFieldTypeMap message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeMap message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeMap
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeMap;

                /**
                 * Creates a plain object from a BamlFieldTypeMap message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeMap
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeMap, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeMap to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeMap
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeUnionVariant. */
            interface IBamlFieldTypeUnionVariant {

                /** BamlFieldTypeUnionVariant name */
                name?: (baml.cffi.v1.IBamlTypeName|null);
            }

            /** Represents a BamlFieldTypeUnionVariant. */
            class BamlFieldTypeUnionVariant implements IBamlFieldTypeUnionVariant {

                /**
                 * Constructs a new BamlFieldTypeUnionVariant.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeUnionVariant);

                /** BamlFieldTypeUnionVariant name. */
                public name?: (baml.cffi.v1.IBamlTypeName|null);

                /**
                 * Creates a new BamlFieldTypeUnionVariant instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeUnionVariant instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeUnionVariant): baml.cffi.v1.BamlFieldTypeUnionVariant;

                /**
                 * Encodes the specified BamlFieldTypeUnionVariant message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeUnionVariant.verify|verify} messages.
                 * @param message BamlFieldTypeUnionVariant message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeUnionVariant, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeUnionVariant message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeUnionVariant.verify|verify} messages.
                 * @param message BamlFieldTypeUnionVariant message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeUnionVariant, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeUnionVariant message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeUnionVariant;

                /**
                 * Decodes a BamlFieldTypeUnionVariant message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeUnionVariant;

                /**
                 * Verifies a BamlFieldTypeUnionVariant message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeUnionVariant message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeUnionVariant
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeUnionVariant;

                /**
                 * Creates a plain object from a BamlFieldTypeUnionVariant message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeUnionVariant
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeUnionVariant, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeUnionVariant to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeUnionVariant
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeOptional. */
            interface IBamlFieldTypeOptional {

                /** BamlFieldTypeOptional value */
                value?: (baml.cffi.v1.IBamlFieldType|null);
            }

            /** Represents a BamlFieldTypeOptional. */
            class BamlFieldTypeOptional implements IBamlFieldTypeOptional {

                /**
                 * Constructs a new BamlFieldTypeOptional.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeOptional);

                /** BamlFieldTypeOptional value. */
                public value?: (baml.cffi.v1.IBamlFieldType|null);

                /**
                 * Creates a new BamlFieldTypeOptional instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeOptional instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeOptional): baml.cffi.v1.BamlFieldTypeOptional;

                /**
                 * Encodes the specified BamlFieldTypeOptional message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeOptional.verify|verify} messages.
                 * @param message BamlFieldTypeOptional message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeOptional, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeOptional message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeOptional.verify|verify} messages.
                 * @param message BamlFieldTypeOptional message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeOptional, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeOptional message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeOptional
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeOptional;

                /**
                 * Decodes a BamlFieldTypeOptional message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeOptional
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeOptional;

                /**
                 * Verifies a BamlFieldTypeOptional message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeOptional message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeOptional
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeOptional;

                /**
                 * Creates a plain object from a BamlFieldTypeOptional message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeOptional
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeOptional, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeOptional to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeOptional
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeChecked. */
            interface IBamlFieldTypeChecked {

                /** BamlFieldTypeChecked value */
                value?: (baml.cffi.v1.IBamlFieldType|null);

                /** BamlFieldTypeChecked checks */
                checks?: (baml.cffi.v1.IBamlCheckType[]|null);
            }

            /** Represents a BamlFieldTypeChecked. */
            class BamlFieldTypeChecked implements IBamlFieldTypeChecked {

                /**
                 * Constructs a new BamlFieldTypeChecked.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeChecked);

                /** BamlFieldTypeChecked value. */
                public value?: (baml.cffi.v1.IBamlFieldType|null);

                /** BamlFieldTypeChecked checks. */
                public checks: baml.cffi.v1.IBamlCheckType[];

                /**
                 * Creates a new BamlFieldTypeChecked instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeChecked instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeChecked): baml.cffi.v1.BamlFieldTypeChecked;

                /**
                 * Encodes the specified BamlFieldTypeChecked message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeChecked.verify|verify} messages.
                 * @param message BamlFieldTypeChecked message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeChecked, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeChecked message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeChecked.verify|verify} messages.
                 * @param message BamlFieldTypeChecked message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeChecked, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeChecked message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeChecked
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeChecked;

                /**
                 * Decodes a BamlFieldTypeChecked message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeChecked
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeChecked;

                /**
                 * Verifies a BamlFieldTypeChecked message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeChecked message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeChecked
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeChecked;

                /**
                 * Creates a plain object from a BamlFieldTypeChecked message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeChecked
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeChecked, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeChecked to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeChecked
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlFieldTypeStreamState. */
            interface IBamlFieldTypeStreamState {

                /** BamlFieldTypeStreamState value */
                value?: (baml.cffi.v1.IBamlFieldType|null);
            }

            /** Represents a BamlFieldTypeStreamState. */
            class BamlFieldTypeStreamState implements IBamlFieldTypeStreamState {

                /**
                 * Constructs a new BamlFieldTypeStreamState.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlFieldTypeStreamState);

                /** BamlFieldTypeStreamState value. */
                public value?: (baml.cffi.v1.IBamlFieldType|null);

                /**
                 * Creates a new BamlFieldTypeStreamState instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlFieldTypeStreamState instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlFieldTypeStreamState): baml.cffi.v1.BamlFieldTypeStreamState;

                /**
                 * Encodes the specified BamlFieldTypeStreamState message. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeStreamState.verify|verify} messages.
                 * @param message BamlFieldTypeStreamState message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlFieldTypeStreamState, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlFieldTypeStreamState message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlFieldTypeStreamState.verify|verify} messages.
                 * @param message BamlFieldTypeStreamState message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlFieldTypeStreamState, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlFieldTypeStreamState message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlFieldTypeStreamState
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlFieldTypeStreamState;

                /**
                 * Decodes a BamlFieldTypeStreamState message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlFieldTypeStreamState
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlFieldTypeStreamState;

                /**
                 * Verifies a BamlFieldTypeStreamState message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlFieldTypeStreamState message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlFieldTypeStreamState
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlFieldTypeStreamState;

                /**
                 * Creates a plain object from a BamlFieldTypeStreamState message. Also converts values to other types if specified.
                 * @param message BamlFieldTypeStreamState
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlFieldTypeStreamState, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlFieldTypeStreamState to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlFieldTypeStreamState
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlCheckType. */
            interface IBamlCheckType {

                /** BamlCheckType name */
                name?: (string|null);
            }

            /** Represents a BamlCheckType. */
            class BamlCheckType implements IBamlCheckType {

                /**
                 * Constructs a new BamlCheckType.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlCheckType);

                /** BamlCheckType name. */
                public name: string;

                /**
                 * Creates a new BamlCheckType instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlCheckType instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlCheckType): baml.cffi.v1.BamlCheckType;

                /**
                 * Encodes the specified BamlCheckType message. Does not implicitly {@link baml.cffi.v1.BamlCheckType.verify|verify} messages.
                 * @param message BamlCheckType message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlCheckType, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlCheckType message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlCheckType.verify|verify} messages.
                 * @param message BamlCheckType message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlCheckType, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlCheckType message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlCheckType
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlCheckType;

                /**
                 * Decodes a BamlCheckType message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlCheckType
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlCheckType;

                /**
                 * Verifies a BamlCheckType message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlCheckType message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlCheckType
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlCheckType;

                /**
                 * Creates a plain object from a BamlCheckType message. Also converts values to other types if specified.
                 * @param message BamlCheckType
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlCheckType, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlCheckType to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlCheckType
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlCheckValue. */
            interface IBamlCheckValue {

                /** BamlCheckValue name */
                name?: (string|null);

                /** BamlCheckValue expression */
                expression?: (string|null);

                /** BamlCheckValue status */
                status?: (string|null);

                /** BamlCheckValue value */
                value?: (baml.cffi.v1.IBamlOutboundValue|null);
            }

            /** Represents a BamlCheckValue. */
            class BamlCheckValue implements IBamlCheckValue {

                /**
                 * Constructs a new BamlCheckValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlCheckValue);

                /** BamlCheckValue name. */
                public name: string;

                /** BamlCheckValue expression. */
                public expression: string;

                /** BamlCheckValue status. */
                public status: string;

                /** BamlCheckValue value. */
                public value?: (baml.cffi.v1.IBamlOutboundValue|null);

                /**
                 * Creates a new BamlCheckValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlCheckValue instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlCheckValue): baml.cffi.v1.BamlCheckValue;

                /**
                 * Encodes the specified BamlCheckValue message. Does not implicitly {@link baml.cffi.v1.BamlCheckValue.verify|verify} messages.
                 * @param message BamlCheckValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlCheckValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlCheckValue message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlCheckValue.verify|verify} messages.
                 * @param message BamlCheckValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlCheckValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlCheckValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlCheckValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlCheckValue;

                /**
                 * Decodes a BamlCheckValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlCheckValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlCheckValue;

                /**
                 * Verifies a BamlCheckValue message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlCheckValue message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlCheckValue
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlCheckValue;

                /**
                 * Creates a plain object from a BamlCheckValue message. Also converts values to other types if specified.
                 * @param message BamlCheckValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlCheckValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlCheckValue to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlCheckValue
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** BamlStreamState enum. */
            enum BamlStreamState {
                PENDING = 0,
                STARTED = 1,
                DONE = 2
            }

            /** Properties of a BamlValueStreamingState. */
            interface IBamlValueStreamingState {

                /** BamlValueStreamingState value */
                value?: (baml.cffi.v1.IBamlOutboundValue|null);

                /** BamlValueStreamingState state */
                state?: (baml.cffi.v1.BamlStreamState|null);

                /** BamlValueStreamingState name */
                name?: (baml.cffi.v1.IBamlTypeName|null);
            }

            /** Represents a BamlValueStreamingState. */
            class BamlValueStreamingState implements IBamlValueStreamingState {

                /**
                 * Constructs a new BamlValueStreamingState.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml.cffi.v1.IBamlValueStreamingState);

                /** BamlValueStreamingState value. */
                public value?: (baml.cffi.v1.IBamlOutboundValue|null);

                /** BamlValueStreamingState state. */
                public state: baml.cffi.v1.BamlStreamState;

                /** BamlValueStreamingState name. */
                public name?: (baml.cffi.v1.IBamlTypeName|null);

                /**
                 * Creates a new BamlValueStreamingState instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueStreamingState instance
                 */
                public static create(properties?: baml.cffi.v1.IBamlValueStreamingState): baml.cffi.v1.BamlValueStreamingState;

                /**
                 * Encodes the specified BamlValueStreamingState message. Does not implicitly {@link baml.cffi.v1.BamlValueStreamingState.verify|verify} messages.
                 * @param message BamlValueStreamingState message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml.cffi.v1.IBamlValueStreamingState, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueStreamingState message, length delimited. Does not implicitly {@link baml.cffi.v1.BamlValueStreamingState.verify|verify} messages.
                 * @param message BamlValueStreamingState message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml.cffi.v1.IBamlValueStreamingState, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueStreamingState message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueStreamingState
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml.cffi.v1.BamlValueStreamingState;

                /**
                 * Decodes a BamlValueStreamingState message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueStreamingState
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml.cffi.v1.BamlValueStreamingState;

                /**
                 * Verifies a BamlValueStreamingState message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlValueStreamingState message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlValueStreamingState
                 */
                public static fromObject(object: { [k: string]: any }): baml.cffi.v1.BamlValueStreamingState;

                /**
                 * Creates a plain object from a BamlValueStreamingState message. Also converts values to other types if specified.
                 * @param message BamlValueStreamingState
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml.cffi.v1.BamlValueStreamingState, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlValueStreamingState to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlValueStreamingState
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }
        }
    }
}
