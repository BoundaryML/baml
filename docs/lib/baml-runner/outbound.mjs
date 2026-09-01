const WIRE_VARINT = 0;
const WIRE_I64 = 1;
const WIRE_LEN = 2;
const WIRE_I32 = 5;

class Reader {
  constructor(bytes) {
    this.bytesValue = bytes;
    this.index = 0;
  }

  get done() {
    return this.index >= this.bytesValue.length;
  }

  varint() {
    let result = 0n;
    let shift = 0n;

    for (;;) {
      if (this.done) throw new Error('truncated varint');
      const byte = this.bytesValue[this.index++];
      result |= BigInt(byte & 0x7f) << shift;
      if ((byte & 0x80) === 0) return result;
      shift += 7n;
    }
  }

  bytes() {
    const length = Number(this.varint());
    if (this.index + length > this.bytesValue.length) {
      throw new Error('truncated length-delimited field');
    }
    const value = this.bytesValue.subarray(this.index, this.index + length);
    this.index += length;
    return value;
  }

  skip(wire) {
    if (wire === WIRE_VARINT) this.varint();
    else if (wire === WIRE_LEN) this.bytes();
    else if (wire === WIRE_I64) this.index += 8;
    else if (wire === WIRE_I32) this.index += 4;
    else throw new Error(`unsupported wire type ${wire}`);
  }
}

const textDecoder = new TextDecoder();

function visitFields(bytes, onField) {
  const reader = new Reader(bytes);
  while (!reader.done) {
    const tag = Number(reader.varint());
    const field = tag >>> 3;
    const wire = tag & 7;
    if (!onField(field, wire, reader)) reader.skip(wire);
  }
}

function mapEntry(bytes) {
  let key = '';
  let value = null;
  visitFields(bytes, (field, wire, reader) => {
    if (field === 1 && wire === WIRE_LEN) {
      key = textDecoder.decode(reader.bytes());
      return true;
    }
    if (field === 2 && wire === WIRE_LEN) {
      value = decodeOutboundValue(reader.bytes());
      return true;
    }
    return false;
  });
  return [key, value];
}

function repeatedInto(bytes, fieldNumber, decode) {
  const values = [];
  visitFields(bytes, (field, wire, reader) => {
    if (field === fieldNumber && wire === WIRE_LEN) {
      values.push(decode(reader.bytes()));
      return true;
    }
    return false;
  });
  return values;
}

/** Decode the `BamlOutboundValue` protobuf returned by bridge_wasm. */
export function decodeOutboundValue(bytes) {
  let value;
  let matched = false;

  visitFields(bytes, (field, wire, reader) => {
    switch (field) {
      case 2:
        reader.bytes();
        value = null;
        matched = true;
        return true;
      case 3:
        value = textDecoder.decode(reader.bytes());
        matched = true;
        return true;
      case 4:
        value = BigInt.asIntN(64, reader.varint());
        matched = true;
        return true;
      case 5: {
        const view = new DataView(
          reader.bytesValue.buffer,
          reader.bytesValue.byteOffset + reader.index,
          8,
        );
        value = view.getFloat64(0, true);
        reader.index += 8;
        matched = true;
        return true;
      }
      case 6:
        value = reader.varint() !== 0n;
        matched = true;
        return true;
      case 7: {
        const body = reader.bytes();
        let name = '';
        visitFields(body, (bodyField, bodyWire, bodyReader) => {
          if (bodyField === 1 && bodyWire === WIRE_LEN) {
            name = textDecoder.decode(bodyReader.bytes());
            return true;
          }
          return false;
        });
        value = {
          $baml: name,
          ...Object.fromEntries(repeatedInto(body, 2, mapEntry)),
        };
        matched = true;
        return true;
      }
      case 8: {
        const body = reader.bytes();
        let enumValue = '';
        visitFields(body, (bodyField, bodyWire, bodyReader) => {
          if (bodyField === 2 && bodyWire === WIRE_LEN) {
            enumValue = textDecoder.decode(bodyReader.bytes());
            return true;
          }
          return false;
        });
        value = enumValue;
        matched = true;
        return true;
      }
      case 9: {
        const body = reader.bytes();
        visitFields(body, (literalField, literalWire, literalReader) => {
          if (literalField === 1 && literalWire === WIRE_LEN) {
            value = textDecoder.decode(literalReader.bytes());
            return true;
          }
          if (literalField === 2 && literalWire === WIRE_VARINT) {
            value = BigInt.asIntN(64, literalReader.varint());
            return true;
          }
          if (literalField === 3 && literalWire === WIRE_VARINT) {
            value = literalReader.varint() !== 0n;
            return true;
          }
          if (literalField === 4 && literalWire === WIRE_LEN) {
            value = BigInt(textDecoder.decode(literalReader.bytes()));
            return true;
          }
          if (literalField === 5 && literalWire === WIRE_LEN) {
            value = Number(textDecoder.decode(literalReader.bytes()));
            return true;
          }
          return false;
        });
        matched = true;
        return true;
      }
      case 11:
        value = repeatedInto(reader.bytes(), 2, decodeOutboundValue);
        matched = true;
        return true;
      case 12:
        value = Object.fromEntries(repeatedInto(reader.bytes(), 3, mapEntry));
        matched = true;
        return true;
      case 19:
        value = reader.bytes().slice();
        matched = true;
        return true;
      case 20:
        value = BigInt(textDecoder.decode(reader.bytes()));
        matched = true;
        return true;
      case 13:
      case 16:
      case 17:
      case 18:
      case 21:
      case 22:
        throw new Error(
          `BamlOutboundValue variant ${field} is not decoded yet; update the decoder from baml_outbound.proto`,
        );
      default:
        return false;
    }
  });

  if (!matched) {
    throw new Error('BamlOutboundValue carried no recognized value variant');
  }
  return value;
}

export function decodeBase64(base64) {
  if (typeof Buffer !== 'undefined') {
    return new Uint8Array(Buffer.from(base64, 'base64'));
  }

  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

export function formatValue(value) {
  if (typeof value === 'string') return JSON.stringify(value);
  if (typeof value === 'bigint') return value.toString();
  if (value === null) return 'null';
  if (value instanceof Uint8Array) {
    const bytes = [...value]
      .map((byte) => `\\x${byte.toString(16).padStart(2, '0')}`)
      .join('');
    return `b"${bytes}"`;
  }
  if (Array.isArray(value)) return `[${value.map(formatValue).join(', ')}]`;
  if (typeof value === 'object') {
    const { $baml, ...rest } = value;
    const body = Object.entries(rest)
      .map(([key, item]) => `${key}: ${formatValue(item)}`)
      .join(', ');
    return $baml ? `${$baml} { ${body} }` : `{ ${body} }`;
  }
  return String(value);
}
