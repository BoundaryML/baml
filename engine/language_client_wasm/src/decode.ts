import { CFFIValueHolder } from './proto/cffi_pb.js';

export type TypeMap = Map<string, new(...args: any[]) => any>;

export function decodeValue(holder: CFFIValueHolder, typeMap: TypeMap): any {
  switch (holder.value?.case) {
    case 'nullValue':
      return null;
      
    case 'stringValue':
      return holder.value.value;
      
    case 'intValue':
      return Number(holder.value.value);
      
    case 'floatValue':
      return holder.value.value;
      
    case 'boolValue':
      return holder.value.value;
      
    case 'listValue':
      return holder.value.value.items.map((item: CFFIValueHolder) => decodeValue(item, typeMap));
      
    case 'mapValue':
      const result: Record<string, any> = {};
      for (const [key, val] of Object.entries(holder.value.value.entries)) {
        result[key] = decodeValue(val as CFFIValueHolder, typeMap);
      }
      return result;
      
    case 'classValue':
      const className = holder.value.value.name;
      const ClassConstructor = typeMap.get(className);
      
      if (ClassConstructor) {
        const instance = new ClassConstructor();
        for (const [key, val] of Object.entries(holder.value.value.fields)) {
          instance[key] = decodeValue(val as CFFIValueHolder, typeMap);
        }
        return instance;
      }
      
      // Dynamic fallback
      return {
        __bamlType: className,
        ...Object.fromEntries(
          Object.entries(holder.value.value.fields).map(
            ([k, v]) => [k, decodeValue(v as CFFIValueHolder, typeMap)]
          )
        )
      };
      
    default:
      throw new Error(`Unknown value case: ${holder.value?.case}`);
  }
}