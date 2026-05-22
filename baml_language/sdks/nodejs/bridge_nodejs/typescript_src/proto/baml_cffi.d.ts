/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
import * as $protobuf from "protobufjs";
import Long = require("long");
/** Namespace baml_core. */
export namespace baml_core {

    /** Namespace cffi. */
    namespace cffi {

        /** Namespace v1. */
        namespace v1 {

            /** BamlHandleType enum. */
            enum BamlHandleType {
                HANDLE_UNSPECIFIED = 0,
                UNTAGGED_RUST_DATA = 1,
                UNTAGGED_BEX_HEAP = 2,
                FUNCTION_REF = 5,
                ADT_MEDIA_IMAGE = 6,
                ADT_MEDIA_AUDIO = 7,
                ADT_MEDIA_VIDEO = 8,
                ADT_MEDIA_PDF = 9,
                ADT_MEDIA_GENERIC = 10,
                ADT_PROMPT_AST = 11,
                ADT_COLLECTOR = 12,
                ADT_TYPE = 13,
                ADT_TAGGED_HEAP_HANDLE = 14
            }

            /** Properties of a BamlHandle. */
            interface IBamlHandle {

                /** BamlHandle key */
                key?: (number|Long|null);

                /** BamlHandle handleType */
                handleType?: (baml_core.cffi.v1.BamlHandleType|null);
            }

            /** Represents a BamlHandle. */
            class BamlHandle implements IBamlHandle {

                /**
                 * Constructs a new BamlHandle.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlHandle);

                /** BamlHandle key. */
                public key: (number|Long);

                /** BamlHandle handleType. */
                public handleType: baml_core.cffi.v1.BamlHandleType;

                /**
                 * Creates a new BamlHandle instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlHandle instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlHandle): baml_core.cffi.v1.BamlHandle;

                /**
                 * Encodes the specified BamlHandle message. Does not implicitly {@link baml_core.cffi.v1.BamlHandle.verify|verify} messages.
                 * @param message BamlHandle message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlHandle, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlHandle message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlHandle.verify|verify} messages.
                 * @param message BamlHandle message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlHandle, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlHandle message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlHandle;

                /**
                 * Decodes a BamlHandle message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlHandle;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlHandle;

                /**
                 * Creates a plain object from a BamlHandle message. Also converts values to other types if specified.
                 * @param message BamlHandle
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlHandle, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                listValue?: (baml_core.cffi.v1.IInboundListValue|null);

                /** InboundValue mapValue */
                mapValue?: (baml_core.cffi.v1.IInboundMapValue|null);

                /** InboundValue classValue */
                classValue?: (baml_core.cffi.v1.IInboundClassValue|null);

                /** InboundValue enumValue */
                enumValue?: (baml_core.cffi.v1.IInboundEnumValue|null);

                /** InboundValue handle */
                handle?: (baml_core.cffi.v1.IBamlHandle|null);

                /** InboundValue uint8arrayValue */
                uint8arrayValue?: (Uint8Array|null);
            }

            /** Represents an InboundValue. */
            class InboundValue implements IInboundValue {

                /**
                 * Constructs a new InboundValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IInboundValue);

                /** InboundValue stringValue. */
                public stringValue?: (string|null);

                /** InboundValue intValue. */
                public intValue?: (number|Long|null);

                /** InboundValue floatValue. */
                public floatValue?: (number|null);

                /** InboundValue boolValue. */
                public boolValue?: (boolean|null);

                /** InboundValue listValue. */
                public listValue?: (baml_core.cffi.v1.IInboundListValue|null);

                /** InboundValue mapValue. */
                public mapValue?: (baml_core.cffi.v1.IInboundMapValue|null);

                /** InboundValue classValue. */
                public classValue?: (baml_core.cffi.v1.IInboundClassValue|null);

                /** InboundValue enumValue. */
                public enumValue?: (baml_core.cffi.v1.IInboundEnumValue|null);

                /** InboundValue handle. */
                public handle?: (baml_core.cffi.v1.IBamlHandle|null);

                /** InboundValue uint8arrayValue. */
                public uint8arrayValue?: (Uint8Array|null);

                /** InboundValue value. */
                public value?: ("stringValue"|"intValue"|"floatValue"|"boolValue"|"listValue"|"mapValue"|"classValue"|"enumValue"|"handle"|"uint8arrayValue");

                /**
                 * Creates a new InboundValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundValue instance
                 */
                public static create(properties?: baml_core.cffi.v1.IInboundValue): baml_core.cffi.v1.InboundValue;

                /**
                 * Encodes the specified InboundValue message. Does not implicitly {@link baml_core.cffi.v1.InboundValue.verify|verify} messages.
                 * @param message InboundValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IInboundValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundValue message, length delimited. Does not implicitly {@link baml_core.cffi.v1.InboundValue.verify|verify} messages.
                 * @param message InboundValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IInboundValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.InboundValue;

                /**
                 * Decodes an InboundValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.InboundValue;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.InboundValue;

                /**
                 * Creates a plain object from an InboundValue message. Also converts values to other types if specified.
                 * @param message InboundValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.InboundValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                values?: (baml_core.cffi.v1.IInboundValue[]|null);
            }

            /** Represents an InboundListValue. */
            class InboundListValue implements IInboundListValue {

                /**
                 * Constructs a new InboundListValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IInboundListValue);

                /** InboundListValue values. */
                public values: baml_core.cffi.v1.IInboundValue[];

                /**
                 * Creates a new InboundListValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundListValue instance
                 */
                public static create(properties?: baml_core.cffi.v1.IInboundListValue): baml_core.cffi.v1.InboundListValue;

                /**
                 * Encodes the specified InboundListValue message. Does not implicitly {@link baml_core.cffi.v1.InboundListValue.verify|verify} messages.
                 * @param message InboundListValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IInboundListValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundListValue message, length delimited. Does not implicitly {@link baml_core.cffi.v1.InboundListValue.verify|verify} messages.
                 * @param message InboundListValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IInboundListValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundListValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundListValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.InboundListValue;

                /**
                 * Decodes an InboundListValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundListValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.InboundListValue;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.InboundListValue;

                /**
                 * Creates a plain object from an InboundListValue message. Also converts values to other types if specified.
                 * @param message InboundListValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.InboundListValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                entries?: (baml_core.cffi.v1.IInboundMapEntry[]|null);
            }

            /** Represents an InboundMapValue. */
            class InboundMapValue implements IInboundMapValue {

                /**
                 * Constructs a new InboundMapValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IInboundMapValue);

                /** InboundMapValue entries. */
                public entries: baml_core.cffi.v1.IInboundMapEntry[];

                /**
                 * Creates a new InboundMapValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundMapValue instance
                 */
                public static create(properties?: baml_core.cffi.v1.IInboundMapValue): baml_core.cffi.v1.InboundMapValue;

                /**
                 * Encodes the specified InboundMapValue message. Does not implicitly {@link baml_core.cffi.v1.InboundMapValue.verify|verify} messages.
                 * @param message InboundMapValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IInboundMapValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundMapValue message, length delimited. Does not implicitly {@link baml_core.cffi.v1.InboundMapValue.verify|verify} messages.
                 * @param message InboundMapValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IInboundMapValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundMapValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundMapValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.InboundMapValue;

                /**
                 * Decodes an InboundMapValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundMapValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.InboundMapValue;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.InboundMapValue;

                /**
                 * Creates a plain object from an InboundMapValue message. Also converts values to other types if specified.
                 * @param message InboundMapValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.InboundMapValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                enumKey?: (baml_core.cffi.v1.IInboundEnumValue|null);

                /** InboundMapEntry value */
                value?: (baml_core.cffi.v1.IInboundValue|null);
            }

            /** Represents an InboundMapEntry. */
            class InboundMapEntry implements IInboundMapEntry {

                /**
                 * Constructs a new InboundMapEntry.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IInboundMapEntry);

                /** InboundMapEntry stringKey. */
                public stringKey?: (string|null);

                /** InboundMapEntry intKey. */
                public intKey?: (number|Long|null);

                /** InboundMapEntry boolKey. */
                public boolKey?: (boolean|null);

                /** InboundMapEntry enumKey. */
                public enumKey?: (baml_core.cffi.v1.IInboundEnumValue|null);

                /** InboundMapEntry value. */
                public value?: (baml_core.cffi.v1.IInboundValue|null);

                /** InboundMapEntry key. */
                public key?: ("stringKey"|"intKey"|"boolKey"|"enumKey");

                /**
                 * Creates a new InboundMapEntry instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundMapEntry instance
                 */
                public static create(properties?: baml_core.cffi.v1.IInboundMapEntry): baml_core.cffi.v1.InboundMapEntry;

                /**
                 * Encodes the specified InboundMapEntry message. Does not implicitly {@link baml_core.cffi.v1.InboundMapEntry.verify|verify} messages.
                 * @param message InboundMapEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IInboundMapEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundMapEntry message, length delimited. Does not implicitly {@link baml_core.cffi.v1.InboundMapEntry.verify|verify} messages.
                 * @param message InboundMapEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IInboundMapEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundMapEntry message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.InboundMapEntry;

                /**
                 * Decodes an InboundMapEntry message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.InboundMapEntry;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.InboundMapEntry;

                /**
                 * Creates a plain object from an InboundMapEntry message. Also converts values to other types if specified.
                 * @param message InboundMapEntry
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.InboundMapEntry, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                fields?: (baml_core.cffi.v1.IInboundMapEntry[]|null);
            }

            /** Represents an InboundClassValue. */
            class InboundClassValue implements IInboundClassValue {

                /**
                 * Constructs a new InboundClassValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IInboundClassValue);

                /** InboundClassValue name. */
                public name: string;

                /** InboundClassValue fields. */
                public fields: baml_core.cffi.v1.IInboundMapEntry[];

                /**
                 * Creates a new InboundClassValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundClassValue instance
                 */
                public static create(properties?: baml_core.cffi.v1.IInboundClassValue): baml_core.cffi.v1.InboundClassValue;

                /**
                 * Encodes the specified InboundClassValue message. Does not implicitly {@link baml_core.cffi.v1.InboundClassValue.verify|verify} messages.
                 * @param message InboundClassValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IInboundClassValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundClassValue message, length delimited. Does not implicitly {@link baml_core.cffi.v1.InboundClassValue.verify|verify} messages.
                 * @param message InboundClassValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IInboundClassValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundClassValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundClassValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.InboundClassValue;

                /**
                 * Decodes an InboundClassValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundClassValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.InboundClassValue;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.InboundClassValue;

                /**
                 * Creates a plain object from an InboundClassValue message. Also converts values to other types if specified.
                 * @param message InboundClassValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.InboundClassValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                constructor(properties?: baml_core.cffi.v1.IInboundEnumValue);

                /** InboundEnumValue name. */
                public name: string;

                /** InboundEnumValue value. */
                public value: string;

                /**
                 * Creates a new InboundEnumValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundEnumValue instance
                 */
                public static create(properties?: baml_core.cffi.v1.IInboundEnumValue): baml_core.cffi.v1.InboundEnumValue;

                /**
                 * Encodes the specified InboundEnumValue message. Does not implicitly {@link baml_core.cffi.v1.InboundEnumValue.verify|verify} messages.
                 * @param message InboundEnumValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IInboundEnumValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundEnumValue message, length delimited. Does not implicitly {@link baml_core.cffi.v1.InboundEnumValue.verify|verify} messages.
                 * @param message InboundEnumValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IInboundEnumValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundEnumValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundEnumValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.InboundEnumValue;

                /**
                 * Decodes an InboundEnumValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundEnumValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.InboundEnumValue;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.InboundEnumValue;

                /**
                 * Creates a plain object from an InboundEnumValue message. Also converts values to other types if specified.
                 * @param message InboundEnumValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.InboundEnumValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                kwargs?: (baml_core.cffi.v1.IInboundMapEntry[]|null);
            }

            /** Represents a CallFunctionArgs. */
            class CallFunctionArgs implements ICallFunctionArgs {

                /**
                 * Constructs a new CallFunctionArgs.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.ICallFunctionArgs);

                /** CallFunctionArgs kwargs. */
                public kwargs: baml_core.cffi.v1.IInboundMapEntry[];

                /**
                 * Creates a new CallFunctionArgs instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns CallFunctionArgs instance
                 */
                public static create(properties?: baml_core.cffi.v1.ICallFunctionArgs): baml_core.cffi.v1.CallFunctionArgs;

                /**
                 * Encodes the specified CallFunctionArgs message. Does not implicitly {@link baml_core.cffi.v1.CallFunctionArgs.verify|verify} messages.
                 * @param message CallFunctionArgs message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.ICallFunctionArgs, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified CallFunctionArgs message, length delimited. Does not implicitly {@link baml_core.cffi.v1.CallFunctionArgs.verify|verify} messages.
                 * @param message CallFunctionArgs message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.ICallFunctionArgs, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CallFunctionArgs message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns CallFunctionArgs
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.CallFunctionArgs;

                /**
                 * Decodes a CallFunctionArgs message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns CallFunctionArgs
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.CallFunctionArgs;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.CallFunctionArgs;

                /**
                 * Creates a plain object from a CallFunctionArgs message. Also converts values to other types if specified.
                 * @param message CallFunctionArgs
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.CallFunctionArgs, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                constructor(properties?: baml_core.cffi.v1.ICallAck);

                /** CallAck error. */
                public error?: (string|null);

                /** CallAck response. */
                public response?: "error";

                /**
                 * Creates a new CallAck instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns CallAck instance
                 */
                public static create(properties?: baml_core.cffi.v1.ICallAck): baml_core.cffi.v1.CallAck;

                /**
                 * Encodes the specified CallAck message. Does not implicitly {@link baml_core.cffi.v1.CallAck.verify|verify} messages.
                 * @param message CallAck message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.ICallAck, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified CallAck message, length delimited. Does not implicitly {@link baml_core.cffi.v1.CallAck.verify|verify} messages.
                 * @param message CallAck message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.ICallAck, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CallAck message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns CallAck
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.CallAck;

                /**
                 * Decodes a CallAck message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns CallAck
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.CallAck;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.CallAck;

                /**
                 * Creates a plain object from a CallAck message. Also converts values to other types if specified.
                 * @param message CallAck
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.CallAck, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                nullValue?: (baml_core.cffi.v1.IBamlValueNull|null);

                /** BamlOutboundValue stringValue */
                stringValue?: (string|null);

                /** BamlOutboundValue intValue */
                intValue?: (number|Long|null);

                /** BamlOutboundValue floatValue */
                floatValue?: (number|null);

                /** BamlOutboundValue boolValue */
                boolValue?: (boolean|null);

                /** BamlOutboundValue classValue */
                classValue?: (baml_core.cffi.v1.IBamlValueClass|null);

                /** BamlOutboundValue enumValue */
                enumValue?: (baml_core.cffi.v1.IBamlValueEnum|null);

                /** BamlOutboundValue literalValue */
                literalValue?: (baml_core.cffi.v1.IBamlTyLiteral|null);

                /** BamlOutboundValue listValue */
                listValue?: (baml_core.cffi.v1.IBamlValueList|null);

                /** BamlOutboundValue mapValue */
                mapValue?: (baml_core.cffi.v1.IBamlValueMap|null);

                /** BamlOutboundValue unionVariantValue */
                unionVariantValue?: (baml_core.cffi.v1.IBamlValueUnionVariant|null);

                /** BamlOutboundValue handleValue */
                handleValue?: (baml_core.cffi.v1.IBamlOutboundHandle|null);

                /** BamlOutboundValue mediaValue */
                mediaValue?: (baml_core.cffi.v1.IBamlValueMedia|null);

                /** BamlOutboundValue promptAstValue */
                promptAstValue?: (baml_core.cffi.v1.IBamlValuePromptAst|null);

                /** BamlOutboundValue uint8arrayValue */
                uint8arrayValue?: (Uint8Array|null);
            }

            /** Represents a BamlOutboundValue. */
            class BamlOutboundValue implements IBamlOutboundValue {

                /**
                 * Constructs a new BamlOutboundValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlOutboundValue);

                /** BamlOutboundValue nullValue. */
                public nullValue?: (baml_core.cffi.v1.IBamlValueNull|null);

                /** BamlOutboundValue stringValue. */
                public stringValue?: (string|null);

                /** BamlOutboundValue intValue. */
                public intValue?: (number|Long|null);

                /** BamlOutboundValue floatValue. */
                public floatValue?: (number|null);

                /** BamlOutboundValue boolValue. */
                public boolValue?: (boolean|null);

                /** BamlOutboundValue classValue. */
                public classValue?: (baml_core.cffi.v1.IBamlValueClass|null);

                /** BamlOutboundValue enumValue. */
                public enumValue?: (baml_core.cffi.v1.IBamlValueEnum|null);

                /** BamlOutboundValue literalValue. */
                public literalValue?: (baml_core.cffi.v1.IBamlTyLiteral|null);

                /** BamlOutboundValue listValue. */
                public listValue?: (baml_core.cffi.v1.IBamlValueList|null);

                /** BamlOutboundValue mapValue. */
                public mapValue?: (baml_core.cffi.v1.IBamlValueMap|null);

                /** BamlOutboundValue unionVariantValue. */
                public unionVariantValue?: (baml_core.cffi.v1.IBamlValueUnionVariant|null);

                /** BamlOutboundValue handleValue. */
                public handleValue?: (baml_core.cffi.v1.IBamlOutboundHandle|null);

                /** BamlOutboundValue mediaValue. */
                public mediaValue?: (baml_core.cffi.v1.IBamlValueMedia|null);

                /** BamlOutboundValue promptAstValue. */
                public promptAstValue?: (baml_core.cffi.v1.IBamlValuePromptAst|null);

                /** BamlOutboundValue uint8arrayValue. */
                public uint8arrayValue?: (Uint8Array|null);

                /** BamlOutboundValue value. */
                public value?: ("nullValue"|"stringValue"|"intValue"|"floatValue"|"boolValue"|"classValue"|"enumValue"|"literalValue"|"listValue"|"mapValue"|"unionVariantValue"|"handleValue"|"mediaValue"|"promptAstValue"|"uint8arrayValue");

                /**
                 * Creates a new BamlOutboundValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlOutboundValue instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlOutboundValue): baml_core.cffi.v1.BamlOutboundValue;

                /**
                 * Encodes the specified BamlOutboundValue message. Does not implicitly {@link baml_core.cffi.v1.BamlOutboundValue.verify|verify} messages.
                 * @param message BamlOutboundValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlOutboundValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlOutboundValue message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlOutboundValue.verify|verify} messages.
                 * @param message BamlOutboundValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlOutboundValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlOutboundValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlOutboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlOutboundValue;

                /**
                 * Decodes a BamlOutboundValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlOutboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlOutboundValue;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlOutboundValue;

                /**
                 * Creates a plain object from a BamlOutboundValue message. Also converts values to other types if specified.
                 * @param message BamlOutboundValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlOutboundValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** Properties of a BamlOutboundHandle. */
            interface IBamlOutboundHandle {

                /** BamlOutboundHandle key */
                key?: (number|Long|null);

                /** BamlOutboundHandle handleType */
                handleType?: (baml_core.cffi.v1.BamlHandleType|null);

                /** BamlOutboundHandle name */
                name?: (baml_core.cffi.v1.IBamlTyName|null);
            }

            /** Represents a BamlOutboundHandle. */
            class BamlOutboundHandle implements IBamlOutboundHandle {

                /**
                 * Constructs a new BamlOutboundHandle.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlOutboundHandle);

                /** BamlOutboundHandle key. */
                public key: (number|Long);

                /** BamlOutboundHandle handleType. */
                public handleType: baml_core.cffi.v1.BamlHandleType;

                /** BamlOutboundHandle name. */
                public name?: (baml_core.cffi.v1.IBamlTyName|null);

                /**
                 * Creates a new BamlOutboundHandle instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlOutboundHandle instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlOutboundHandle): baml_core.cffi.v1.BamlOutboundHandle;

                /**
                 * Encodes the specified BamlOutboundHandle message. Does not implicitly {@link baml_core.cffi.v1.BamlOutboundHandle.verify|verify} messages.
                 * @param message BamlOutboundHandle message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlOutboundHandle, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlOutboundHandle message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlOutboundHandle.verify|verify} messages.
                 * @param message BamlOutboundHandle message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlOutboundHandle, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlOutboundHandle message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlOutboundHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlOutboundHandle;

                /**
                 * Decodes a BamlOutboundHandle message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlOutboundHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlOutboundHandle;

                /**
                 * Verifies a BamlOutboundHandle message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlOutboundHandle message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlOutboundHandle
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlOutboundHandle;

                /**
                 * Creates a plain object from a BamlOutboundHandle message. Also converts values to other types if specified.
                 * @param message BamlOutboundHandle
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlOutboundHandle, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlOutboundHandle to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlOutboundHandle
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyName. */
            interface IBamlTyName {

                /** BamlTyName name */
                name?: (string|null);

                /** BamlTyName genericArgs */
                genericArgs?: (baml_core.cffi.v1.IBamlTyGenericArg[]|null);
            }

            /** Represents a BamlTyName. */
            class BamlTyName implements IBamlTyName {

                /**
                 * Constructs a new BamlTyName.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyName);

                /** BamlTyName name. */
                public name: string;

                /** BamlTyName genericArgs. */
                public genericArgs: baml_core.cffi.v1.IBamlTyGenericArg[];

                /**
                 * Creates a new BamlTyName instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyName instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyName): baml_core.cffi.v1.BamlTyName;

                /**
                 * Encodes the specified BamlTyName message. Does not implicitly {@link baml_core.cffi.v1.BamlTyName.verify|verify} messages.
                 * @param message BamlTyName message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyName, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyName message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyName.verify|verify} messages.
                 * @param message BamlTyName message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyName, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyName message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyName
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyName;

                /**
                 * Decodes a BamlTyName message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyName
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyName;

                /**
                 * Verifies a BamlTyName message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyName message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyName
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyName;

                /**
                 * Creates a plain object from a BamlTyName message. Also converts values to other types if specified.
                 * @param message BamlTyName
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyName, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyName to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyName
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyGenericArg. */
            interface IBamlTyGenericArg {

                /** BamlTyGenericArg name */
                name?: (string|null);

                /** BamlTyGenericArg ty */
                ty?: (baml_core.cffi.v1.IBamlTy|null);
            }

            /** Represents a BamlTyGenericArg. */
            class BamlTyGenericArg implements IBamlTyGenericArg {

                /**
                 * Constructs a new BamlTyGenericArg.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyGenericArg);

                /** BamlTyGenericArg name. */
                public name: string;

                /** BamlTyGenericArg ty. */
                public ty?: (baml_core.cffi.v1.IBamlTy|null);

                /**
                 * Creates a new BamlTyGenericArg instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyGenericArg instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyGenericArg): baml_core.cffi.v1.BamlTyGenericArg;

                /**
                 * Encodes the specified BamlTyGenericArg message. Does not implicitly {@link baml_core.cffi.v1.BamlTyGenericArg.verify|verify} messages.
                 * @param message BamlTyGenericArg message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyGenericArg, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyGenericArg message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyGenericArg.verify|verify} messages.
                 * @param message BamlTyGenericArg message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyGenericArg, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyGenericArg message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyGenericArg
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyGenericArg;

                /**
                 * Decodes a BamlTyGenericArg message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyGenericArg
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyGenericArg;

                /**
                 * Verifies a BamlTyGenericArg message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyGenericArg message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyGenericArg
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyGenericArg;

                /**
                 * Creates a plain object from a BamlTyGenericArg message. Also converts values to other types if specified.
                 * @param message BamlTyGenericArg
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyGenericArg, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyGenericArg to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyGenericArg
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
                constructor(properties?: baml_core.cffi.v1.IBamlValueNull);

                /**
                 * Creates a new BamlValueNull instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueNull instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlValueNull): baml_core.cffi.v1.BamlValueNull;

                /**
                 * Encodes the specified BamlValueNull message. Does not implicitly {@link baml_core.cffi.v1.BamlValueNull.verify|verify} messages.
                 * @param message BamlValueNull message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlValueNull, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueNull message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueNull.verify|verify} messages.
                 * @param message BamlValueNull message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlValueNull, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueNull message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlValueNull;

                /**
                 * Decodes a BamlValueNull message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlValueNull;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlValueNull;

                /**
                 * Creates a plain object from a BamlValueNull message. Also converts values to other types if specified.
                 * @param message BamlValueNull
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlValueNull, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                itemType?: (baml_core.cffi.v1.IBamlTy|null);

                /** BamlValueList items */
                items?: (baml_core.cffi.v1.IBamlOutboundValue[]|null);
            }

            /** Represents a BamlValueList. */
            class BamlValueList implements IBamlValueList {

                /**
                 * Constructs a new BamlValueList.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlValueList);

                /** BamlValueList itemType. */
                public itemType?: (baml_core.cffi.v1.IBamlTy|null);

                /** BamlValueList items. */
                public items: baml_core.cffi.v1.IBamlOutboundValue[];

                /**
                 * Creates a new BamlValueList instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueList instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlValueList): baml_core.cffi.v1.BamlValueList;

                /**
                 * Encodes the specified BamlValueList message. Does not implicitly {@link baml_core.cffi.v1.BamlValueList.verify|verify} messages.
                 * @param message BamlValueList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlValueList, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueList message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueList.verify|verify} messages.
                 * @param message BamlValueList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlValueList, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueList message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlValueList;

                /**
                 * Decodes a BamlValueList message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlValueList;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlValueList;

                /**
                 * Creates a plain object from a BamlValueList message. Also converts values to other types if specified.
                 * @param message BamlValueList
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlValueList, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                value?: (baml_core.cffi.v1.IBamlOutboundValue|null);
            }

            /** Represents a BamlOutboundMapEntry. */
            class BamlOutboundMapEntry implements IBamlOutboundMapEntry {

                /**
                 * Constructs a new BamlOutboundMapEntry.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlOutboundMapEntry);

                /** BamlOutboundMapEntry key. */
                public key: string;

                /** BamlOutboundMapEntry value. */
                public value?: (baml_core.cffi.v1.IBamlOutboundValue|null);

                /**
                 * Creates a new BamlOutboundMapEntry instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlOutboundMapEntry instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlOutboundMapEntry): baml_core.cffi.v1.BamlOutboundMapEntry;

                /**
                 * Encodes the specified BamlOutboundMapEntry message. Does not implicitly {@link baml_core.cffi.v1.BamlOutboundMapEntry.verify|verify} messages.
                 * @param message BamlOutboundMapEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlOutboundMapEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlOutboundMapEntry message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlOutboundMapEntry.verify|verify} messages.
                 * @param message BamlOutboundMapEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlOutboundMapEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlOutboundMapEntry message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlOutboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlOutboundMapEntry;

                /**
                 * Decodes a BamlOutboundMapEntry message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlOutboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlOutboundMapEntry;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlOutboundMapEntry;

                /**
                 * Creates a plain object from a BamlOutboundMapEntry message. Also converts values to other types if specified.
                 * @param message BamlOutboundMapEntry
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlOutboundMapEntry, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                keyType?: (baml_core.cffi.v1.IBamlTy|null);

                /** BamlValueMap valueType */
                valueType?: (baml_core.cffi.v1.IBamlTy|null);

                /** BamlValueMap entries */
                entries?: (baml_core.cffi.v1.IBamlOutboundMapEntry[]|null);
            }

            /** Represents a BamlValueMap. */
            class BamlValueMap implements IBamlValueMap {

                /**
                 * Constructs a new BamlValueMap.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlValueMap);

                /** BamlValueMap keyType. */
                public keyType?: (baml_core.cffi.v1.IBamlTy|null);

                /** BamlValueMap valueType. */
                public valueType?: (baml_core.cffi.v1.IBamlTy|null);

                /** BamlValueMap entries. */
                public entries: baml_core.cffi.v1.IBamlOutboundMapEntry[];

                /**
                 * Creates a new BamlValueMap instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueMap instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlValueMap): baml_core.cffi.v1.BamlValueMap;

                /**
                 * Encodes the specified BamlValueMap message. Does not implicitly {@link baml_core.cffi.v1.BamlValueMap.verify|verify} messages.
                 * @param message BamlValueMap message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlValueMap, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueMap message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueMap.verify|verify} messages.
                 * @param message BamlValueMap message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlValueMap, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueMap message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlValueMap;

                /**
                 * Decodes a BamlValueMap message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlValueMap;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlValueMap;

                /**
                 * Creates a plain object from a BamlValueMap message. Also converts values to other types if specified.
                 * @param message BamlValueMap
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlValueMap, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                name?: (baml_core.cffi.v1.IBamlTyName|null);

                /** BamlValueClass fields */
                fields?: (baml_core.cffi.v1.IBamlOutboundMapEntry[]|null);
            }

            /** Represents a BamlValueClass. */
            class BamlValueClass implements IBamlValueClass {

                /**
                 * Constructs a new BamlValueClass.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlValueClass);

                /** BamlValueClass name. */
                public name?: (baml_core.cffi.v1.IBamlTyName|null);

                /** BamlValueClass fields. */
                public fields: baml_core.cffi.v1.IBamlOutboundMapEntry[];

                /**
                 * Creates a new BamlValueClass instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueClass instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlValueClass): baml_core.cffi.v1.BamlValueClass;

                /**
                 * Encodes the specified BamlValueClass message. Does not implicitly {@link baml_core.cffi.v1.BamlValueClass.verify|verify} messages.
                 * @param message BamlValueClass message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlValueClass, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueClass message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueClass.verify|verify} messages.
                 * @param message BamlValueClass message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlValueClass, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueClass message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlValueClass;

                /**
                 * Decodes a BamlValueClass message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlValueClass;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlValueClass;

                /**
                 * Creates a plain object from a BamlValueClass message. Also converts values to other types if specified.
                 * @param message BamlValueClass
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlValueClass, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                name?: (baml_core.cffi.v1.IBamlTyName|null);

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
                constructor(properties?: baml_core.cffi.v1.IBamlValueEnum);

                /** BamlValueEnum name. */
                public name?: (baml_core.cffi.v1.IBamlTyName|null);

                /** BamlValueEnum value. */
                public value: string;

                /** BamlValueEnum isDynamic. */
                public isDynamic: boolean;

                /**
                 * Creates a new BamlValueEnum instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueEnum instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlValueEnum): baml_core.cffi.v1.BamlValueEnum;

                /**
                 * Encodes the specified BamlValueEnum message. Does not implicitly {@link baml_core.cffi.v1.BamlValueEnum.verify|verify} messages.
                 * @param message BamlValueEnum message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlValueEnum, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueEnum message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueEnum.verify|verify} messages.
                 * @param message BamlValueEnum message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlValueEnum, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueEnum message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlValueEnum;

                /**
                 * Decodes a BamlValueEnum message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlValueEnum;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlValueEnum;

                /**
                 * Creates a plain object from a BamlValueEnum message. Also converts values to other types if specified.
                 * @param message BamlValueEnum
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlValueEnum, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                name?: (baml_core.cffi.v1.IBamlTyName|null);

                /** BamlValueUnionVariant isOptional */
                isOptional?: (boolean|null);

                /** BamlValueUnionVariant isSinglePattern */
                isSinglePattern?: (boolean|null);

                /** BamlValueUnionVariant selfType */
                selfType?: (baml_core.cffi.v1.IBamlTy|null);

                /** BamlValueUnionVariant valueOptionName */
                valueOptionName?: (string|null);

                /** BamlValueUnionVariant value */
                value?: (baml_core.cffi.v1.IBamlOutboundValue|null);
            }

            /** Represents a BamlValueUnionVariant. */
            class BamlValueUnionVariant implements IBamlValueUnionVariant {

                /**
                 * Constructs a new BamlValueUnionVariant.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlValueUnionVariant);

                /** BamlValueUnionVariant name. */
                public name?: (baml_core.cffi.v1.IBamlTyName|null);

                /** BamlValueUnionVariant isOptional. */
                public isOptional: boolean;

                /** BamlValueUnionVariant isSinglePattern. */
                public isSinglePattern: boolean;

                /** BamlValueUnionVariant selfType. */
                public selfType?: (baml_core.cffi.v1.IBamlTy|null);

                /** BamlValueUnionVariant valueOptionName. */
                public valueOptionName: string;

                /** BamlValueUnionVariant value. */
                public value?: (baml_core.cffi.v1.IBamlOutboundValue|null);

                /**
                 * Creates a new BamlValueUnionVariant instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueUnionVariant instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlValueUnionVariant): baml_core.cffi.v1.BamlValueUnionVariant;

                /**
                 * Encodes the specified BamlValueUnionVariant message. Does not implicitly {@link baml_core.cffi.v1.BamlValueUnionVariant.verify|verify} messages.
                 * @param message BamlValueUnionVariant message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlValueUnionVariant, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueUnionVariant message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueUnionVariant.verify|verify} messages.
                 * @param message BamlValueUnionVariant message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlValueUnionVariant, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueUnionVariant message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlValueUnionVariant;

                /**
                 * Decodes a BamlValueUnionVariant message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlValueUnionVariant;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlValueUnionVariant;

                /**
                 * Creates a plain object from a BamlValueUnionVariant message. Also converts values to other types if specified.
                 * @param message BamlValueUnionVariant
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlValueUnionVariant, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                media?: (baml_core.cffi.v1.MediaTypeEnum|null);

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
                constructor(properties?: baml_core.cffi.v1.IBamlValueMedia);

                /** BamlValueMedia media. */
                public media: baml_core.cffi.v1.MediaTypeEnum;

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
                public static create(properties?: baml_core.cffi.v1.IBamlValueMedia): baml_core.cffi.v1.BamlValueMedia;

                /**
                 * Encodes the specified BamlValueMedia message. Does not implicitly {@link baml_core.cffi.v1.BamlValueMedia.verify|verify} messages.
                 * @param message BamlValueMedia message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlValueMedia, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueMedia message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValueMedia.verify|verify} messages.
                 * @param message BamlValueMedia message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlValueMedia, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueMedia message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlValueMedia;

                /**
                 * Decodes a BamlValueMedia message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlValueMedia;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlValueMedia;

                /**
                 * Creates a plain object from a BamlValueMedia message. Also converts values to other types if specified.
                 * @param message BamlValueMedia
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlValueMedia, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                simple?: (baml_core.cffi.v1.IBamlValuePromptAstSimple|null);

                /** BamlValuePromptAst message */
                message?: (baml_core.cffi.v1.IBamlValuePromptAstMessage|null);

                /** BamlValuePromptAst multiple */
                multiple?: (baml_core.cffi.v1.IBamlValuePromptAstMultiple|null);
            }

            /** Represents a BamlValuePromptAst. */
            class BamlValuePromptAst implements IBamlValuePromptAst {

                /**
                 * Constructs a new BamlValuePromptAst.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlValuePromptAst);

                /** BamlValuePromptAst simple. */
                public simple?: (baml_core.cffi.v1.IBamlValuePromptAstSimple|null);

                /** BamlValuePromptAst message. */
                public message?: (baml_core.cffi.v1.IBamlValuePromptAstMessage|null);

                /** BamlValuePromptAst multiple. */
                public multiple?: (baml_core.cffi.v1.IBamlValuePromptAstMultiple|null);

                /** BamlValuePromptAst value. */
                public value?: ("simple"|"message"|"multiple");

                /**
                 * Creates a new BamlValuePromptAst instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAst instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlValuePromptAst): baml_core.cffi.v1.BamlValuePromptAst;

                /**
                 * Encodes the specified BamlValuePromptAst message. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAst.verify|verify} messages.
                 * @param message BamlValuePromptAst message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlValuePromptAst, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAst message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAst.verify|verify} messages.
                 * @param message BamlValuePromptAst message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlValuePromptAst, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAst message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAst
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlValuePromptAst;

                /**
                 * Decodes a BamlValuePromptAst message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAst
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlValuePromptAst;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlValuePromptAst;

                /**
                 * Creates a plain object from a BamlValuePromptAst message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAst
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlValuePromptAst, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                content?: (baml_core.cffi.v1.IBamlValuePromptAstSimple|null);

                /** BamlValuePromptAstMessage metadataAsJson */
                metadataAsJson?: (string|null);
            }

            /** Represents a BamlValuePromptAstMessage. */
            class BamlValuePromptAstMessage implements IBamlValuePromptAstMessage {

                /**
                 * Constructs a new BamlValuePromptAstMessage.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlValuePromptAstMessage);

                /** BamlValuePromptAstMessage role. */
                public role: string;

                /** BamlValuePromptAstMessage content. */
                public content?: (baml_core.cffi.v1.IBamlValuePromptAstSimple|null);

                /** BamlValuePromptAstMessage metadataAsJson. */
                public metadataAsJson: string;

                /**
                 * Creates a new BamlValuePromptAstMessage instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAstMessage instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlValuePromptAstMessage): baml_core.cffi.v1.BamlValuePromptAstMessage;

                /**
                 * Encodes the specified BamlValuePromptAstMessage message. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstMessage.verify|verify} messages.
                 * @param message BamlValuePromptAstMessage message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlValuePromptAstMessage, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAstMessage message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstMessage.verify|verify} messages.
                 * @param message BamlValuePromptAstMessage message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlValuePromptAstMessage, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAstMessage message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAstMessage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlValuePromptAstMessage;

                /**
                 * Decodes a BamlValuePromptAstMessage message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAstMessage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlValuePromptAstMessage;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlValuePromptAstMessage;

                /**
                 * Creates a plain object from a BamlValuePromptAstMessage message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAstMessage
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlValuePromptAstMessage, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                items?: (baml_core.cffi.v1.IBamlValuePromptAst[]|null);
            }

            /** Represents a BamlValuePromptAstMultiple. */
            class BamlValuePromptAstMultiple implements IBamlValuePromptAstMultiple {

                /**
                 * Constructs a new BamlValuePromptAstMultiple.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlValuePromptAstMultiple);

                /** BamlValuePromptAstMultiple items. */
                public items: baml_core.cffi.v1.IBamlValuePromptAst[];

                /**
                 * Creates a new BamlValuePromptAstMultiple instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAstMultiple instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlValuePromptAstMultiple): baml_core.cffi.v1.BamlValuePromptAstMultiple;

                /**
                 * Encodes the specified BamlValuePromptAstMultiple message. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstMultiple.verify|verify} messages.
                 * @param message BamlValuePromptAstMultiple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlValuePromptAstMultiple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAstMultiple message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstMultiple.verify|verify} messages.
                 * @param message BamlValuePromptAstMultiple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlValuePromptAstMultiple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAstMultiple message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAstMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlValuePromptAstMultiple;

                /**
                 * Decodes a BamlValuePromptAstMultiple message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAstMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlValuePromptAstMultiple;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlValuePromptAstMultiple;

                /**
                 * Creates a plain object from a BamlValuePromptAstMultiple message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAstMultiple
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlValuePromptAstMultiple, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                media?: (baml_core.cffi.v1.IBamlValueMedia|null);

                /** BamlValuePromptAstSimple multiple */
                multiple?: (baml_core.cffi.v1.IBamlValuePromptAstSimpleMultiple|null);
            }

            /** Represents a BamlValuePromptAstSimple. */
            class BamlValuePromptAstSimple implements IBamlValuePromptAstSimple {

                /**
                 * Constructs a new BamlValuePromptAstSimple.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlValuePromptAstSimple);

                /** BamlValuePromptAstSimple string. */
                public string?: (string|null);

                /** BamlValuePromptAstSimple media. */
                public media?: (baml_core.cffi.v1.IBamlValueMedia|null);

                /** BamlValuePromptAstSimple multiple. */
                public multiple?: (baml_core.cffi.v1.IBamlValuePromptAstSimpleMultiple|null);

                /** BamlValuePromptAstSimple value. */
                public value?: ("string"|"media"|"multiple");

                /**
                 * Creates a new BamlValuePromptAstSimple instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAstSimple instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlValuePromptAstSimple): baml_core.cffi.v1.BamlValuePromptAstSimple;

                /**
                 * Encodes the specified BamlValuePromptAstSimple message. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstSimple.verify|verify} messages.
                 * @param message BamlValuePromptAstSimple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlValuePromptAstSimple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAstSimple message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstSimple.verify|verify} messages.
                 * @param message BamlValuePromptAstSimple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlValuePromptAstSimple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAstSimple message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAstSimple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlValuePromptAstSimple;

                /**
                 * Decodes a BamlValuePromptAstSimple message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAstSimple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlValuePromptAstSimple;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlValuePromptAstSimple;

                /**
                 * Creates a plain object from a BamlValuePromptAstSimple message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAstSimple
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlValuePromptAstSimple, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                items?: (baml_core.cffi.v1.IBamlValuePromptAstSimple[]|null);
            }

            /** Represents a BamlValuePromptAstSimpleMultiple. */
            class BamlValuePromptAstSimpleMultiple implements IBamlValuePromptAstSimpleMultiple {

                /**
                 * Constructs a new BamlValuePromptAstSimpleMultiple.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlValuePromptAstSimpleMultiple);

                /** BamlValuePromptAstSimpleMultiple items. */
                public items: baml_core.cffi.v1.IBamlValuePromptAstSimple[];

                /**
                 * Creates a new BamlValuePromptAstSimpleMultiple instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAstSimpleMultiple instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlValuePromptAstSimpleMultiple): baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple;

                /**
                 * Encodes the specified BamlValuePromptAstSimpleMultiple message. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple.verify|verify} messages.
                 * @param message BamlValuePromptAstSimpleMultiple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlValuePromptAstSimpleMultiple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAstSimpleMultiple message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple.verify|verify} messages.
                 * @param message BamlValuePromptAstSimpleMultiple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlValuePromptAstSimpleMultiple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAstSimpleMultiple message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAstSimpleMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple;

                /**
                 * Decodes a BamlValuePromptAstSimpleMultiple message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAstSimpleMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple;

                /**
                 * Creates a plain object from a BamlValuePromptAstSimpleMultiple message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAstSimpleMultiple
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlValuePromptAstSimpleMultiple, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** Properties of a BamlTy. */
            interface IBamlTy {

                /** BamlTy stringType */
                stringType?: (baml_core.cffi.v1.IBamlTyString|null);

                /** BamlTy intType */
                intType?: (baml_core.cffi.v1.IBamlTyInt|null);

                /** BamlTy floatType */
                floatType?: (baml_core.cffi.v1.IBamlTyFloat|null);

                /** BamlTy boolType */
                boolType?: (baml_core.cffi.v1.IBamlTyBool|null);

                /** BamlTy nullType */
                nullType?: (baml_core.cffi.v1.IBamlTyNull|null);

                /** BamlTy literalType */
                literalType?: (baml_core.cffi.v1.IBamlTyLiteral|null);

                /** BamlTy mediaType */
                mediaType?: (baml_core.cffi.v1.IBamlTyMedia|null);

                /** BamlTy enumType */
                enumType?: (baml_core.cffi.v1.IBamlTyEnum|null);

                /** BamlTy classType */
                classType?: (baml_core.cffi.v1.IBamlTyClass|null);

                /** BamlTy typeAliasType */
                typeAliasType?: (baml_core.cffi.v1.IBamlTyTypeAlias|null);

                /** BamlTy listType */
                listType?: (baml_core.cffi.v1.IBamlTyList|null);

                /** BamlTy mapType */
                mapType?: (baml_core.cffi.v1.IBamlTyMap|null);

                /** BamlTy unionVariantType */
                unionVariantType?: (baml_core.cffi.v1.IBamlTyUnionVariant|null);

                /** BamlTy optionalType */
                optionalType?: (baml_core.cffi.v1.IBamlTyOptional|null);

                /** BamlTy anyType */
                anyType?: (baml_core.cffi.v1.IBamlTyAny|null);

                /** BamlTy uint8arrayType */
                uint8arrayType?: (baml_core.cffi.v1.IBamlTyUint8Array|null);

                /** BamlTy unknownType */
                unknownType?: (baml_core.cffi.v1.IBamlTyUnknown|null);
            }

            /** Represents a BamlTy. */
            class BamlTy implements IBamlTy {

                /**
                 * Constructs a new BamlTy.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTy);

                /** BamlTy stringType. */
                public stringType?: (baml_core.cffi.v1.IBamlTyString|null);

                /** BamlTy intType. */
                public intType?: (baml_core.cffi.v1.IBamlTyInt|null);

                /** BamlTy floatType. */
                public floatType?: (baml_core.cffi.v1.IBamlTyFloat|null);

                /** BamlTy boolType. */
                public boolType?: (baml_core.cffi.v1.IBamlTyBool|null);

                /** BamlTy nullType. */
                public nullType?: (baml_core.cffi.v1.IBamlTyNull|null);

                /** BamlTy literalType. */
                public literalType?: (baml_core.cffi.v1.IBamlTyLiteral|null);

                /** BamlTy mediaType. */
                public mediaType?: (baml_core.cffi.v1.IBamlTyMedia|null);

                /** BamlTy enumType. */
                public enumType?: (baml_core.cffi.v1.IBamlTyEnum|null);

                /** BamlTy classType. */
                public classType?: (baml_core.cffi.v1.IBamlTyClass|null);

                /** BamlTy typeAliasType. */
                public typeAliasType?: (baml_core.cffi.v1.IBamlTyTypeAlias|null);

                /** BamlTy listType. */
                public listType?: (baml_core.cffi.v1.IBamlTyList|null);

                /** BamlTy mapType. */
                public mapType?: (baml_core.cffi.v1.IBamlTyMap|null);

                /** BamlTy unionVariantType. */
                public unionVariantType?: (baml_core.cffi.v1.IBamlTyUnionVariant|null);

                /** BamlTy optionalType. */
                public optionalType?: (baml_core.cffi.v1.IBamlTyOptional|null);

                /** BamlTy anyType. */
                public anyType?: (baml_core.cffi.v1.IBamlTyAny|null);

                /** BamlTy uint8arrayType. */
                public uint8arrayType?: (baml_core.cffi.v1.IBamlTyUint8Array|null);

                /** BamlTy unknownType. */
                public unknownType?: (baml_core.cffi.v1.IBamlTyUnknown|null);

                /** BamlTy type. */
                public type?: ("stringType"|"intType"|"floatType"|"boolType"|"nullType"|"literalType"|"mediaType"|"enumType"|"classType"|"typeAliasType"|"listType"|"mapType"|"unionVariantType"|"optionalType"|"anyType"|"uint8arrayType"|"unknownType");

                /**
                 * Creates a new BamlTy instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTy instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTy): baml_core.cffi.v1.BamlTy;

                /**
                 * Encodes the specified BamlTy message. Does not implicitly {@link baml_core.cffi.v1.BamlTy.verify|verify} messages.
                 * @param message BamlTy message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTy, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTy message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTy.verify|verify} messages.
                 * @param message BamlTy message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTy, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTy message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTy
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTy;

                /**
                 * Decodes a BamlTy message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTy
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTy;

                /**
                 * Verifies a BamlTy message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTy message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTy
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTy;

                /**
                 * Creates a plain object from a BamlTy message. Also converts values to other types if specified.
                 * @param message BamlTy
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTy, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTy to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTy
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyString. */
            interface IBamlTyString {
            }

            /** Represents a BamlTyString. */
            class BamlTyString implements IBamlTyString {

                /**
                 * Constructs a new BamlTyString.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyString);

                /**
                 * Creates a new BamlTyString instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyString instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyString): baml_core.cffi.v1.BamlTyString;

                /**
                 * Encodes the specified BamlTyString message. Does not implicitly {@link baml_core.cffi.v1.BamlTyString.verify|verify} messages.
                 * @param message BamlTyString message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyString, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyString message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyString.verify|verify} messages.
                 * @param message BamlTyString message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyString, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyString message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyString;

                /**
                 * Decodes a BamlTyString message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyString;

                /**
                 * Verifies a BamlTyString message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyString message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyString
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyString;

                /**
                 * Creates a plain object from a BamlTyString message. Also converts values to other types if specified.
                 * @param message BamlTyString
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyString, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyString to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyString
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyInt. */
            interface IBamlTyInt {
            }

            /** Represents a BamlTyInt. */
            class BamlTyInt implements IBamlTyInt {

                /**
                 * Constructs a new BamlTyInt.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyInt);

                /**
                 * Creates a new BamlTyInt instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyInt instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyInt): baml_core.cffi.v1.BamlTyInt;

                /**
                 * Encodes the specified BamlTyInt message. Does not implicitly {@link baml_core.cffi.v1.BamlTyInt.verify|verify} messages.
                 * @param message BamlTyInt message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyInt, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyInt message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyInt.verify|verify} messages.
                 * @param message BamlTyInt message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyInt, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyInt message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyInt;

                /**
                 * Decodes a BamlTyInt message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyInt;

                /**
                 * Verifies a BamlTyInt message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyInt message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyInt
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyInt;

                /**
                 * Creates a plain object from a BamlTyInt message. Also converts values to other types if specified.
                 * @param message BamlTyInt
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyInt, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyInt to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyInt
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyFloat. */
            interface IBamlTyFloat {
            }

            /** Represents a BamlTyFloat. */
            class BamlTyFloat implements IBamlTyFloat {

                /**
                 * Constructs a new BamlTyFloat.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyFloat);

                /**
                 * Creates a new BamlTyFloat instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyFloat instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyFloat): baml_core.cffi.v1.BamlTyFloat;

                /**
                 * Encodes the specified BamlTyFloat message. Does not implicitly {@link baml_core.cffi.v1.BamlTyFloat.verify|verify} messages.
                 * @param message BamlTyFloat message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyFloat, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyFloat message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyFloat.verify|verify} messages.
                 * @param message BamlTyFloat message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyFloat, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyFloat message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyFloat
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyFloat;

                /**
                 * Decodes a BamlTyFloat message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyFloat
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyFloat;

                /**
                 * Verifies a BamlTyFloat message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyFloat message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyFloat
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyFloat;

                /**
                 * Creates a plain object from a BamlTyFloat message. Also converts values to other types if specified.
                 * @param message BamlTyFloat
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyFloat, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyFloat to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyFloat
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyBool. */
            interface IBamlTyBool {
            }

            /** Represents a BamlTyBool. */
            class BamlTyBool implements IBamlTyBool {

                /**
                 * Constructs a new BamlTyBool.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyBool);

                /**
                 * Creates a new BamlTyBool instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyBool instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyBool): baml_core.cffi.v1.BamlTyBool;

                /**
                 * Encodes the specified BamlTyBool message. Does not implicitly {@link baml_core.cffi.v1.BamlTyBool.verify|verify} messages.
                 * @param message BamlTyBool message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyBool, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyBool message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyBool.verify|verify} messages.
                 * @param message BamlTyBool message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyBool, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyBool message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyBool;

                /**
                 * Decodes a BamlTyBool message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyBool;

                /**
                 * Verifies a BamlTyBool message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyBool message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyBool
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyBool;

                /**
                 * Creates a plain object from a BamlTyBool message. Also converts values to other types if specified.
                 * @param message BamlTyBool
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyBool, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyBool to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyBool
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyNull. */
            interface IBamlTyNull {
            }

            /** Represents a BamlTyNull. */
            class BamlTyNull implements IBamlTyNull {

                /**
                 * Constructs a new BamlTyNull.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyNull);

                /**
                 * Creates a new BamlTyNull instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyNull instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyNull): baml_core.cffi.v1.BamlTyNull;

                /**
                 * Encodes the specified BamlTyNull message. Does not implicitly {@link baml_core.cffi.v1.BamlTyNull.verify|verify} messages.
                 * @param message BamlTyNull message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyNull, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyNull message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyNull.verify|verify} messages.
                 * @param message BamlTyNull message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyNull, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyNull message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyNull;

                /**
                 * Decodes a BamlTyNull message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyNull;

                /**
                 * Verifies a BamlTyNull message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyNull message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyNull
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyNull;

                /**
                 * Creates a plain object from a BamlTyNull message. Also converts values to other types if specified.
                 * @param message BamlTyNull
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyNull, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyNull to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyNull
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyUint8Array. */
            interface IBamlTyUint8Array {
            }

            /** Represents a BamlTyUint8Array. */
            class BamlTyUint8Array implements IBamlTyUint8Array {

                /**
                 * Constructs a new BamlTyUint8Array.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyUint8Array);

                /**
                 * Creates a new BamlTyUint8Array instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyUint8Array instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyUint8Array): baml_core.cffi.v1.BamlTyUint8Array;

                /**
                 * Encodes the specified BamlTyUint8Array message. Does not implicitly {@link baml_core.cffi.v1.BamlTyUint8Array.verify|verify} messages.
                 * @param message BamlTyUint8Array message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyUint8Array, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyUint8Array message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyUint8Array.verify|verify} messages.
                 * @param message BamlTyUint8Array message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyUint8Array, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyUint8Array message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyUint8Array
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyUint8Array;

                /**
                 * Decodes a BamlTyUint8Array message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyUint8Array
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyUint8Array;

                /**
                 * Verifies a BamlTyUint8Array message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyUint8Array message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyUint8Array
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyUint8Array;

                /**
                 * Creates a plain object from a BamlTyUint8Array message. Also converts values to other types if specified.
                 * @param message BamlTyUint8Array
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyUint8Array, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyUint8Array to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyUint8Array
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyAny. */
            interface IBamlTyAny {
            }

            /** Represents a BamlTyAny. */
            class BamlTyAny implements IBamlTyAny {

                /**
                 * Constructs a new BamlTyAny.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyAny);

                /**
                 * Creates a new BamlTyAny instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyAny instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyAny): baml_core.cffi.v1.BamlTyAny;

                /**
                 * Encodes the specified BamlTyAny message. Does not implicitly {@link baml_core.cffi.v1.BamlTyAny.verify|verify} messages.
                 * @param message BamlTyAny message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyAny, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyAny message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyAny.verify|verify} messages.
                 * @param message BamlTyAny message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyAny, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyAny message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyAny
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyAny;

                /**
                 * Decodes a BamlTyAny message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyAny
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyAny;

                /**
                 * Verifies a BamlTyAny message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyAny message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyAny
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyAny;

                /**
                 * Creates a plain object from a BamlTyAny message. Also converts values to other types if specified.
                 * @param message BamlTyAny
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyAny, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyAny to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyAny
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyUnknown. */
            interface IBamlTyUnknown {
            }

            /** Represents a BamlTyUnknown. */
            class BamlTyUnknown implements IBamlTyUnknown {

                /**
                 * Constructs a new BamlTyUnknown.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyUnknown);

                /**
                 * Creates a new BamlTyUnknown instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyUnknown instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyUnknown): baml_core.cffi.v1.BamlTyUnknown;

                /**
                 * Encodes the specified BamlTyUnknown message. Does not implicitly {@link baml_core.cffi.v1.BamlTyUnknown.verify|verify} messages.
                 * @param message BamlTyUnknown message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyUnknown, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyUnknown message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyUnknown.verify|verify} messages.
                 * @param message BamlTyUnknown message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyUnknown, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyUnknown message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyUnknown
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyUnknown;

                /**
                 * Decodes a BamlTyUnknown message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyUnknown
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyUnknown;

                /**
                 * Verifies a BamlTyUnknown message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyUnknown message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyUnknown
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyUnknown;

                /**
                 * Creates a plain object from a BamlTyUnknown message. Also converts values to other types if specified.
                 * @param message BamlTyUnknown
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyUnknown, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyUnknown to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyUnknown
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
                constructor(properties?: baml_core.cffi.v1.IBamlLiteralString);

                /** BamlLiteralString value. */
                public value: string;

                /**
                 * Creates a new BamlLiteralString instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlLiteralString instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlLiteralString): baml_core.cffi.v1.BamlLiteralString;

                /**
                 * Encodes the specified BamlLiteralString message. Does not implicitly {@link baml_core.cffi.v1.BamlLiteralString.verify|verify} messages.
                 * @param message BamlLiteralString message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlLiteralString, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlLiteralString message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlLiteralString.verify|verify} messages.
                 * @param message BamlLiteralString message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlLiteralString, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlLiteralString message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlLiteralString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlLiteralString;

                /**
                 * Decodes a BamlLiteralString message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlLiteralString
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlLiteralString;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlLiteralString;

                /**
                 * Creates a plain object from a BamlLiteralString message. Also converts values to other types if specified.
                 * @param message BamlLiteralString
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlLiteralString, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                constructor(properties?: baml_core.cffi.v1.IBamlLiteralInt);

                /** BamlLiteralInt value. */
                public value: (number|Long);

                /**
                 * Creates a new BamlLiteralInt instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlLiteralInt instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlLiteralInt): baml_core.cffi.v1.BamlLiteralInt;

                /**
                 * Encodes the specified BamlLiteralInt message. Does not implicitly {@link baml_core.cffi.v1.BamlLiteralInt.verify|verify} messages.
                 * @param message BamlLiteralInt message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlLiteralInt, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlLiteralInt message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlLiteralInt.verify|verify} messages.
                 * @param message BamlLiteralInt message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlLiteralInt, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlLiteralInt message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlLiteralInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlLiteralInt;

                /**
                 * Decodes a BamlLiteralInt message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlLiteralInt
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlLiteralInt;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlLiteralInt;

                /**
                 * Creates a plain object from a BamlLiteralInt message. Also converts values to other types if specified.
                 * @param message BamlLiteralInt
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlLiteralInt, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                constructor(properties?: baml_core.cffi.v1.IBamlLiteralBool);

                /** BamlLiteralBool value. */
                public value: boolean;

                /**
                 * Creates a new BamlLiteralBool instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlLiteralBool instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlLiteralBool): baml_core.cffi.v1.BamlLiteralBool;

                /**
                 * Encodes the specified BamlLiteralBool message. Does not implicitly {@link baml_core.cffi.v1.BamlLiteralBool.verify|verify} messages.
                 * @param message BamlLiteralBool message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlLiteralBool, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlLiteralBool message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlLiteralBool.verify|verify} messages.
                 * @param message BamlLiteralBool message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlLiteralBool, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlLiteralBool message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlLiteralBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlLiteralBool;

                /**
                 * Decodes a BamlLiteralBool message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlLiteralBool
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlLiteralBool;

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
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlLiteralBool;

                /**
                 * Creates a plain object from a BamlLiteralBool message. Also converts values to other types if specified.
                 * @param message BamlLiteralBool
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlLiteralBool, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** Properties of a BamlTyLiteral. */
            interface IBamlTyLiteral {

                /** BamlTyLiteral stringLiteral */
                stringLiteral?: (baml_core.cffi.v1.IBamlLiteralString|null);

                /** BamlTyLiteral intLiteral */
                intLiteral?: (baml_core.cffi.v1.IBamlLiteralInt|null);

                /** BamlTyLiteral boolLiteral */
                boolLiteral?: (baml_core.cffi.v1.IBamlLiteralBool|null);
            }

            /** Represents a BamlTyLiteral. */
            class BamlTyLiteral implements IBamlTyLiteral {

                /**
                 * Constructs a new BamlTyLiteral.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyLiteral);

                /** BamlTyLiteral stringLiteral. */
                public stringLiteral?: (baml_core.cffi.v1.IBamlLiteralString|null);

                /** BamlTyLiteral intLiteral. */
                public intLiteral?: (baml_core.cffi.v1.IBamlLiteralInt|null);

                /** BamlTyLiteral boolLiteral. */
                public boolLiteral?: (baml_core.cffi.v1.IBamlLiteralBool|null);

                /** BamlTyLiteral literal. */
                public literal?: ("stringLiteral"|"intLiteral"|"boolLiteral");

                /**
                 * Creates a new BamlTyLiteral instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyLiteral instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyLiteral): baml_core.cffi.v1.BamlTyLiteral;

                /**
                 * Encodes the specified BamlTyLiteral message. Does not implicitly {@link baml_core.cffi.v1.BamlTyLiteral.verify|verify} messages.
                 * @param message BamlTyLiteral message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyLiteral, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyLiteral message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyLiteral.verify|verify} messages.
                 * @param message BamlTyLiteral message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyLiteral, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyLiteral message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyLiteral
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyLiteral;

                /**
                 * Decodes a BamlTyLiteral message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyLiteral
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyLiteral;

                /**
                 * Verifies a BamlTyLiteral message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyLiteral message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyLiteral
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyLiteral;

                /**
                 * Creates a plain object from a BamlTyLiteral message. Also converts values to other types if specified.
                 * @param message BamlTyLiteral
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyLiteral, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyLiteral to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyLiteral
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyMedia. */
            interface IBamlTyMedia {

                /** BamlTyMedia media */
                media?: (baml_core.cffi.v1.MediaTypeEnum|null);
            }

            /** Represents a BamlTyMedia. */
            class BamlTyMedia implements IBamlTyMedia {

                /**
                 * Constructs a new BamlTyMedia.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyMedia);

                /** BamlTyMedia media. */
                public media: baml_core.cffi.v1.MediaTypeEnum;

                /**
                 * Creates a new BamlTyMedia instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyMedia instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyMedia): baml_core.cffi.v1.BamlTyMedia;

                /**
                 * Encodes the specified BamlTyMedia message. Does not implicitly {@link baml_core.cffi.v1.BamlTyMedia.verify|verify} messages.
                 * @param message BamlTyMedia message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyMedia, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyMedia message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyMedia.verify|verify} messages.
                 * @param message BamlTyMedia message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyMedia, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyMedia message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyMedia;

                /**
                 * Decodes a BamlTyMedia message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyMedia;

                /**
                 * Verifies a BamlTyMedia message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyMedia message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyMedia
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyMedia;

                /**
                 * Creates a plain object from a BamlTyMedia message. Also converts values to other types if specified.
                 * @param message BamlTyMedia
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyMedia, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyMedia to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyMedia
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyEnum. */
            interface IBamlTyEnum {

                /** BamlTyEnum name */
                name?: (string|null);
            }

            /** Represents a BamlTyEnum. */
            class BamlTyEnum implements IBamlTyEnum {

                /**
                 * Constructs a new BamlTyEnum.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyEnum);

                /** BamlTyEnum name. */
                public name: string;

                /**
                 * Creates a new BamlTyEnum instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyEnum instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyEnum): baml_core.cffi.v1.BamlTyEnum;

                /**
                 * Encodes the specified BamlTyEnum message. Does not implicitly {@link baml_core.cffi.v1.BamlTyEnum.verify|verify} messages.
                 * @param message BamlTyEnum message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyEnum, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyEnum message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyEnum.verify|verify} messages.
                 * @param message BamlTyEnum message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyEnum, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyEnum message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyEnum;

                /**
                 * Decodes a BamlTyEnum message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyEnum;

                /**
                 * Verifies a BamlTyEnum message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyEnum message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyEnum
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyEnum;

                /**
                 * Creates a plain object from a BamlTyEnum message. Also converts values to other types if specified.
                 * @param message BamlTyEnum
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyEnum, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyEnum to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyEnum
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyClass. */
            interface IBamlTyClass {

                /** BamlTyClass name */
                name?: (baml_core.cffi.v1.IBamlTyName|null);
            }

            /** Represents a BamlTyClass. */
            class BamlTyClass implements IBamlTyClass {

                /**
                 * Constructs a new BamlTyClass.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyClass);

                /** BamlTyClass name. */
                public name?: (baml_core.cffi.v1.IBamlTyName|null);

                /**
                 * Creates a new BamlTyClass instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyClass instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyClass): baml_core.cffi.v1.BamlTyClass;

                /**
                 * Encodes the specified BamlTyClass message. Does not implicitly {@link baml_core.cffi.v1.BamlTyClass.verify|verify} messages.
                 * @param message BamlTyClass message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyClass, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyClass message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyClass.verify|verify} messages.
                 * @param message BamlTyClass message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyClass, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyClass message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyClass;

                /**
                 * Decodes a BamlTyClass message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyClass;

                /**
                 * Verifies a BamlTyClass message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyClass message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyClass
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyClass;

                /**
                 * Creates a plain object from a BamlTyClass message. Also converts values to other types if specified.
                 * @param message BamlTyClass
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyClass, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyClass to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyClass
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyTypeAlias. */
            interface IBamlTyTypeAlias {

                /** BamlTyTypeAlias name */
                name?: (baml_core.cffi.v1.IBamlTyName|null);
            }

            /** Represents a BamlTyTypeAlias. */
            class BamlTyTypeAlias implements IBamlTyTypeAlias {

                /**
                 * Constructs a new BamlTyTypeAlias.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyTypeAlias);

                /** BamlTyTypeAlias name. */
                public name?: (baml_core.cffi.v1.IBamlTyName|null);

                /**
                 * Creates a new BamlTyTypeAlias instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyTypeAlias instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyTypeAlias): baml_core.cffi.v1.BamlTyTypeAlias;

                /**
                 * Encodes the specified BamlTyTypeAlias message. Does not implicitly {@link baml_core.cffi.v1.BamlTyTypeAlias.verify|verify} messages.
                 * @param message BamlTyTypeAlias message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyTypeAlias, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyTypeAlias message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyTypeAlias.verify|verify} messages.
                 * @param message BamlTyTypeAlias message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyTypeAlias, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyTypeAlias message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyTypeAlias
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyTypeAlias;

                /**
                 * Decodes a BamlTyTypeAlias message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyTypeAlias
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyTypeAlias;

                /**
                 * Verifies a BamlTyTypeAlias message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyTypeAlias message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyTypeAlias
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyTypeAlias;

                /**
                 * Creates a plain object from a BamlTyTypeAlias message. Also converts values to other types if specified.
                 * @param message BamlTyTypeAlias
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyTypeAlias, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyTypeAlias to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyTypeAlias
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyList. */
            interface IBamlTyList {

                /** BamlTyList itemType */
                itemType?: (baml_core.cffi.v1.IBamlTy|null);
            }

            /** Represents a BamlTyList. */
            class BamlTyList implements IBamlTyList {

                /**
                 * Constructs a new BamlTyList.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyList);

                /** BamlTyList itemType. */
                public itemType?: (baml_core.cffi.v1.IBamlTy|null);

                /**
                 * Creates a new BamlTyList instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyList instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyList): baml_core.cffi.v1.BamlTyList;

                /**
                 * Encodes the specified BamlTyList message. Does not implicitly {@link baml_core.cffi.v1.BamlTyList.verify|verify} messages.
                 * @param message BamlTyList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyList, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyList message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyList.verify|verify} messages.
                 * @param message BamlTyList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyList, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyList message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyList;

                /**
                 * Decodes a BamlTyList message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyList;

                /**
                 * Verifies a BamlTyList message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyList message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyList
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyList;

                /**
                 * Creates a plain object from a BamlTyList message. Also converts values to other types if specified.
                 * @param message BamlTyList
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyList, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyList to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyList
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyMap. */
            interface IBamlTyMap {

                /** BamlTyMap keyType */
                keyType?: (baml_core.cffi.v1.IBamlTy|null);

                /** BamlTyMap valueType */
                valueType?: (baml_core.cffi.v1.IBamlTy|null);
            }

            /** Represents a BamlTyMap. */
            class BamlTyMap implements IBamlTyMap {

                /**
                 * Constructs a new BamlTyMap.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyMap);

                /** BamlTyMap keyType. */
                public keyType?: (baml_core.cffi.v1.IBamlTy|null);

                /** BamlTyMap valueType. */
                public valueType?: (baml_core.cffi.v1.IBamlTy|null);

                /**
                 * Creates a new BamlTyMap instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyMap instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyMap): baml_core.cffi.v1.BamlTyMap;

                /**
                 * Encodes the specified BamlTyMap message. Does not implicitly {@link baml_core.cffi.v1.BamlTyMap.verify|verify} messages.
                 * @param message BamlTyMap message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyMap, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyMap message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyMap.verify|verify} messages.
                 * @param message BamlTyMap message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyMap, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyMap message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyMap;

                /**
                 * Decodes a BamlTyMap message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyMap;

                /**
                 * Verifies a BamlTyMap message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyMap message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyMap
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyMap;

                /**
                 * Creates a plain object from a BamlTyMap message. Also converts values to other types if specified.
                 * @param message BamlTyMap
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyMap, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyMap to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyMap
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyUnionVariant. */
            interface IBamlTyUnionVariant {

                /** BamlTyUnionVariant name */
                name?: (baml_core.cffi.v1.IBamlTyName|null);
            }

            /** Represents a BamlTyUnionVariant. */
            class BamlTyUnionVariant implements IBamlTyUnionVariant {

                /**
                 * Constructs a new BamlTyUnionVariant.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyUnionVariant);

                /** BamlTyUnionVariant name. */
                public name?: (baml_core.cffi.v1.IBamlTyName|null);

                /**
                 * Creates a new BamlTyUnionVariant instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyUnionVariant instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyUnionVariant): baml_core.cffi.v1.BamlTyUnionVariant;

                /**
                 * Encodes the specified BamlTyUnionVariant message. Does not implicitly {@link baml_core.cffi.v1.BamlTyUnionVariant.verify|verify} messages.
                 * @param message BamlTyUnionVariant message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyUnionVariant, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyUnionVariant message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyUnionVariant.verify|verify} messages.
                 * @param message BamlTyUnionVariant message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyUnionVariant, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyUnionVariant message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyUnionVariant;

                /**
                 * Decodes a BamlTyUnionVariant message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyUnionVariant;

                /**
                 * Verifies a BamlTyUnionVariant message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyUnionVariant message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyUnionVariant
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyUnionVariant;

                /**
                 * Creates a plain object from a BamlTyUnionVariant message. Also converts values to other types if specified.
                 * @param message BamlTyUnionVariant
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyUnionVariant, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyUnionVariant to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyUnionVariant
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyOptional. */
            interface IBamlTyOptional {

                /** BamlTyOptional value */
                value?: (baml_core.cffi.v1.IBamlTy|null);
            }

            /** Represents a BamlTyOptional. */
            class BamlTyOptional implements IBamlTyOptional {

                /**
                 * Constructs a new BamlTyOptional.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_core.cffi.v1.IBamlTyOptional);

                /** BamlTyOptional value. */
                public value?: (baml_core.cffi.v1.IBamlTy|null);

                /**
                 * Creates a new BamlTyOptional instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyOptional instance
                 */
                public static create(properties?: baml_core.cffi.v1.IBamlTyOptional): baml_core.cffi.v1.BamlTyOptional;

                /**
                 * Encodes the specified BamlTyOptional message. Does not implicitly {@link baml_core.cffi.v1.BamlTyOptional.verify|verify} messages.
                 * @param message BamlTyOptional message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_core.cffi.v1.IBamlTyOptional, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyOptional message, length delimited. Does not implicitly {@link baml_core.cffi.v1.BamlTyOptional.verify|verify} messages.
                 * @param message BamlTyOptional message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_core.cffi.v1.IBamlTyOptional, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyOptional message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyOptional
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_core.cffi.v1.BamlTyOptional;

                /**
                 * Decodes a BamlTyOptional message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyOptional
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_core.cffi.v1.BamlTyOptional;

                /**
                 * Verifies a BamlTyOptional message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyOptional message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyOptional
                 */
                public static fromObject(object: { [k: string]: any }): baml_core.cffi.v1.BamlTyOptional;

                /**
                 * Creates a plain object from a BamlTyOptional message. Also converts values to other types if specified.
                 * @param message BamlTyOptional
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_core.cffi.v1.BamlTyOptional, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyOptional to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyOptional
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }
        }
    }
}
