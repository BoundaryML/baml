/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import * as $protobuf from "protobufjs";
import Long from "long";
/** Namespace baml_bridge. */
export namespace baml_bridge {

    /** Namespace cffi. */
    namespace cffi {

        /** Namespace v1. */
        namespace v1 {

            /** Properties of an InboundValue. */
            interface IInboundValue {

                /** InboundValue valueType */
                valueType?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** InboundValue stringValue */
                stringValue?: (string|null);

                /** InboundValue intValue */
                intValue?: (number|Long|null);

                /** InboundValue floatValue */
                floatValue?: (number|null);

                /** InboundValue boolValue */
                boolValue?: (boolean|null);

                /** InboundValue listValue */
                listValue?: (baml_bridge.cffi.v1.IInboundListValue|null);

                /** InboundValue mapValue */
                mapValue?: (baml_bridge.cffi.v1.IInboundMapValue|null);

                /** InboundValue classValue */
                classValue?: (baml_bridge.cffi.v1.IInboundClassValue|null);

                /** InboundValue enumValue */
                enumValue?: (baml_bridge.cffi.v1.IInboundEnumValue|null);

                /** InboundValue handle */
                handle?: (baml_bridge.cffi.v1.IBamlHandle|null);

                /** InboundValue uint8arrayValue */
                uint8arrayValue?: (Uint8Array|null);

                /** InboundValue bigintValue */
                bigintValue?: (string|null);

                /** InboundValue tyValue */
                tyValue?: (baml_bridge.cffi.v1.IBamlTy|null);
            }

            /** Represents an InboundValue. */
            class InboundValue implements IInboundValue {

                /**
                 * Constructs a new InboundValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IInboundValue);

                /** InboundValue valueType. */
                public valueType?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** InboundValue stringValue. */
                public stringValue?: (string|null);

                /** InboundValue intValue. */
                public intValue?: (number|Long|null);

                /** InboundValue floatValue. */
                public floatValue?: (number|null);

                /** InboundValue boolValue. */
                public boolValue?: (boolean|null);

                /** InboundValue listValue. */
                public listValue?: (baml_bridge.cffi.v1.IInboundListValue|null);

                /** InboundValue mapValue. */
                public mapValue?: (baml_bridge.cffi.v1.IInboundMapValue|null);

                /** InboundValue classValue. */
                public classValue?: (baml_bridge.cffi.v1.IInboundClassValue|null);

                /** InboundValue enumValue. */
                public enumValue?: (baml_bridge.cffi.v1.IInboundEnumValue|null);

                /** InboundValue handle. */
                public handle?: (baml_bridge.cffi.v1.IBamlHandle|null);

                /** InboundValue uint8arrayValue. */
                public uint8arrayValue?: (Uint8Array|null);

                /** InboundValue bigintValue. */
                public bigintValue?: (string|null);

                /** InboundValue tyValue. */
                public tyValue?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** InboundValue value. */
                public value?: ("stringValue"|"intValue"|"floatValue"|"boolValue"|"listValue"|"mapValue"|"classValue"|"enumValue"|"handle"|"uint8arrayValue"|"bigintValue"|"tyValue");

                /**
                 * Creates a new InboundValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundValue instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IInboundValue): baml_bridge.cffi.v1.InboundValue;

                /**
                 * Encodes the specified InboundValue message. Does not implicitly {@link baml_bridge.cffi.v1.InboundValue.verify|verify} messages.
                 * @param message InboundValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IInboundValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundValue message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.InboundValue.verify|verify} messages.
                 * @param message InboundValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IInboundValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.InboundValue;

                /**
                 * Decodes an InboundValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.InboundValue;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.InboundValue;

                /**
                 * Creates a plain object from an InboundValue message. Also converts values to other types if specified.
                 * @param message InboundValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.InboundValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                values?: (baml_bridge.cffi.v1.IInboundValue[]|null);
            }

            /** Represents an InboundListValue. */
            class InboundListValue implements IInboundListValue {

                /**
                 * Constructs a new InboundListValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IInboundListValue);

                /** InboundListValue values. */
                public values: baml_bridge.cffi.v1.IInboundValue[];

                /**
                 * Creates a new InboundListValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundListValue instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IInboundListValue): baml_bridge.cffi.v1.InboundListValue;

                /**
                 * Encodes the specified InboundListValue message. Does not implicitly {@link baml_bridge.cffi.v1.InboundListValue.verify|verify} messages.
                 * @param message InboundListValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IInboundListValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundListValue message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.InboundListValue.verify|verify} messages.
                 * @param message InboundListValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IInboundListValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundListValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundListValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.InboundListValue;

                /**
                 * Decodes an InboundListValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundListValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.InboundListValue;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.InboundListValue;

                /**
                 * Creates a plain object from an InboundListValue message. Also converts values to other types if specified.
                 * @param message InboundListValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.InboundListValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                entries?: (baml_bridge.cffi.v1.IInboundMapEntry[]|null);
            }

            /** Represents an InboundMapValue. */
            class InboundMapValue implements IInboundMapValue {

                /**
                 * Constructs a new InboundMapValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IInboundMapValue);

                /** InboundMapValue entries. */
                public entries: baml_bridge.cffi.v1.IInboundMapEntry[];

                /**
                 * Creates a new InboundMapValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundMapValue instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IInboundMapValue): baml_bridge.cffi.v1.InboundMapValue;

                /**
                 * Encodes the specified InboundMapValue message. Does not implicitly {@link baml_bridge.cffi.v1.InboundMapValue.verify|verify} messages.
                 * @param message InboundMapValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IInboundMapValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundMapValue message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.InboundMapValue.verify|verify} messages.
                 * @param message InboundMapValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IInboundMapValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundMapValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundMapValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.InboundMapValue;

                /**
                 * Decodes an InboundMapValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundMapValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.InboundMapValue;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.InboundMapValue;

                /**
                 * Creates a plain object from an InboundMapValue message. Also converts values to other types if specified.
                 * @param message InboundMapValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.InboundMapValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                enumKey?: (baml_bridge.cffi.v1.IInboundEnumValue|null);

                /** InboundMapEntry value */
                value?: (baml_bridge.cffi.v1.IInboundValue|null);
            }

            /** Represents an InboundMapEntry. */
            class InboundMapEntry implements IInboundMapEntry {

                /**
                 * Constructs a new InboundMapEntry.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IInboundMapEntry);

                /** InboundMapEntry stringKey. */
                public stringKey?: (string|null);

                /** InboundMapEntry intKey. */
                public intKey?: (number|Long|null);

                /** InboundMapEntry boolKey. */
                public boolKey?: (boolean|null);

                /** InboundMapEntry enumKey. */
                public enumKey?: (baml_bridge.cffi.v1.IInboundEnumValue|null);

                /** InboundMapEntry value. */
                public value?: (baml_bridge.cffi.v1.IInboundValue|null);

                /** InboundMapEntry key. */
                public key?: ("stringKey"|"intKey"|"boolKey"|"enumKey");

                /**
                 * Creates a new InboundMapEntry instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundMapEntry instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IInboundMapEntry): baml_bridge.cffi.v1.InboundMapEntry;

                /**
                 * Encodes the specified InboundMapEntry message. Does not implicitly {@link baml_bridge.cffi.v1.InboundMapEntry.verify|verify} messages.
                 * @param message InboundMapEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IInboundMapEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundMapEntry message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.InboundMapEntry.verify|verify} messages.
                 * @param message InboundMapEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IInboundMapEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundMapEntry message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.InboundMapEntry;

                /**
                 * Decodes an InboundMapEntry message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.InboundMapEntry;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.InboundMapEntry;

                /**
                 * Creates a plain object from an InboundMapEntry message. Also converts values to other types if specified.
                 * @param message InboundMapEntry
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.InboundMapEntry, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

                /** InboundClassValue fields */
                fields?: (baml_bridge.cffi.v1.IInboundMapEntry[]|null);
            }

            /** Represents an InboundClassValue. */
            class InboundClassValue implements IInboundClassValue {

                /**
                 * Constructs a new InboundClassValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IInboundClassValue);

                /** InboundClassValue fields. */
                public fields: baml_bridge.cffi.v1.IInboundMapEntry[];

                /**
                 * Creates a new InboundClassValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundClassValue instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IInboundClassValue): baml_bridge.cffi.v1.InboundClassValue;

                /**
                 * Encodes the specified InboundClassValue message. Does not implicitly {@link baml_bridge.cffi.v1.InboundClassValue.verify|verify} messages.
                 * @param message InboundClassValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IInboundClassValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundClassValue message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.InboundClassValue.verify|verify} messages.
                 * @param message InboundClassValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IInboundClassValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundClassValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundClassValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.InboundClassValue;

                /**
                 * Decodes an InboundClassValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundClassValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.InboundClassValue;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.InboundClassValue;

                /**
                 * Creates a plain object from an InboundClassValue message. Also converts values to other types if specified.
                 * @param message InboundClassValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.InboundClassValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                constructor(properties?: baml_bridge.cffi.v1.IInboundEnumValue);

                /** InboundEnumValue name. */
                public name: string;

                /** InboundEnumValue value. */
                public value: string;

                /**
                 * Creates a new InboundEnumValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns InboundEnumValue instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IInboundEnumValue): baml_bridge.cffi.v1.InboundEnumValue;

                /**
                 * Encodes the specified InboundEnumValue message. Does not implicitly {@link baml_bridge.cffi.v1.InboundEnumValue.verify|verify} messages.
                 * @param message InboundEnumValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IInboundEnumValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified InboundEnumValue message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.InboundEnumValue.verify|verify} messages.
                 * @param message InboundEnumValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IInboundEnumValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an InboundEnumValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns InboundEnumValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.InboundEnumValue;

                /**
                 * Decodes an InboundEnumValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns InboundEnumValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.InboundEnumValue;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.InboundEnumValue;

                /**
                 * Creates a plain object from an InboundEnumValue message. Also converts values to other types if specified.
                 * @param message InboundEnumValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.InboundEnumValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** Properties of a BamlTyArg. */
            interface IBamlTyArg {

                /** BamlTyArg typeVar */
                typeVar?: (string|null);

                /** BamlTyArg typeValue */
                typeValue?: (baml_bridge.cffi.v1.IBamlTy|null);
            }

            /** Represents a BamlTyArg. */
            class BamlTyArg implements IBamlTyArg {

                /**
                 * Constructs a new BamlTyArg.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyArg);

                /** BamlTyArg typeVar. */
                public typeVar: string;

                /** BamlTyArg typeValue. */
                public typeValue?: (baml_bridge.cffi.v1.IBamlTy|null);

                /**
                 * Creates a new BamlTyArg instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyArg instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyArg): baml_bridge.cffi.v1.BamlTyArg;

                /**
                 * Encodes the specified BamlTyArg message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyArg.verify|verify} messages.
                 * @param message BamlTyArg message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyArg, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyArg message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyArg.verify|verify} messages.
                 * @param message BamlTyArg message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyArg, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyArg message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyArg
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyArg;

                /**
                 * Decodes a BamlTyArg message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyArg
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyArg;

                /**
                 * Verifies a BamlTyArg message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyArg message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyArg
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyArg;

                /**
                 * Creates a plain object from a BamlTyArg message. Also converts values to other types if specified.
                 * @param message BamlTyArg
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyArg, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyArg to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyArg
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a CallFunctionArgs. */
            interface ICallFunctionArgs {

                /** CallFunctionArgs kwargs */
                kwargs?: (baml_bridge.cffi.v1.IInboundMapEntry[]|null);

                /** CallFunctionArgs callId */
                callId?: (number|Long|null);

                /** CallFunctionArgs typeArgs */
                typeArgs?: (baml_bridge.cffi.v1.IBamlTyArg[]|null);

                /** CallFunctionArgs functionName */
                functionName?: (string|null);

                /** CallFunctionArgs functionHandle */
                functionHandle?: (number|Long|null);
            }

            /** Represents a CallFunctionArgs. */
            class CallFunctionArgs implements ICallFunctionArgs {

                /**
                 * Constructs a new CallFunctionArgs.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.ICallFunctionArgs);

                /** CallFunctionArgs kwargs. */
                public kwargs: baml_bridge.cffi.v1.IInboundMapEntry[];

                /** CallFunctionArgs callId. */
                public callId: (number|Long);

                /** CallFunctionArgs typeArgs. */
                public typeArgs: baml_bridge.cffi.v1.IBamlTyArg[];

                /** CallFunctionArgs functionName. */
                public functionName?: (string|null);

                /** CallFunctionArgs functionHandle. */
                public functionHandle?: (number|Long|null);

                /** CallFunctionArgs callTarget. */
                public callTarget?: ("functionName"|"functionHandle");

                /**
                 * Creates a new CallFunctionArgs instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns CallFunctionArgs instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.ICallFunctionArgs): baml_bridge.cffi.v1.CallFunctionArgs;

                /**
                 * Encodes the specified CallFunctionArgs message. Does not implicitly {@link baml_bridge.cffi.v1.CallFunctionArgs.verify|verify} messages.
                 * @param message CallFunctionArgs message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.ICallFunctionArgs, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified CallFunctionArgs message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.CallFunctionArgs.verify|verify} messages.
                 * @param message CallFunctionArgs message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.ICallFunctionArgs, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CallFunctionArgs message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns CallFunctionArgs
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.CallFunctionArgs;

                /**
                 * Decodes a CallFunctionArgs message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns CallFunctionArgs
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.CallFunctionArgs;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.CallFunctionArgs;

                /**
                 * Creates a plain object from a CallFunctionArgs message. Also converts values to other types if specified.
                 * @param message CallFunctionArgs
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.CallFunctionArgs, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                constructor(properties?: baml_bridge.cffi.v1.ICallAck);

                /** CallAck error. */
                public error?: (string|null);

                /** CallAck response. */
                public response?: "error";

                /**
                 * Creates a new CallAck instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns CallAck instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.ICallAck): baml_bridge.cffi.v1.CallAck;

                /**
                 * Encodes the specified CallAck message. Does not implicitly {@link baml_bridge.cffi.v1.CallAck.verify|verify} messages.
                 * @param message CallAck message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.ICallAck, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified CallAck message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.CallAck.verify|verify} messages.
                 * @param message CallAck message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.ICallAck, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CallAck message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns CallAck
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.CallAck;

                /**
                 * Decodes a CallAck message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns CallAck
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.CallAck;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.CallAck;

                /**
                 * Creates a plain object from a CallAck message. Also converts values to other types if specified.
                 * @param message CallAck
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.CallAck, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                ADT_TAGGED_HEAP_HANDLE = 14,
                HOST_VALUE_CALLABLE = 15,
                HOST_VALUE_OPAQUE = 16
            }

            /** Properties of a BamlHandle. */
            interface IBamlHandle {

                /** BamlHandle key */
                key?: (number|Long|null);

                /** BamlHandle handleType */
                handleType?: (baml_bridge.cffi.v1.BamlHandleType|null);
            }

            /** Represents a BamlHandle. */
            class BamlHandle implements IBamlHandle {

                /**
                 * Constructs a new BamlHandle.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlHandle);

                /** BamlHandle key. */
                public key: (number|Long);

                /** BamlHandle handleType. */
                public handleType: baml_bridge.cffi.v1.BamlHandleType;

                /**
                 * Creates a new BamlHandle instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlHandle instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlHandle): baml_bridge.cffi.v1.BamlHandle;

                /**
                 * Encodes the specified BamlHandle message. Does not implicitly {@link baml_bridge.cffi.v1.BamlHandle.verify|verify} messages.
                 * @param message BamlHandle message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlHandle, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlHandle message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlHandle.verify|verify} messages.
                 * @param message BamlHandle message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlHandle, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlHandle message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlHandle;

                /**
                 * Decodes a BamlHandle message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlHandle;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlHandle;

                /**
                 * Creates a plain object from a BamlHandle message. Also converts values to other types if specified.
                 * @param message BamlHandle
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlHandle, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** Properties of a BamlTy. */
            interface IBamlTy {

                /** BamlTy primitive */
                primitive?: (baml_bridge.cffi.v1.IBamlTyPrimitive|null);

                /** BamlTy classTy */
                classTy?: (baml_bridge.cffi.v1.IBamlTyClass|null);

                /** BamlTy enum */
                "enum"?: (baml_bridge.cffi.v1.IBamlTyEnum|null);

                /** BamlTy list */
                list?: (baml_bridge.cffi.v1.IBamlTyList|null);

                /** BamlTy map */
                map?: (baml_bridge.cffi.v1.IBamlTyMap|null);

                /** BamlTy optional */
                optional?: (baml_bridge.cffi.v1.IBamlTyOptional|null);

                /** BamlTy union */
                union?: (baml_bridge.cffi.v1.IBamlTyUnion|null);

                /** BamlTy literal */
                literal?: (baml_bridge.cffi.v1.IBamlTyLiteral|null);

                /** BamlTy typeAlias */
                typeAlias?: (baml_bridge.cffi.v1.IBamlTyTypeAlias|null);

                /** BamlTy unknown */
                unknown?: (baml_bridge.cffi.v1.IBamlTyUnknown|null);

                /** BamlTy media */
                media?: (baml_bridge.cffi.v1.IBamlTyMedia|null);

                /** BamlTy interface */
                "interface"?: (baml_bridge.cffi.v1.IBamlTyInterface|null);

                /** BamlTy enumVariant */
                enumVariant?: (baml_bridge.cffi.v1.IBamlTyEnumVariant|null);

                /** BamlTy function */
                "function"?: (baml_bridge.cffi.v1.IBamlTyFunction|null);

                /** BamlTy future */
                future?: (baml_bridge.cffi.v1.IBamlTyFuture|null);

                /** BamlTy rustType */
                rustType?: (baml_bridge.cffi.v1.IBamlTyRustType|null);

                /** BamlTy metaType */
                metaType?: (baml_bridge.cffi.v1.IBamlTyMetaType|null);

                /** BamlTy resource */
                resource?: (baml_bridge.cffi.v1.IBamlTyResource|null);

                /** BamlTy promptAst */
                promptAst?: (baml_bridge.cffi.v1.IBamlTyPromptAst|null);

                /** BamlTy void */
                "void"?: (baml_bridge.cffi.v1.IBamlTyVoid|null);

                /** BamlTy typeVar */
                typeVar?: (baml_bridge.cffi.v1.IBamlTyTypeVar|null);

                /** BamlTy associatedTypeProjection */
                associatedTypeProjection?: (baml_bridge.cffi.v1.IBamlTyAssociatedTypeProjection|null);

                /** BamlTy never */
                never?: (baml_bridge.cffi.v1.IBamlTyNever|null);
            }

            /** Represents a BamlTy. */
            class BamlTy implements IBamlTy {

                /**
                 * Constructs a new BamlTy.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTy);

                /** BamlTy primitive. */
                public primitive?: (baml_bridge.cffi.v1.IBamlTyPrimitive|null);

                /** BamlTy classTy. */
                public classTy?: (baml_bridge.cffi.v1.IBamlTyClass|null);

                /** BamlTy enum. */
                public enum?: (baml_bridge.cffi.v1.IBamlTyEnum|null);

                /** BamlTy list. */
                public list?: (baml_bridge.cffi.v1.IBamlTyList|null);

                /** BamlTy map. */
                public map?: (baml_bridge.cffi.v1.IBamlTyMap|null);

                /** BamlTy optional. */
                public optional?: (baml_bridge.cffi.v1.IBamlTyOptional|null);

                /** BamlTy union. */
                public union?: (baml_bridge.cffi.v1.IBamlTyUnion|null);

                /** BamlTy literal. */
                public literal?: (baml_bridge.cffi.v1.IBamlTyLiteral|null);

                /** BamlTy typeAlias. */
                public typeAlias?: (baml_bridge.cffi.v1.IBamlTyTypeAlias|null);

                /** BamlTy unknown. */
                public unknown?: (baml_bridge.cffi.v1.IBamlTyUnknown|null);

                /** BamlTy media. */
                public media?: (baml_bridge.cffi.v1.IBamlTyMedia|null);

                /** BamlTy interface. */
                public interface?: (baml_bridge.cffi.v1.IBamlTyInterface|null);

                /** BamlTy enumVariant. */
                public enumVariant?: (baml_bridge.cffi.v1.IBamlTyEnumVariant|null);

                /** BamlTy function. */
                public function?: (baml_bridge.cffi.v1.IBamlTyFunction|null);

                /** BamlTy future. */
                public future?: (baml_bridge.cffi.v1.IBamlTyFuture|null);

                /** BamlTy rustType. */
                public rustType?: (baml_bridge.cffi.v1.IBamlTyRustType|null);

                /** BamlTy metaType. */
                public metaType?: (baml_bridge.cffi.v1.IBamlTyMetaType|null);

                /** BamlTy resource. */
                public resource?: (baml_bridge.cffi.v1.IBamlTyResource|null);

                /** BamlTy promptAst. */
                public promptAst?: (baml_bridge.cffi.v1.IBamlTyPromptAst|null);

                /** BamlTy void. */
                public void?: (baml_bridge.cffi.v1.IBamlTyVoid|null);

                /** BamlTy typeVar. */
                public typeVar?: (baml_bridge.cffi.v1.IBamlTyTypeVar|null);

                /** BamlTy associatedTypeProjection. */
                public associatedTypeProjection?: (baml_bridge.cffi.v1.IBamlTyAssociatedTypeProjection|null);

                /** BamlTy never. */
                public never?: (baml_bridge.cffi.v1.IBamlTyNever|null);

                /** BamlTy ty. */
                public ty?: ("primitive"|"classTy"|"enum"|"list"|"map"|"optional"|"union"|"literal"|"typeAlias"|"unknown"|"media"|"interface"|"enumVariant"|"Function"|"future"|"rustType"|"metaType"|"resource"|"promptAst"|"void"|"typeVar"|"associatedTypeProjection"|"never");

                /**
                 * Creates a new BamlTy instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTy instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTy): baml_bridge.cffi.v1.BamlTy;

                /**
                 * Encodes the specified BamlTy message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTy.verify|verify} messages.
                 * @param message BamlTy message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTy, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTy message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTy.verify|verify} messages.
                 * @param message BamlTy message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTy, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTy message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTy
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTy;

                /**
                 * Decodes a BamlTy message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTy
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTy;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTy;

                /**
                 * Creates a plain object from a BamlTy message. Also converts values to other types if specified.
                 * @param message BamlTy
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTy, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** BamlTyPrimitiveKind enum. */
            enum BamlTyPrimitiveKind {
                BAML_TY_PRIMITIVE_UNSPECIFIED = 0,
                BAML_TY_PRIMITIVE_STRING = 1,
                BAML_TY_PRIMITIVE_INT = 2,
                BAML_TY_PRIMITIVE_FLOAT = 3,
                BAML_TY_PRIMITIVE_BOOL = 4,
                BAML_TY_PRIMITIVE_NULL = 5,
                BAML_TY_PRIMITIVE_BYTES = 6,
                BAML_TY_PRIMITIVE_BIGINT = 7
            }

            /** Properties of a BamlTyPrimitive. */
            interface IBamlTyPrimitive {

                /** BamlTyPrimitive kind */
                kind?: (baml_bridge.cffi.v1.BamlTyPrimitiveKind|null);
            }

            /** Represents a BamlTyPrimitive. */
            class BamlTyPrimitive implements IBamlTyPrimitive {

                /**
                 * Constructs a new BamlTyPrimitive.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyPrimitive);

                /** BamlTyPrimitive kind. */
                public kind: baml_bridge.cffi.v1.BamlTyPrimitiveKind;

                /**
                 * Creates a new BamlTyPrimitive instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyPrimitive instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyPrimitive): baml_bridge.cffi.v1.BamlTyPrimitive;

                /**
                 * Encodes the specified BamlTyPrimitive message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyPrimitive.verify|verify} messages.
                 * @param message BamlTyPrimitive message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyPrimitive, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyPrimitive message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyPrimitive.verify|verify} messages.
                 * @param message BamlTyPrimitive message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyPrimitive, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyPrimitive message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyPrimitive
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyPrimitive;

                /**
                 * Decodes a BamlTyPrimitive message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyPrimitive
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyPrimitive;

                /**
                 * Verifies a BamlTyPrimitive message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyPrimitive message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyPrimitive
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyPrimitive;

                /**
                 * Creates a plain object from a BamlTyPrimitive message. Also converts values to other types if specified.
                 * @param message BamlTyPrimitive
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyPrimitive, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyPrimitive to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyPrimitive
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyClass. */
            interface IBamlTyClass {

                /** BamlTyClass name */
                name?: (string|null);

                /** BamlTyClass typeArgs */
                typeArgs?: (baml_bridge.cffi.v1.IBamlTy[]|null);
            }

            /** Represents a BamlTyClass. */
            class BamlTyClass implements IBamlTyClass {

                /**
                 * Constructs a new BamlTyClass.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyClass);

                /** BamlTyClass name. */
                public name: string;

                /** BamlTyClass typeArgs. */
                public typeArgs: baml_bridge.cffi.v1.IBamlTy[];

                /**
                 * Creates a new BamlTyClass instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyClass instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyClass): baml_bridge.cffi.v1.BamlTyClass;

                /**
                 * Encodes the specified BamlTyClass message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyClass.verify|verify} messages.
                 * @param message BamlTyClass message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyClass, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyClass message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyClass.verify|verify} messages.
                 * @param message BamlTyClass message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyClass, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyClass message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyClass;

                /**
                 * Decodes a BamlTyClass message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyClass;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyClass;

                /**
                 * Creates a plain object from a BamlTyClass message. Also converts values to other types if specified.
                 * @param message BamlTyClass
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyClass, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                name?: (string|null);

                /** BamlTyTypeAlias typeArgs */
                typeArgs?: (baml_bridge.cffi.v1.IBamlTy[]|null);
            }

            /** Represents a BamlTyTypeAlias. */
            class BamlTyTypeAlias implements IBamlTyTypeAlias {

                /**
                 * Constructs a new BamlTyTypeAlias.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyTypeAlias);

                /** BamlTyTypeAlias name. */
                public name: string;

                /** BamlTyTypeAlias typeArgs. */
                public typeArgs: baml_bridge.cffi.v1.IBamlTy[];

                /**
                 * Creates a new BamlTyTypeAlias instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyTypeAlias instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyTypeAlias): baml_bridge.cffi.v1.BamlTyTypeAlias;

                /**
                 * Encodes the specified BamlTyTypeAlias message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyTypeAlias.verify|verify} messages.
                 * @param message BamlTyTypeAlias message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyTypeAlias, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyTypeAlias message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyTypeAlias.verify|verify} messages.
                 * @param message BamlTyTypeAlias message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyTypeAlias, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyTypeAlias message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyTypeAlias
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyTypeAlias;

                /**
                 * Decodes a BamlTyTypeAlias message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyTypeAlias
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyTypeAlias;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyTypeAlias;

                /**
                 * Creates a plain object from a BamlTyTypeAlias message. Also converts values to other types if specified.
                 * @param message BamlTyTypeAlias
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyTypeAlias, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyEnum);

                /** BamlTyEnum name. */
                public name: string;

                /**
                 * Creates a new BamlTyEnum instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyEnum instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyEnum): baml_bridge.cffi.v1.BamlTyEnum;

                /**
                 * Encodes the specified BamlTyEnum message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyEnum.verify|verify} messages.
                 * @param message BamlTyEnum message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyEnum, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyEnum message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyEnum.verify|verify} messages.
                 * @param message BamlTyEnum message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyEnum, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyEnum message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyEnum;

                /**
                 * Decodes a BamlTyEnum message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyEnum;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyEnum;

                /**
                 * Creates a plain object from a BamlTyEnum message. Also converts values to other types if specified.
                 * @param message BamlTyEnum
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyEnum, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** Properties of a BamlTyList. */
            interface IBamlTyList {

                /** BamlTyList item */
                item?: (baml_bridge.cffi.v1.IBamlTy|null);
            }

            /** Represents a BamlTyList. */
            class BamlTyList implements IBamlTyList {

                /**
                 * Constructs a new BamlTyList.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyList);

                /** BamlTyList item. */
                public item?: (baml_bridge.cffi.v1.IBamlTy|null);

                /**
                 * Creates a new BamlTyList instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyList instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyList): baml_bridge.cffi.v1.BamlTyList;

                /**
                 * Encodes the specified BamlTyList message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyList.verify|verify} messages.
                 * @param message BamlTyList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyList, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyList message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyList.verify|verify} messages.
                 * @param message BamlTyList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyList, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyList message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyList;

                /**
                 * Decodes a BamlTyList message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyList;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyList;

                /**
                 * Creates a plain object from a BamlTyList message. Also converts values to other types if specified.
                 * @param message BamlTyList
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyList, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

                /** BamlTyMap key */
                key?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlTyMap value */
                value?: (baml_bridge.cffi.v1.IBamlTy|null);
            }

            /** Represents a BamlTyMap. */
            class BamlTyMap implements IBamlTyMap {

                /**
                 * Constructs a new BamlTyMap.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyMap);

                /** BamlTyMap key. */
                public key?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlTyMap value. */
                public value?: (baml_bridge.cffi.v1.IBamlTy|null);

                /**
                 * Creates a new BamlTyMap instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyMap instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyMap): baml_bridge.cffi.v1.BamlTyMap;

                /**
                 * Encodes the specified BamlTyMap message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyMap.verify|verify} messages.
                 * @param message BamlTyMap message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyMap, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyMap message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyMap.verify|verify} messages.
                 * @param message BamlTyMap message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyMap, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyMap message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyMap;

                /**
                 * Decodes a BamlTyMap message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyMap;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyMap;

                /**
                 * Creates a plain object from a BamlTyMap message. Also converts values to other types if specified.
                 * @param message BamlTyMap
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyMap, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** Properties of a BamlTyOptional. */
            interface IBamlTyOptional {

                /** BamlTyOptional inner */
                inner?: (baml_bridge.cffi.v1.IBamlTy|null);
            }

            /** Represents a BamlTyOptional. */
            class BamlTyOptional implements IBamlTyOptional {

                /**
                 * Constructs a new BamlTyOptional.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyOptional);

                /** BamlTyOptional inner. */
                public inner?: (baml_bridge.cffi.v1.IBamlTy|null);

                /**
                 * Creates a new BamlTyOptional instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyOptional instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyOptional): baml_bridge.cffi.v1.BamlTyOptional;

                /**
                 * Encodes the specified BamlTyOptional message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyOptional.verify|verify} messages.
                 * @param message BamlTyOptional message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyOptional, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyOptional message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyOptional.verify|verify} messages.
                 * @param message BamlTyOptional message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyOptional, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyOptional message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyOptional
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyOptional;

                /**
                 * Decodes a BamlTyOptional message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyOptional
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyOptional;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyOptional;

                /**
                 * Creates a plain object from a BamlTyOptional message. Also converts values to other types if specified.
                 * @param message BamlTyOptional
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyOptional, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** Properties of a BamlTyUnion. */
            interface IBamlTyUnion {

                /** BamlTyUnion options */
                options?: (baml_bridge.cffi.v1.IBamlTy[]|null);
            }

            /** Represents a BamlTyUnion. */
            class BamlTyUnion implements IBamlTyUnion {

                /**
                 * Constructs a new BamlTyUnion.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyUnion);

                /** BamlTyUnion options. */
                public options: baml_bridge.cffi.v1.IBamlTy[];

                /**
                 * Creates a new BamlTyUnion instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyUnion instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyUnion): baml_bridge.cffi.v1.BamlTyUnion;

                /**
                 * Encodes the specified BamlTyUnion message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyUnion.verify|verify} messages.
                 * @param message BamlTyUnion message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyUnion, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyUnion message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyUnion.verify|verify} messages.
                 * @param message BamlTyUnion message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyUnion, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyUnion message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyUnion
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyUnion;

                /**
                 * Decodes a BamlTyUnion message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyUnion
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyUnion;

                /**
                 * Verifies a BamlTyUnion message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyUnion message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyUnion
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyUnion;

                /**
                 * Creates a plain object from a BamlTyUnion message. Also converts values to other types if specified.
                 * @param message BamlTyUnion
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyUnion, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyUnion to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyUnion
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
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyUnknown);

                /**
                 * Creates a new BamlTyUnknown instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyUnknown instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyUnknown): baml_bridge.cffi.v1.BamlTyUnknown;

                /**
                 * Encodes the specified BamlTyUnknown message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyUnknown.verify|verify} messages.
                 * @param message BamlTyUnknown message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyUnknown, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyUnknown message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyUnknown.verify|verify} messages.
                 * @param message BamlTyUnknown message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyUnknown, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyUnknown message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyUnknown
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyUnknown;

                /**
                 * Decodes a BamlTyUnknown message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyUnknown
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyUnknown;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyUnknown;

                /**
                 * Creates a plain object from a BamlTyUnknown message. Also converts values to other types if specified.
                 * @param message BamlTyUnknown
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyUnknown, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** Properties of a BamlTyLiteral. */
            interface IBamlTyLiteral {

                /** BamlTyLiteral stringValue */
                stringValue?: (string|null);

                /** BamlTyLiteral intValue */
                intValue?: (number|Long|null);

                /** BamlTyLiteral boolValue */
                boolValue?: (boolean|null);

                /** BamlTyLiteral bigintValue */
                bigintValue?: (string|null);

                /** BamlTyLiteral floatValue */
                floatValue?: (string|null);
            }

            /** Represents a BamlTyLiteral. */
            class BamlTyLiteral implements IBamlTyLiteral {

                /**
                 * Constructs a new BamlTyLiteral.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyLiteral);

                /** BamlTyLiteral stringValue. */
                public stringValue?: (string|null);

                /** BamlTyLiteral intValue. */
                public intValue?: (number|Long|null);

                /** BamlTyLiteral boolValue. */
                public boolValue?: (boolean|null);

                /** BamlTyLiteral bigintValue. */
                public bigintValue?: (string|null);

                /** BamlTyLiteral floatValue. */
                public floatValue?: (string|null);

                /** BamlTyLiteral literal. */
                public literal?: ("stringValue"|"intValue"|"boolValue"|"bigintValue"|"floatValue");

                /**
                 * Creates a new BamlTyLiteral instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyLiteral instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyLiteral): baml_bridge.cffi.v1.BamlTyLiteral;

                /**
                 * Encodes the specified BamlTyLiteral message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyLiteral.verify|verify} messages.
                 * @param message BamlTyLiteral message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyLiteral, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyLiteral message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyLiteral.verify|verify} messages.
                 * @param message BamlTyLiteral message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyLiteral, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyLiteral message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyLiteral
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyLiteral;

                /**
                 * Decodes a BamlTyLiteral message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyLiteral
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyLiteral;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyLiteral;

                /**
                 * Creates a plain object from a BamlTyLiteral message. Also converts values to other types if specified.
                 * @param message BamlTyLiteral
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyLiteral, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** BamlTyMediaKind enum. */
            enum BamlTyMediaKind {
                BAML_TY_MEDIA_KIND_UNSPECIFIED = 0,
                BAML_TY_MEDIA_KIND_IMAGE = 1,
                BAML_TY_MEDIA_KIND_AUDIO = 2,
                BAML_TY_MEDIA_KIND_VIDEO = 3,
                BAML_TY_MEDIA_KIND_PDF = 4,
                BAML_TY_MEDIA_KIND_GENERIC = 5
            }

            /** Properties of a BamlTyMedia. */
            interface IBamlTyMedia {

                /** BamlTyMedia kind */
                kind?: (baml_bridge.cffi.v1.BamlTyMediaKind|null);
            }

            /** Represents a BamlTyMedia. */
            class BamlTyMedia implements IBamlTyMedia {

                /**
                 * Constructs a new BamlTyMedia.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyMedia);

                /** BamlTyMedia kind. */
                public kind: baml_bridge.cffi.v1.BamlTyMediaKind;

                /**
                 * Creates a new BamlTyMedia instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyMedia instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyMedia): baml_bridge.cffi.v1.BamlTyMedia;

                /**
                 * Encodes the specified BamlTyMedia message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyMedia.verify|verify} messages.
                 * @param message BamlTyMedia message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyMedia, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyMedia message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyMedia.verify|verify} messages.
                 * @param message BamlTyMedia message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyMedia, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyMedia message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyMedia;

                /**
                 * Decodes a BamlTyMedia message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyMedia;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyMedia;

                /**
                 * Creates a plain object from a BamlTyMedia message. Also converts values to other types if specified.
                 * @param message BamlTyMedia
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyMedia, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** Properties of a BamlTyInterface. */
            interface IBamlTyInterface {

                /** BamlTyInterface name */
                name?: (string|null);

                /** BamlTyInterface typeArgs */
                typeArgs?: (baml_bridge.cffi.v1.IBamlTy[]|null);

                /** BamlTyInterface bindings */
                bindings?: (baml_bridge.cffi.v1.IBamlTyAssociatedBinding[]|null);
            }

            /** Represents a BamlTyInterface. */
            class BamlTyInterface implements IBamlTyInterface {

                /**
                 * Constructs a new BamlTyInterface.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyInterface);

                /** BamlTyInterface name. */
                public name: string;

                /** BamlTyInterface typeArgs. */
                public typeArgs: baml_bridge.cffi.v1.IBamlTy[];

                /** BamlTyInterface bindings. */
                public bindings: baml_bridge.cffi.v1.IBamlTyAssociatedBinding[];

                /**
                 * Creates a new BamlTyInterface instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyInterface instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyInterface): baml_bridge.cffi.v1.BamlTyInterface;

                /**
                 * Encodes the specified BamlTyInterface message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyInterface.verify|verify} messages.
                 * @param message BamlTyInterface message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyInterface, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyInterface message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyInterface.verify|verify} messages.
                 * @param message BamlTyInterface message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyInterface, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyInterface message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyInterface
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyInterface;

                /**
                 * Decodes a BamlTyInterface message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyInterface
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyInterface;

                /**
                 * Verifies a BamlTyInterface message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyInterface message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyInterface
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyInterface;

                /**
                 * Creates a plain object from a BamlTyInterface message. Also converts values to other types if specified.
                 * @param message BamlTyInterface
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyInterface, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyInterface to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyInterface
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyAssociatedBinding. */
            interface IBamlTyAssociatedBinding {

                /** BamlTyAssociatedBinding name */
                name?: (string|null);

                /** BamlTyAssociatedBinding ty */
                ty?: (baml_bridge.cffi.v1.IBamlTy|null);
            }

            /** Represents a BamlTyAssociatedBinding. */
            class BamlTyAssociatedBinding implements IBamlTyAssociatedBinding {

                /**
                 * Constructs a new BamlTyAssociatedBinding.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyAssociatedBinding);

                /** BamlTyAssociatedBinding name. */
                public name: string;

                /** BamlTyAssociatedBinding ty. */
                public ty?: (baml_bridge.cffi.v1.IBamlTy|null);

                /**
                 * Creates a new BamlTyAssociatedBinding instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyAssociatedBinding instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyAssociatedBinding): baml_bridge.cffi.v1.BamlTyAssociatedBinding;

                /**
                 * Encodes the specified BamlTyAssociatedBinding message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyAssociatedBinding.verify|verify} messages.
                 * @param message BamlTyAssociatedBinding message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyAssociatedBinding, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyAssociatedBinding message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyAssociatedBinding.verify|verify} messages.
                 * @param message BamlTyAssociatedBinding message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyAssociatedBinding, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyAssociatedBinding message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyAssociatedBinding
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyAssociatedBinding;

                /**
                 * Decodes a BamlTyAssociatedBinding message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyAssociatedBinding
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyAssociatedBinding;

                /**
                 * Verifies a BamlTyAssociatedBinding message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyAssociatedBinding message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyAssociatedBinding
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyAssociatedBinding;

                /**
                 * Creates a plain object from a BamlTyAssociatedBinding message. Also converts values to other types if specified.
                 * @param message BamlTyAssociatedBinding
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyAssociatedBinding, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyAssociatedBinding to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyAssociatedBinding
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyEnumVariant. */
            interface IBamlTyEnumVariant {

                /** BamlTyEnumVariant name */
                name?: (string|null);

                /** BamlTyEnumVariant variant */
                variant?: (string|null);
            }

            /** Represents a BamlTyEnumVariant. */
            class BamlTyEnumVariant implements IBamlTyEnumVariant {

                /**
                 * Constructs a new BamlTyEnumVariant.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyEnumVariant);

                /** BamlTyEnumVariant name. */
                public name: string;

                /** BamlTyEnumVariant variant. */
                public variant: string;

                /**
                 * Creates a new BamlTyEnumVariant instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyEnumVariant instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyEnumVariant): baml_bridge.cffi.v1.BamlTyEnumVariant;

                /**
                 * Encodes the specified BamlTyEnumVariant message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyEnumVariant.verify|verify} messages.
                 * @param message BamlTyEnumVariant message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyEnumVariant, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyEnumVariant message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyEnumVariant.verify|verify} messages.
                 * @param message BamlTyEnumVariant message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyEnumVariant, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyEnumVariant message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyEnumVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyEnumVariant;

                /**
                 * Decodes a BamlTyEnumVariant message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyEnumVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyEnumVariant;

                /**
                 * Verifies a BamlTyEnumVariant message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyEnumVariant message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyEnumVariant
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyEnumVariant;

                /**
                 * Creates a plain object from a BamlTyEnumVariant message. Also converts values to other types if specified.
                 * @param message BamlTyEnumVariant
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyEnumVariant, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyEnumVariant to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyEnumVariant
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** BamlTyFunctionParamMode enum. */
            enum BamlTyFunctionParamMode {
                BAML_TY_FUNCTION_PARAM_MODE_UNSPECIFIED = 0,
                BAML_TY_FUNCTION_PARAM_MODE_REQUIRED = 1,
                BAML_TY_FUNCTION_PARAM_MODE_OPTIONAL = 2
            }

            /** Properties of a BamlTyFunctionParam. */
            interface IBamlTyFunctionParam {

                /** BamlTyFunctionParam name */
                name?: (string|null);

                /** BamlTyFunctionParam ty */
                ty?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlTyFunctionParam mode */
                mode?: (baml_bridge.cffi.v1.BamlTyFunctionParamMode|null);
            }

            /** Represents a BamlTyFunctionParam. */
            class BamlTyFunctionParam implements IBamlTyFunctionParam {

                /**
                 * Constructs a new BamlTyFunctionParam.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyFunctionParam);

                /** BamlTyFunctionParam name. */
                public name?: (string|null);

                /** BamlTyFunctionParam ty. */
                public ty?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlTyFunctionParam mode. */
                public mode: baml_bridge.cffi.v1.BamlTyFunctionParamMode;

                /**
                 * Creates a new BamlTyFunctionParam instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyFunctionParam instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyFunctionParam): baml_bridge.cffi.v1.BamlTyFunctionParam;

                /**
                 * Encodes the specified BamlTyFunctionParam message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyFunctionParam.verify|verify} messages.
                 * @param message BamlTyFunctionParam message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyFunctionParam, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyFunctionParam message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyFunctionParam.verify|verify} messages.
                 * @param message BamlTyFunctionParam message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyFunctionParam, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyFunctionParam message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyFunctionParam
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyFunctionParam;

                /**
                 * Decodes a BamlTyFunctionParam message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyFunctionParam
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyFunctionParam;

                /**
                 * Verifies a BamlTyFunctionParam message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyFunctionParam message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyFunctionParam
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyFunctionParam;

                /**
                 * Creates a plain object from a BamlTyFunctionParam message. Also converts values to other types if specified.
                 * @param message BamlTyFunctionParam
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyFunctionParam, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyFunctionParam to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyFunctionParam
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyFunction. */
            interface IBamlTyFunction {

                /** BamlTyFunction genericParams */
                genericParams?: (string[]|null);

                /** BamlTyFunction params */
                params?: (baml_bridge.cffi.v1.IBamlTyFunctionParam[]|null);

                /** BamlTyFunction ret */
                ret?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlTyFunction throws */
                throws?: (baml_bridge.cffi.v1.IBamlTy|null);
            }

            /** Represents a BamlTyFunction. */
            class BamlTyFunction implements IBamlTyFunction {

                /**
                 * Constructs a new BamlTyFunction.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyFunction);

                /** BamlTyFunction genericParams. */
                public genericParams: string[];

                /** BamlTyFunction params. */
                public params: baml_bridge.cffi.v1.IBamlTyFunctionParam[];

                /** BamlTyFunction ret. */
                public ret?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlTyFunction throws. */
                public throws?: (baml_bridge.cffi.v1.IBamlTy|null);

                /**
                 * Creates a new BamlTyFunction instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyFunction instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyFunction): baml_bridge.cffi.v1.BamlTyFunction;

                /**
                 * Encodes the specified BamlTyFunction message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyFunction.verify|verify} messages.
                 * @param message BamlTyFunction message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyFunction, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyFunction message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyFunction.verify|verify} messages.
                 * @param message BamlTyFunction message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyFunction, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyFunction message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyFunction
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyFunction;

                /**
                 * Decodes a BamlTyFunction message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyFunction
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyFunction;

                /**
                 * Verifies a BamlTyFunction message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyFunction message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyFunction
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyFunction;

                /**
                 * Creates a plain object from a BamlTyFunction message. Also converts values to other types if specified.
                 * @param message BamlTyFunction
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyFunction, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyFunction to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyFunction
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyFuture. */
            interface IBamlTyFuture {

                /** BamlTyFuture value */
                value?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlTyFuture error */
                error?: (baml_bridge.cffi.v1.IBamlTy|null);
            }

            /** Represents a BamlTyFuture. */
            class BamlTyFuture implements IBamlTyFuture {

                /**
                 * Constructs a new BamlTyFuture.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyFuture);

                /** BamlTyFuture value. */
                public value?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlTyFuture error. */
                public error?: (baml_bridge.cffi.v1.IBamlTy|null);

                /**
                 * Creates a new BamlTyFuture instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyFuture instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyFuture): baml_bridge.cffi.v1.BamlTyFuture;

                /**
                 * Encodes the specified BamlTyFuture message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyFuture.verify|verify} messages.
                 * @param message BamlTyFuture message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyFuture, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyFuture message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyFuture.verify|verify} messages.
                 * @param message BamlTyFuture message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyFuture, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyFuture message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyFuture
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyFuture;

                /**
                 * Decodes a BamlTyFuture message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyFuture
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyFuture;

                /**
                 * Verifies a BamlTyFuture message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyFuture message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyFuture
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyFuture;

                /**
                 * Creates a plain object from a BamlTyFuture message. Also converts values to other types if specified.
                 * @param message BamlTyFuture
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyFuture, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyFuture to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyFuture
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyRustType. */
            interface IBamlTyRustType {
            }

            /** Represents a BamlTyRustType. */
            class BamlTyRustType implements IBamlTyRustType {

                /**
                 * Constructs a new BamlTyRustType.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyRustType);

                /**
                 * Creates a new BamlTyRustType instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyRustType instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyRustType): baml_bridge.cffi.v1.BamlTyRustType;

                /**
                 * Encodes the specified BamlTyRustType message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyRustType.verify|verify} messages.
                 * @param message BamlTyRustType message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyRustType, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyRustType message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyRustType.verify|verify} messages.
                 * @param message BamlTyRustType message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyRustType, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyRustType message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyRustType
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyRustType;

                /**
                 * Decodes a BamlTyRustType message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyRustType
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyRustType;

                /**
                 * Verifies a BamlTyRustType message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyRustType message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyRustType
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyRustType;

                /**
                 * Creates a plain object from a BamlTyRustType message. Also converts values to other types if specified.
                 * @param message BamlTyRustType
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyRustType, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyRustType to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyRustType
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyMetaType. */
            interface IBamlTyMetaType {
            }

            /** Represents a BamlTyMetaType. */
            class BamlTyMetaType implements IBamlTyMetaType {

                /**
                 * Constructs a new BamlTyMetaType.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyMetaType);

                /**
                 * Creates a new BamlTyMetaType instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyMetaType instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyMetaType): baml_bridge.cffi.v1.BamlTyMetaType;

                /**
                 * Encodes the specified BamlTyMetaType message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyMetaType.verify|verify} messages.
                 * @param message BamlTyMetaType message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyMetaType, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyMetaType message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyMetaType.verify|verify} messages.
                 * @param message BamlTyMetaType message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyMetaType, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyMetaType message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyMetaType
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyMetaType;

                /**
                 * Decodes a BamlTyMetaType message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyMetaType
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyMetaType;

                /**
                 * Verifies a BamlTyMetaType message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyMetaType message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyMetaType
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyMetaType;

                /**
                 * Creates a plain object from a BamlTyMetaType message. Also converts values to other types if specified.
                 * @param message BamlTyMetaType
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyMetaType, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyMetaType to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyMetaType
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyResource. */
            interface IBamlTyResource {
            }

            /** Represents a BamlTyResource. */
            class BamlTyResource implements IBamlTyResource {

                /**
                 * Constructs a new BamlTyResource.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyResource);

                /**
                 * Creates a new BamlTyResource instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyResource instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyResource): baml_bridge.cffi.v1.BamlTyResource;

                /**
                 * Encodes the specified BamlTyResource message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyResource.verify|verify} messages.
                 * @param message BamlTyResource message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyResource, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyResource message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyResource.verify|verify} messages.
                 * @param message BamlTyResource message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyResource, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyResource message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyResource
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyResource;

                /**
                 * Decodes a BamlTyResource message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyResource
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyResource;

                /**
                 * Verifies a BamlTyResource message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyResource message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyResource
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyResource;

                /**
                 * Creates a plain object from a BamlTyResource message. Also converts values to other types if specified.
                 * @param message BamlTyResource
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyResource, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyResource to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyResource
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyPromptAst. */
            interface IBamlTyPromptAst {
            }

            /** Represents a BamlTyPromptAst. */
            class BamlTyPromptAst implements IBamlTyPromptAst {

                /**
                 * Constructs a new BamlTyPromptAst.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyPromptAst);

                /**
                 * Creates a new BamlTyPromptAst instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyPromptAst instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyPromptAst): baml_bridge.cffi.v1.BamlTyPromptAst;

                /**
                 * Encodes the specified BamlTyPromptAst message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyPromptAst.verify|verify} messages.
                 * @param message BamlTyPromptAst message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyPromptAst, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyPromptAst message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyPromptAst.verify|verify} messages.
                 * @param message BamlTyPromptAst message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyPromptAst, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyPromptAst message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyPromptAst
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyPromptAst;

                /**
                 * Decodes a BamlTyPromptAst message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyPromptAst
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyPromptAst;

                /**
                 * Verifies a BamlTyPromptAst message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyPromptAst message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyPromptAst
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyPromptAst;

                /**
                 * Creates a plain object from a BamlTyPromptAst message. Also converts values to other types if specified.
                 * @param message BamlTyPromptAst
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyPromptAst, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyPromptAst to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyPromptAst
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyVoid. */
            interface IBamlTyVoid {
            }

            /** Represents a BamlTyVoid. */
            class BamlTyVoid implements IBamlTyVoid {

                /**
                 * Constructs a new BamlTyVoid.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyVoid);

                /**
                 * Creates a new BamlTyVoid instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyVoid instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyVoid): baml_bridge.cffi.v1.BamlTyVoid;

                /**
                 * Encodes the specified BamlTyVoid message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyVoid.verify|verify} messages.
                 * @param message BamlTyVoid message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyVoid, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyVoid message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyVoid.verify|verify} messages.
                 * @param message BamlTyVoid message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyVoid, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyVoid message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyVoid
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyVoid;

                /**
                 * Decodes a BamlTyVoid message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyVoid
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyVoid;

                /**
                 * Verifies a BamlTyVoid message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyVoid message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyVoid
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyVoid;

                /**
                 * Creates a plain object from a BamlTyVoid message. Also converts values to other types if specified.
                 * @param message BamlTyVoid
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyVoid, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyVoid to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyVoid
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyTypeVar. */
            interface IBamlTyTypeVar {

                /** BamlTyTypeVar name */
                name?: (string|null);

                /** BamlTyTypeVar index */
                index?: (number|null);
            }

            /** Represents a BamlTyTypeVar. */
            class BamlTyTypeVar implements IBamlTyTypeVar {

                /**
                 * Constructs a new BamlTyTypeVar.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyTypeVar);

                /** BamlTyTypeVar name. */
                public name: string;

                /** BamlTyTypeVar index. */
                public index: number;

                /**
                 * Creates a new BamlTyTypeVar instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyTypeVar instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyTypeVar): baml_bridge.cffi.v1.BamlTyTypeVar;

                /**
                 * Encodes the specified BamlTyTypeVar message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyTypeVar.verify|verify} messages.
                 * @param message BamlTyTypeVar message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyTypeVar, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyTypeVar message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyTypeVar.verify|verify} messages.
                 * @param message BamlTyTypeVar message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyTypeVar, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyTypeVar message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyTypeVar
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyTypeVar;

                /**
                 * Decodes a BamlTyTypeVar message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyTypeVar
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyTypeVar;

                /**
                 * Verifies a BamlTyTypeVar message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyTypeVar message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyTypeVar
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyTypeVar;

                /**
                 * Creates a plain object from a BamlTyTypeVar message. Also converts values to other types if specified.
                 * @param message BamlTyTypeVar
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyTypeVar, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyTypeVar to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyTypeVar
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyAssociatedTypeProjection. */
            interface IBamlTyAssociatedTypeProjection {

                /** BamlTyAssociatedTypeProjection base */
                base?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlTyAssociatedTypeProjection interface */
                "interface"?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlTyAssociatedTypeProjection member */
                member?: (string|null);
            }

            /** Represents a BamlTyAssociatedTypeProjection. */
            class BamlTyAssociatedTypeProjection implements IBamlTyAssociatedTypeProjection {

                /**
                 * Constructs a new BamlTyAssociatedTypeProjection.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyAssociatedTypeProjection);

                /** BamlTyAssociatedTypeProjection base. */
                public base?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlTyAssociatedTypeProjection interface. */
                public interface?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlTyAssociatedTypeProjection member. */
                public member: string;

                /**
                 * Creates a new BamlTyAssociatedTypeProjection instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyAssociatedTypeProjection instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyAssociatedTypeProjection): baml_bridge.cffi.v1.BamlTyAssociatedTypeProjection;

                /**
                 * Encodes the specified BamlTyAssociatedTypeProjection message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyAssociatedTypeProjection.verify|verify} messages.
                 * @param message BamlTyAssociatedTypeProjection message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyAssociatedTypeProjection, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyAssociatedTypeProjection message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyAssociatedTypeProjection.verify|verify} messages.
                 * @param message BamlTyAssociatedTypeProjection message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyAssociatedTypeProjection, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyAssociatedTypeProjection message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyAssociatedTypeProjection
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyAssociatedTypeProjection;

                /**
                 * Decodes a BamlTyAssociatedTypeProjection message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyAssociatedTypeProjection
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyAssociatedTypeProjection;

                /**
                 * Verifies a BamlTyAssociatedTypeProjection message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyAssociatedTypeProjection message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyAssociatedTypeProjection
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyAssociatedTypeProjection;

                /**
                 * Creates a plain object from a BamlTyAssociatedTypeProjection message. Also converts values to other types if specified.
                 * @param message BamlTyAssociatedTypeProjection
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyAssociatedTypeProjection, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyAssociatedTypeProjection to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyAssociatedTypeProjection
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlTyNever. */
            interface IBamlTyNever {
            }

            /** Represents a BamlTyNever. */
            class BamlTyNever implements IBamlTyNever {

                /**
                 * Constructs a new BamlTyNever.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlTyNever);

                /**
                 * Creates a new BamlTyNever instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlTyNever instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlTyNever): baml_bridge.cffi.v1.BamlTyNever;

                /**
                 * Encodes the specified BamlTyNever message. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyNever.verify|verify} messages.
                 * @param message BamlTyNever message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlTyNever, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlTyNever message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlTyNever.verify|verify} messages.
                 * @param message BamlTyNever message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlTyNever, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlTyNever message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlTyNever
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlTyNever;

                /**
                 * Decodes a BamlTyNever message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlTyNever
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlTyNever;

                /**
                 * Verifies a BamlTyNever message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlTyNever message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlTyNever
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlTyNever;

                /**
                 * Creates a plain object from a BamlTyNever message. Also converts values to other types if specified.
                 * @param message BamlTyNever
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlTyNever, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlTyNever to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlTyNever
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlOutboundResult. */
            interface IBamlOutboundResult {

                /** BamlOutboundResult ok */
                ok?: (baml_bridge.cffi.v1.IBamlOutboundValue|null);

                /** BamlOutboundResult error */
                error?: (baml_bridge.cffi.v1.IBamlOutboundError|null);

                /** BamlOutboundResult panic */
                panic?: (baml_bridge.cffi.v1.IBamlOutboundPanic|null);
            }

            /** Represents a BamlOutboundResult. */
            class BamlOutboundResult implements IBamlOutboundResult {

                /**
                 * Constructs a new BamlOutboundResult.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlOutboundResult);

                /** BamlOutboundResult ok. */
                public ok?: (baml_bridge.cffi.v1.IBamlOutboundValue|null);

                /** BamlOutboundResult error. */
                public error?: (baml_bridge.cffi.v1.IBamlOutboundError|null);

                /** BamlOutboundResult panic. */
                public panic?: (baml_bridge.cffi.v1.IBamlOutboundPanic|null);

                /** BamlOutboundResult result. */
                public result?: ("ok"|"error"|"panic");

                /**
                 * Creates a new BamlOutboundResult instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlOutboundResult instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlOutboundResult): baml_bridge.cffi.v1.BamlOutboundResult;

                /**
                 * Encodes the specified BamlOutboundResult message. Does not implicitly {@link baml_bridge.cffi.v1.BamlOutboundResult.verify|verify} messages.
                 * @param message BamlOutboundResult message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlOutboundResult, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlOutboundResult message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlOutboundResult.verify|verify} messages.
                 * @param message BamlOutboundResult message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlOutboundResult, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlOutboundResult message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlOutboundResult
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlOutboundResult;

                /**
                 * Decodes a BamlOutboundResult message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlOutboundResult
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlOutboundResult;

                /**
                 * Verifies a BamlOutboundResult message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlOutboundResult message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlOutboundResult
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlOutboundResult;

                /**
                 * Creates a plain object from a BamlOutboundResult message. Also converts values to other types if specified.
                 * @param message BamlOutboundResult
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlOutboundResult, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlOutboundResult to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlOutboundResult
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlOutboundError. */
            interface IBamlOutboundError {

                /** BamlOutboundError value */
                value?: (baml_bridge.cffi.v1.IBamlOutboundValue|null);

                /** BamlOutboundError trace */
                trace?: (string[]|null);
            }

            /** Represents a BamlOutboundError. */
            class BamlOutboundError implements IBamlOutboundError {

                /**
                 * Constructs a new BamlOutboundError.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlOutboundError);

                /** BamlOutboundError value. */
                public value?: (baml_bridge.cffi.v1.IBamlOutboundValue|null);

                /** BamlOutboundError trace. */
                public trace: string[];

                /**
                 * Creates a new BamlOutboundError instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlOutboundError instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlOutboundError): baml_bridge.cffi.v1.BamlOutboundError;

                /**
                 * Encodes the specified BamlOutboundError message. Does not implicitly {@link baml_bridge.cffi.v1.BamlOutboundError.verify|verify} messages.
                 * @param message BamlOutboundError message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlOutboundError, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlOutboundError message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlOutboundError.verify|verify} messages.
                 * @param message BamlOutboundError message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlOutboundError, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlOutboundError message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlOutboundError
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlOutboundError;

                /**
                 * Decodes a BamlOutboundError message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlOutboundError
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlOutboundError;

                /**
                 * Verifies a BamlOutboundError message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlOutboundError message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlOutboundError
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlOutboundError;

                /**
                 * Creates a plain object from a BamlOutboundError message. Also converts values to other types if specified.
                 * @param message BamlOutboundError
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlOutboundError, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlOutboundError to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlOutboundError
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlOutboundPanic. */
            interface IBamlOutboundPanic {

                /** BamlOutboundPanic value */
                value?: (baml_bridge.cffi.v1.IBamlOutboundValue|null);

                /** BamlOutboundPanic trace */
                trace?: (string[]|null);

                /** BamlOutboundPanic isExitPanic */
                isExitPanic?: (boolean|null);

                /** BamlOutboundPanic exitCode */
                exitCode?: (number|Long|null);
            }

            /** Represents a BamlOutboundPanic. */
            class BamlOutboundPanic implements IBamlOutboundPanic {

                /**
                 * Constructs a new BamlOutboundPanic.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlOutboundPanic);

                /** BamlOutboundPanic value. */
                public value?: (baml_bridge.cffi.v1.IBamlOutboundValue|null);

                /** BamlOutboundPanic trace. */
                public trace: string[];

                /** BamlOutboundPanic isExitPanic. */
                public isExitPanic: boolean;

                /** BamlOutboundPanic exitCode. */
                public exitCode: (number|Long);

                /**
                 * Creates a new BamlOutboundPanic instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlOutboundPanic instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlOutboundPanic): baml_bridge.cffi.v1.BamlOutboundPanic;

                /**
                 * Encodes the specified BamlOutboundPanic message. Does not implicitly {@link baml_bridge.cffi.v1.BamlOutboundPanic.verify|verify} messages.
                 * @param message BamlOutboundPanic message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlOutboundPanic, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlOutboundPanic message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlOutboundPanic.verify|verify} messages.
                 * @param message BamlOutboundPanic message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlOutboundPanic, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlOutboundPanic message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlOutboundPanic
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlOutboundPanic;

                /**
                 * Decodes a BamlOutboundPanic message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlOutboundPanic
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlOutboundPanic;

                /**
                 * Verifies a BamlOutboundPanic message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlOutboundPanic message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlOutboundPanic
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlOutboundPanic;

                /**
                 * Creates a plain object from a BamlOutboundPanic message. Also converts values to other types if specified.
                 * @param message BamlOutboundPanic
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlOutboundPanic, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlOutboundPanic to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlOutboundPanic
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlOutboundValue. */
            interface IBamlOutboundValue {

                /** BamlOutboundValue nullValue */
                nullValue?: (baml_bridge.cffi.v1.IBamlValueNull|null);

                /** BamlOutboundValue stringValue */
                stringValue?: (string|null);

                /** BamlOutboundValue intValue */
                intValue?: (number|Long|null);

                /** BamlOutboundValue floatValue */
                floatValue?: (number|null);

                /** BamlOutboundValue boolValue */
                boolValue?: (boolean|null);

                /** BamlOutboundValue classValue */
                classValue?: (baml_bridge.cffi.v1.IBamlValueClass|null);

                /** BamlOutboundValue enumValue */
                enumValue?: (baml_bridge.cffi.v1.IBamlValueEnum|null);

                /** BamlOutboundValue literalValue */
                literalValue?: (baml_bridge.cffi.v1.IBamlLiteralValue|null);

                /** BamlOutboundValue listValue */
                listValue?: (baml_bridge.cffi.v1.IBamlValueList|null);

                /** BamlOutboundValue mapValue */
                mapValue?: (baml_bridge.cffi.v1.IBamlValueMap|null);

                /** BamlOutboundValue unionVariantValue */
                unionVariantValue?: (baml_bridge.cffi.v1.IBamlValueUnionVariant|null);

                /** BamlOutboundValue handleValue */
                handleValue?: (baml_bridge.cffi.v1.IBamlOutboundHandle|null);

                /** BamlOutboundValue mediaValue */
                mediaValue?: (baml_bridge.cffi.v1.IBamlValueMedia|null);

                /** BamlOutboundValue promptAstValue */
                promptAstValue?: (baml_bridge.cffi.v1.IBamlValuePromptAst|null);

                /** BamlOutboundValue uint8arrayValue */
                uint8arrayValue?: (Uint8Array|null);

                /** BamlOutboundValue bigintValue */
                bigintValue?: (string|null);

                /** BamlOutboundValue tyValue */
                tyValue?: (baml_bridge.cffi.v1.IBamlTy|null);
            }

            /** Represents a BamlOutboundValue. */
            class BamlOutboundValue implements IBamlOutboundValue {

                /**
                 * Constructs a new BamlOutboundValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlOutboundValue);

                /** BamlOutboundValue nullValue. */
                public nullValue?: (baml_bridge.cffi.v1.IBamlValueNull|null);

                /** BamlOutboundValue stringValue. */
                public stringValue?: (string|null);

                /** BamlOutboundValue intValue. */
                public intValue?: (number|Long|null);

                /** BamlOutboundValue floatValue. */
                public floatValue?: (number|null);

                /** BamlOutboundValue boolValue. */
                public boolValue?: (boolean|null);

                /** BamlOutboundValue classValue. */
                public classValue?: (baml_bridge.cffi.v1.IBamlValueClass|null);

                /** BamlOutboundValue enumValue. */
                public enumValue?: (baml_bridge.cffi.v1.IBamlValueEnum|null);

                /** BamlOutboundValue literalValue. */
                public literalValue?: (baml_bridge.cffi.v1.IBamlLiteralValue|null);

                /** BamlOutboundValue listValue. */
                public listValue?: (baml_bridge.cffi.v1.IBamlValueList|null);

                /** BamlOutboundValue mapValue. */
                public mapValue?: (baml_bridge.cffi.v1.IBamlValueMap|null);

                /** BamlOutboundValue unionVariantValue. */
                public unionVariantValue?: (baml_bridge.cffi.v1.IBamlValueUnionVariant|null);

                /** BamlOutboundValue handleValue. */
                public handleValue?: (baml_bridge.cffi.v1.IBamlOutboundHandle|null);

                /** BamlOutboundValue mediaValue. */
                public mediaValue?: (baml_bridge.cffi.v1.IBamlValueMedia|null);

                /** BamlOutboundValue promptAstValue. */
                public promptAstValue?: (baml_bridge.cffi.v1.IBamlValuePromptAst|null);

                /** BamlOutboundValue uint8arrayValue. */
                public uint8arrayValue?: (Uint8Array|null);

                /** BamlOutboundValue bigintValue. */
                public bigintValue?: (string|null);

                /** BamlOutboundValue tyValue. */
                public tyValue?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlOutboundValue value. */
                public value?: ("nullValue"|"stringValue"|"intValue"|"floatValue"|"boolValue"|"classValue"|"enumValue"|"literalValue"|"listValue"|"mapValue"|"unionVariantValue"|"handleValue"|"mediaValue"|"promptAstValue"|"uint8arrayValue"|"bigintValue"|"tyValue");

                /**
                 * Creates a new BamlOutboundValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlOutboundValue instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlOutboundValue): baml_bridge.cffi.v1.BamlOutboundValue;

                /**
                 * Encodes the specified BamlOutboundValue message. Does not implicitly {@link baml_bridge.cffi.v1.BamlOutboundValue.verify|verify} messages.
                 * @param message BamlOutboundValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlOutboundValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlOutboundValue message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlOutboundValue.verify|verify} messages.
                 * @param message BamlOutboundValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlOutboundValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlOutboundValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlOutboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlOutboundValue;

                /**
                 * Decodes a BamlOutboundValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlOutboundValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlOutboundValue;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlOutboundValue;

                /**
                 * Creates a plain object from a BamlOutboundValue message. Also converts values to other types if specified.
                 * @param message BamlOutboundValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlOutboundValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                handleType?: (baml_bridge.cffi.v1.BamlHandleType|null);

                /** BamlOutboundHandle ty */
                ty?: (baml_bridge.cffi.v1.IBamlTy|null);
            }

            /** Represents a BamlOutboundHandle. */
            class BamlOutboundHandle implements IBamlOutboundHandle {

                /**
                 * Constructs a new BamlOutboundHandle.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlOutboundHandle);

                /** BamlOutboundHandle key. */
                public key: (number|Long);

                /** BamlOutboundHandle handleType. */
                public handleType: baml_bridge.cffi.v1.BamlHandleType;

                /** BamlOutboundHandle ty. */
                public ty?: (baml_bridge.cffi.v1.IBamlTy|null);

                /**
                 * Creates a new BamlOutboundHandle instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlOutboundHandle instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlOutboundHandle): baml_bridge.cffi.v1.BamlOutboundHandle;

                /**
                 * Encodes the specified BamlOutboundHandle message. Does not implicitly {@link baml_bridge.cffi.v1.BamlOutboundHandle.verify|verify} messages.
                 * @param message BamlOutboundHandle message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlOutboundHandle, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlOutboundHandle message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlOutboundHandle.verify|verify} messages.
                 * @param message BamlOutboundHandle message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlOutboundHandle, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlOutboundHandle message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlOutboundHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlOutboundHandle;

                /**
                 * Decodes a BamlOutboundHandle message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlOutboundHandle
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlOutboundHandle;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlOutboundHandle;

                /**
                 * Creates a plain object from a BamlOutboundHandle message. Also converts values to other types if specified.
                 * @param message BamlOutboundHandle
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlOutboundHandle, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** Properties of a BamlValueNull. */
            interface IBamlValueNull {
            }

            /** Represents a BamlValueNull. */
            class BamlValueNull implements IBamlValueNull {

                /**
                 * Constructs a new BamlValueNull.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlValueNull);

                /**
                 * Creates a new BamlValueNull instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueNull instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlValueNull): baml_bridge.cffi.v1.BamlValueNull;

                /**
                 * Encodes the specified BamlValueNull message. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueNull.verify|verify} messages.
                 * @param message BamlValueNull message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlValueNull, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueNull message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueNull.verify|verify} messages.
                 * @param message BamlValueNull message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlValueNull, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueNull message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlValueNull;

                /**
                 * Decodes a BamlValueNull message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueNull
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlValueNull;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlValueNull;

                /**
                 * Creates a plain object from a BamlValueNull message. Also converts values to other types if specified.
                 * @param message BamlValueNull
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlValueNull, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                itemType?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlValueList items */
                items?: (baml_bridge.cffi.v1.IBamlOutboundValue[]|null);
            }

            /** Represents a BamlValueList. */
            class BamlValueList implements IBamlValueList {

                /**
                 * Constructs a new BamlValueList.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlValueList);

                /** BamlValueList itemType. */
                public itemType?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlValueList items. */
                public items: baml_bridge.cffi.v1.IBamlOutboundValue[];

                /**
                 * Creates a new BamlValueList instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueList instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlValueList): baml_bridge.cffi.v1.BamlValueList;

                /**
                 * Encodes the specified BamlValueList message. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueList.verify|verify} messages.
                 * @param message BamlValueList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlValueList, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueList message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueList.verify|verify} messages.
                 * @param message BamlValueList message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlValueList, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueList message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlValueList;

                /**
                 * Decodes a BamlValueList message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlValueList;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlValueList;

                /**
                 * Creates a plain object from a BamlValueList message. Also converts values to other types if specified.
                 * @param message BamlValueList
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlValueList, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                value?: (baml_bridge.cffi.v1.IBamlOutboundValue|null);
            }

            /** Represents a BamlOutboundMapEntry. */
            class BamlOutboundMapEntry implements IBamlOutboundMapEntry {

                /**
                 * Constructs a new BamlOutboundMapEntry.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlOutboundMapEntry);

                /** BamlOutboundMapEntry key. */
                public key: string;

                /** BamlOutboundMapEntry value. */
                public value?: (baml_bridge.cffi.v1.IBamlOutboundValue|null);

                /**
                 * Creates a new BamlOutboundMapEntry instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlOutboundMapEntry instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlOutboundMapEntry): baml_bridge.cffi.v1.BamlOutboundMapEntry;

                /**
                 * Encodes the specified BamlOutboundMapEntry message. Does not implicitly {@link baml_bridge.cffi.v1.BamlOutboundMapEntry.verify|verify} messages.
                 * @param message BamlOutboundMapEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlOutboundMapEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlOutboundMapEntry message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlOutboundMapEntry.verify|verify} messages.
                 * @param message BamlOutboundMapEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlOutboundMapEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlOutboundMapEntry message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlOutboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlOutboundMapEntry;

                /**
                 * Decodes a BamlOutboundMapEntry message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlOutboundMapEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlOutboundMapEntry;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlOutboundMapEntry;

                /**
                 * Creates a plain object from a BamlOutboundMapEntry message. Also converts values to other types if specified.
                 * @param message BamlOutboundMapEntry
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlOutboundMapEntry, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                keyType?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlValueMap valueType */
                valueType?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlValueMap entries */
                entries?: (baml_bridge.cffi.v1.IBamlOutboundMapEntry[]|null);
            }

            /** Represents a BamlValueMap. */
            class BamlValueMap implements IBamlValueMap {

                /**
                 * Constructs a new BamlValueMap.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlValueMap);

                /** BamlValueMap keyType. */
                public keyType?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlValueMap valueType. */
                public valueType?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlValueMap entries. */
                public entries: baml_bridge.cffi.v1.IBamlOutboundMapEntry[];

                /**
                 * Creates a new BamlValueMap instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueMap instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlValueMap): baml_bridge.cffi.v1.BamlValueMap;

                /**
                 * Encodes the specified BamlValueMap message. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueMap.verify|verify} messages.
                 * @param message BamlValueMap message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlValueMap, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueMap message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueMap.verify|verify} messages.
                 * @param message BamlValueMap message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlValueMap, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueMap message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlValueMap;

                /**
                 * Decodes a BamlValueMap message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueMap
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlValueMap;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlValueMap;

                /**
                 * Creates a plain object from a BamlValueMap message. Also converts values to other types if specified.
                 * @param message BamlValueMap
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlValueMap, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                name?: (string|null);

                /** BamlValueClass fields */
                fields?: (baml_bridge.cffi.v1.IBamlOutboundMapEntry[]|null);

                /** BamlValueClass typeArgs */
                typeArgs?: (baml_bridge.cffi.v1.IBamlTy[]|null);
            }

            /** Represents a BamlValueClass. */
            class BamlValueClass implements IBamlValueClass {

                /**
                 * Constructs a new BamlValueClass.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlValueClass);

                /** BamlValueClass name. */
                public name: string;

                /** BamlValueClass fields. */
                public fields: baml_bridge.cffi.v1.IBamlOutboundMapEntry[];

                /** BamlValueClass typeArgs. */
                public typeArgs: baml_bridge.cffi.v1.IBamlTy[];

                /**
                 * Creates a new BamlValueClass instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueClass instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlValueClass): baml_bridge.cffi.v1.BamlValueClass;

                /**
                 * Encodes the specified BamlValueClass message. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueClass.verify|verify} messages.
                 * @param message BamlValueClass message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlValueClass, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueClass message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueClass.verify|verify} messages.
                 * @param message BamlValueClass message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlValueClass, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueClass message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlValueClass;

                /**
                 * Decodes a BamlValueClass message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueClass
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlValueClass;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlValueClass;

                /**
                 * Creates a plain object from a BamlValueClass message. Also converts values to other types if specified.
                 * @param message BamlValueClass
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlValueClass, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                name?: (string|null);

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
                constructor(properties?: baml_bridge.cffi.v1.IBamlValueEnum);

                /** BamlValueEnum name. */
                public name: string;

                /** BamlValueEnum value. */
                public value: string;

                /** BamlValueEnum isDynamic. */
                public isDynamic: boolean;

                /**
                 * Creates a new BamlValueEnum instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueEnum instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlValueEnum): baml_bridge.cffi.v1.BamlValueEnum;

                /**
                 * Encodes the specified BamlValueEnum message. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueEnum.verify|verify} messages.
                 * @param message BamlValueEnum message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlValueEnum, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueEnum message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueEnum.verify|verify} messages.
                 * @param message BamlValueEnum message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlValueEnum, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueEnum message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlValueEnum;

                /**
                 * Decodes a BamlValueEnum message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueEnum
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlValueEnum;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlValueEnum;

                /**
                 * Creates a plain object from a BamlValueEnum message. Also converts values to other types if specified.
                 * @param message BamlValueEnum
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlValueEnum, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                name?: (string|null);

                /** BamlValueUnionVariant isOptional */
                isOptional?: (boolean|null);

                /** BamlValueUnionVariant isSinglePattern */
                isSinglePattern?: (boolean|null);

                /** BamlValueUnionVariant selfType */
                selfType?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlValueUnionVariant valueOptionName */
                valueOptionName?: (string|null);

                /** BamlValueUnionVariant value */
                value?: (baml_bridge.cffi.v1.IBamlOutboundValue|null);

                /** BamlValueUnionVariant selectedOptionIndex */
                selectedOptionIndex?: (number|null);
            }

            /** Represents a BamlValueUnionVariant. */
            class BamlValueUnionVariant implements IBamlValueUnionVariant {

                /**
                 * Constructs a new BamlValueUnionVariant.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlValueUnionVariant);

                /** BamlValueUnionVariant name. */
                public name: string;

                /** BamlValueUnionVariant isOptional. */
                public isOptional: boolean;

                /** BamlValueUnionVariant isSinglePattern. */
                public isSinglePattern: boolean;

                /** BamlValueUnionVariant selfType. */
                public selfType?: (baml_bridge.cffi.v1.IBamlTy|null);

                /** BamlValueUnionVariant valueOptionName. */
                public valueOptionName: string;

                /** BamlValueUnionVariant value. */
                public value?: (baml_bridge.cffi.v1.IBamlOutboundValue|null);

                /** BamlValueUnionVariant selectedOptionIndex. */
                public selectedOptionIndex?: (number|null);

                /**
                 * Creates a new BamlValueUnionVariant instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValueUnionVariant instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlValueUnionVariant): baml_bridge.cffi.v1.BamlValueUnionVariant;

                /**
                 * Encodes the specified BamlValueUnionVariant message. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueUnionVariant.verify|verify} messages.
                 * @param message BamlValueUnionVariant message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlValueUnionVariant, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueUnionVariant message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueUnionVariant.verify|verify} messages.
                 * @param message BamlValueUnionVariant message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlValueUnionVariant, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueUnionVariant message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlValueUnionVariant;

                /**
                 * Decodes a BamlValueUnionVariant message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueUnionVariant
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlValueUnionVariant;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlValueUnionVariant;

                /**
                 * Creates a plain object from a BamlValueUnionVariant message. Also converts values to other types if specified.
                 * @param message BamlValueUnionVariant
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlValueUnionVariant, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                media?: (baml_bridge.cffi.v1.MediaTypeEnum|null);

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
                constructor(properties?: baml_bridge.cffi.v1.IBamlValueMedia);

                /** BamlValueMedia media. */
                public media: baml_bridge.cffi.v1.MediaTypeEnum;

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
                public static create(properties?: baml_bridge.cffi.v1.IBamlValueMedia): baml_bridge.cffi.v1.BamlValueMedia;

                /**
                 * Encodes the specified BamlValueMedia message. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueMedia.verify|verify} messages.
                 * @param message BamlValueMedia message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlValueMedia, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValueMedia message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlValueMedia.verify|verify} messages.
                 * @param message BamlValueMedia message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlValueMedia, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValueMedia message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValueMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlValueMedia;

                /**
                 * Decodes a BamlValueMedia message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValueMedia
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlValueMedia;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlValueMedia;

                /**
                 * Creates a plain object from a BamlValueMedia message. Also converts values to other types if specified.
                 * @param message BamlValueMedia
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlValueMedia, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                simple?: (baml_bridge.cffi.v1.IBamlValuePromptAstSimple|null);

                /** BamlValuePromptAst message */
                message?: (baml_bridge.cffi.v1.IBamlValuePromptAstMessage|null);

                /** BamlValuePromptAst multiple */
                multiple?: (baml_bridge.cffi.v1.IBamlValuePromptAstMultiple|null);
            }

            /** Represents a BamlValuePromptAst. */
            class BamlValuePromptAst implements IBamlValuePromptAst {

                /**
                 * Constructs a new BamlValuePromptAst.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlValuePromptAst);

                /** BamlValuePromptAst simple. */
                public simple?: (baml_bridge.cffi.v1.IBamlValuePromptAstSimple|null);

                /** BamlValuePromptAst message. */
                public message?: (baml_bridge.cffi.v1.IBamlValuePromptAstMessage|null);

                /** BamlValuePromptAst multiple. */
                public multiple?: (baml_bridge.cffi.v1.IBamlValuePromptAstMultiple|null);

                /** BamlValuePromptAst value. */
                public value?: ("simple"|"message"|"multiple");

                /**
                 * Creates a new BamlValuePromptAst instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAst instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlValuePromptAst): baml_bridge.cffi.v1.BamlValuePromptAst;

                /**
                 * Encodes the specified BamlValuePromptAst message. Does not implicitly {@link baml_bridge.cffi.v1.BamlValuePromptAst.verify|verify} messages.
                 * @param message BamlValuePromptAst message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlValuePromptAst, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAst message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlValuePromptAst.verify|verify} messages.
                 * @param message BamlValuePromptAst message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlValuePromptAst, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAst message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAst
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlValuePromptAst;

                /**
                 * Decodes a BamlValuePromptAst message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAst
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlValuePromptAst;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlValuePromptAst;

                /**
                 * Creates a plain object from a BamlValuePromptAst message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAst
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlValuePromptAst, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                content?: (baml_bridge.cffi.v1.IBamlValuePromptAstSimple|null);

                /** BamlValuePromptAstMessage metadataAsJson */
                metadataAsJson?: (string|null);
            }

            /** Represents a BamlValuePromptAstMessage. */
            class BamlValuePromptAstMessage implements IBamlValuePromptAstMessage {

                /**
                 * Constructs a new BamlValuePromptAstMessage.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlValuePromptAstMessage);

                /** BamlValuePromptAstMessage role. */
                public role: string;

                /** BamlValuePromptAstMessage content. */
                public content?: (baml_bridge.cffi.v1.IBamlValuePromptAstSimple|null);

                /** BamlValuePromptAstMessage metadataAsJson. */
                public metadataAsJson: string;

                /**
                 * Creates a new BamlValuePromptAstMessage instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAstMessage instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlValuePromptAstMessage): baml_bridge.cffi.v1.BamlValuePromptAstMessage;

                /**
                 * Encodes the specified BamlValuePromptAstMessage message. Does not implicitly {@link baml_bridge.cffi.v1.BamlValuePromptAstMessage.verify|verify} messages.
                 * @param message BamlValuePromptAstMessage message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlValuePromptAstMessage, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAstMessage message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlValuePromptAstMessage.verify|verify} messages.
                 * @param message BamlValuePromptAstMessage message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlValuePromptAstMessage, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAstMessage message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAstMessage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlValuePromptAstMessage;

                /**
                 * Decodes a BamlValuePromptAstMessage message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAstMessage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlValuePromptAstMessage;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlValuePromptAstMessage;

                /**
                 * Creates a plain object from a BamlValuePromptAstMessage message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAstMessage
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlValuePromptAstMessage, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                items?: (baml_bridge.cffi.v1.IBamlValuePromptAst[]|null);
            }

            /** Represents a BamlValuePromptAstMultiple. */
            class BamlValuePromptAstMultiple implements IBamlValuePromptAstMultiple {

                /**
                 * Constructs a new BamlValuePromptAstMultiple.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlValuePromptAstMultiple);

                /** BamlValuePromptAstMultiple items. */
                public items: baml_bridge.cffi.v1.IBamlValuePromptAst[];

                /**
                 * Creates a new BamlValuePromptAstMultiple instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAstMultiple instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlValuePromptAstMultiple): baml_bridge.cffi.v1.BamlValuePromptAstMultiple;

                /**
                 * Encodes the specified BamlValuePromptAstMultiple message. Does not implicitly {@link baml_bridge.cffi.v1.BamlValuePromptAstMultiple.verify|verify} messages.
                 * @param message BamlValuePromptAstMultiple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlValuePromptAstMultiple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAstMultiple message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlValuePromptAstMultiple.verify|verify} messages.
                 * @param message BamlValuePromptAstMultiple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlValuePromptAstMultiple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAstMultiple message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAstMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlValuePromptAstMultiple;

                /**
                 * Decodes a BamlValuePromptAstMultiple message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAstMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlValuePromptAstMultiple;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlValuePromptAstMultiple;

                /**
                 * Creates a plain object from a BamlValuePromptAstMultiple message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAstMultiple
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlValuePromptAstMultiple, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                media?: (baml_bridge.cffi.v1.IBamlValueMedia|null);

                /** BamlValuePromptAstSimple multiple */
                multiple?: (baml_bridge.cffi.v1.IBamlValuePromptAstSimpleMultiple|null);
            }

            /** Represents a BamlValuePromptAstSimple. */
            class BamlValuePromptAstSimple implements IBamlValuePromptAstSimple {

                /**
                 * Constructs a new BamlValuePromptAstSimple.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlValuePromptAstSimple);

                /** BamlValuePromptAstSimple string. */
                public string?: (string|null);

                /** BamlValuePromptAstSimple media. */
                public media?: (baml_bridge.cffi.v1.IBamlValueMedia|null);

                /** BamlValuePromptAstSimple multiple. */
                public multiple?: (baml_bridge.cffi.v1.IBamlValuePromptAstSimpleMultiple|null);

                /** BamlValuePromptAstSimple value. */
                public value?: ("string"|"media"|"multiple");

                /**
                 * Creates a new BamlValuePromptAstSimple instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAstSimple instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlValuePromptAstSimple): baml_bridge.cffi.v1.BamlValuePromptAstSimple;

                /**
                 * Encodes the specified BamlValuePromptAstSimple message. Does not implicitly {@link baml_bridge.cffi.v1.BamlValuePromptAstSimple.verify|verify} messages.
                 * @param message BamlValuePromptAstSimple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlValuePromptAstSimple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAstSimple message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlValuePromptAstSimple.verify|verify} messages.
                 * @param message BamlValuePromptAstSimple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlValuePromptAstSimple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAstSimple message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAstSimple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlValuePromptAstSimple;

                /**
                 * Decodes a BamlValuePromptAstSimple message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAstSimple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlValuePromptAstSimple;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlValuePromptAstSimple;

                /**
                 * Creates a plain object from a BamlValuePromptAstSimple message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAstSimple
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlValuePromptAstSimple, options?: $protobuf.IConversionOptions): { [k: string]: any };

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
                items?: (baml_bridge.cffi.v1.IBamlValuePromptAstSimple[]|null);
            }

            /** Represents a BamlValuePromptAstSimpleMultiple. */
            class BamlValuePromptAstSimpleMultiple implements IBamlValuePromptAstSimpleMultiple {

                /**
                 * Constructs a new BamlValuePromptAstSimpleMultiple.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlValuePromptAstSimpleMultiple);

                /** BamlValuePromptAstSimpleMultiple items. */
                public items: baml_bridge.cffi.v1.IBamlValuePromptAstSimple[];

                /**
                 * Creates a new BamlValuePromptAstSimpleMultiple instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlValuePromptAstSimpleMultiple instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlValuePromptAstSimpleMultiple): baml_bridge.cffi.v1.BamlValuePromptAstSimpleMultiple;

                /**
                 * Encodes the specified BamlValuePromptAstSimpleMultiple message. Does not implicitly {@link baml_bridge.cffi.v1.BamlValuePromptAstSimpleMultiple.verify|verify} messages.
                 * @param message BamlValuePromptAstSimpleMultiple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlValuePromptAstSimpleMultiple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlValuePromptAstSimpleMultiple message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlValuePromptAstSimpleMultiple.verify|verify} messages.
                 * @param message BamlValuePromptAstSimpleMultiple message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlValuePromptAstSimpleMultiple, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlValuePromptAstSimpleMultiple message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlValuePromptAstSimpleMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlValuePromptAstSimpleMultiple;

                /**
                 * Decodes a BamlValuePromptAstSimpleMultiple message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlValuePromptAstSimpleMultiple
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlValuePromptAstSimpleMultiple;

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
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlValuePromptAstSimpleMultiple;

                /**
                 * Creates a plain object from a BamlValuePromptAstSimpleMultiple message. Also converts values to other types if specified.
                 * @param message BamlValuePromptAstSimpleMultiple
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlValuePromptAstSimpleMultiple, options?: $protobuf.IConversionOptions): { [k: string]: any };

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

            /** Properties of a BamlToHostCall. */
            interface IBamlToHostCall {

                /** BamlToHostCall args */
                args?: (baml_bridge.cffi.v1.IBamlToHostArg[]|null);
            }

            /** Represents a BamlToHostCall. */
            class BamlToHostCall implements IBamlToHostCall {

                /**
                 * Constructs a new BamlToHostCall.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlToHostCall);

                /** BamlToHostCall args. */
                public args: baml_bridge.cffi.v1.IBamlToHostArg[];

                /**
                 * Creates a new BamlToHostCall instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlToHostCall instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlToHostCall): baml_bridge.cffi.v1.BamlToHostCall;

                /**
                 * Encodes the specified BamlToHostCall message. Does not implicitly {@link baml_bridge.cffi.v1.BamlToHostCall.verify|verify} messages.
                 * @param message BamlToHostCall message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlToHostCall, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlToHostCall message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlToHostCall.verify|verify} messages.
                 * @param message BamlToHostCall message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlToHostCall, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlToHostCall message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlToHostCall
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlToHostCall;

                /**
                 * Decodes a BamlToHostCall message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlToHostCall
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlToHostCall;

                /**
                 * Verifies a BamlToHostCall message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlToHostCall message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlToHostCall
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlToHostCall;

                /**
                 * Creates a plain object from a BamlToHostCall message. Also converts values to other types if specified.
                 * @param message BamlToHostCall
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlToHostCall, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlToHostCall to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlToHostCall
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlToHostArg. */
            interface IBamlToHostArg {

                /** BamlToHostArg value */
                value?: (baml_bridge.cffi.v1.IBamlOutboundValue|null);

                /** BamlToHostArg argName */
                argName?: (string|null);

                /** BamlToHostArg isOptionalArg */
                isOptionalArg?: (boolean|null);
            }

            /** Represents a BamlToHostArg. */
            class BamlToHostArg implements IBamlToHostArg {

                /**
                 * Constructs a new BamlToHostArg.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlToHostArg);

                /** BamlToHostArg value. */
                public value?: (baml_bridge.cffi.v1.IBamlOutboundValue|null);

                /** BamlToHostArg argName. */
                public argName: string;

                /** BamlToHostArg isOptionalArg. */
                public isOptionalArg: boolean;

                /**
                 * Creates a new BamlToHostArg instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlToHostArg instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlToHostArg): baml_bridge.cffi.v1.BamlToHostArg;

                /**
                 * Encodes the specified BamlToHostArg message. Does not implicitly {@link baml_bridge.cffi.v1.BamlToHostArg.verify|verify} messages.
                 * @param message BamlToHostArg message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlToHostArg, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlToHostArg message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlToHostArg.verify|verify} messages.
                 * @param message BamlToHostArg message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlToHostArg, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlToHostArg message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlToHostArg
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlToHostArg;

                /**
                 * Decodes a BamlToHostArg message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlToHostArg
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlToHostArg;

                /**
                 * Verifies a BamlToHostArg message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlToHostArg message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlToHostArg
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlToHostArg;

                /**
                 * Creates a plain object from a BamlToHostArg message. Also converts values to other types if specified.
                 * @param message BamlToHostArg
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlToHostArg, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlToHostArg to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlToHostArg
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a BamlLiteralValue. */
            interface IBamlLiteralValue {

                /** BamlLiteralValue stringValue */
                stringValue?: (string|null);

                /** BamlLiteralValue intValue */
                intValue?: (number|Long|null);

                /** BamlLiteralValue boolValue */
                boolValue?: (boolean|null);

                /** BamlLiteralValue bigintValue */
                bigintValue?: (string|null);

                /** BamlLiteralValue floatValue */
                floatValue?: (string|null);
            }

            /** Represents a BamlLiteralValue. */
            class BamlLiteralValue implements IBamlLiteralValue {

                /**
                 * Constructs a new BamlLiteralValue.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: baml_bridge.cffi.v1.IBamlLiteralValue);

                /** BamlLiteralValue stringValue. */
                public stringValue?: (string|null);

                /** BamlLiteralValue intValue. */
                public intValue?: (number|Long|null);

                /** BamlLiteralValue boolValue. */
                public boolValue?: (boolean|null);

                /** BamlLiteralValue bigintValue. */
                public bigintValue?: (string|null);

                /** BamlLiteralValue floatValue. */
                public floatValue?: (string|null);

                /** BamlLiteralValue literal. */
                public literal?: ("stringValue"|"intValue"|"boolValue"|"bigintValue"|"floatValue");

                /**
                 * Creates a new BamlLiteralValue instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns BamlLiteralValue instance
                 */
                public static create(properties?: baml_bridge.cffi.v1.IBamlLiteralValue): baml_bridge.cffi.v1.BamlLiteralValue;

                /**
                 * Encodes the specified BamlLiteralValue message. Does not implicitly {@link baml_bridge.cffi.v1.BamlLiteralValue.verify|verify} messages.
                 * @param message BamlLiteralValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: baml_bridge.cffi.v1.IBamlLiteralValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified BamlLiteralValue message, length delimited. Does not implicitly {@link baml_bridge.cffi.v1.BamlLiteralValue.verify|verify} messages.
                 * @param message BamlLiteralValue message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: baml_bridge.cffi.v1.IBamlLiteralValue, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a BamlLiteralValue message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns BamlLiteralValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): baml_bridge.cffi.v1.BamlLiteralValue;

                /**
                 * Decodes a BamlLiteralValue message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns BamlLiteralValue
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): baml_bridge.cffi.v1.BamlLiteralValue;

                /**
                 * Verifies a BamlLiteralValue message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a BamlLiteralValue message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns BamlLiteralValue
                 */
                public static fromObject(object: { [k: string]: any }): baml_bridge.cffi.v1.BamlLiteralValue;

                /**
                 * Creates a plain object from a BamlLiteralValue message. Also converts values to other types if specified.
                 * @param message BamlLiteralValue
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: baml_bridge.cffi.v1.BamlLiteralValue, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this BamlLiteralValue to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for BamlLiteralValue
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }
        }
    }
}
