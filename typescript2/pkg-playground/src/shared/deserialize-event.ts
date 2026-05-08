import { deserializeValue, type RuntimeEvent } from '@b/pkg-proto';
import type { DeserializedRuntimeEvent, DeserializedEventKind } from '../worker-protocol';

function wrapHandle(key: bigint, handleType: number, typeName: string) {
  return { handle_key: key, handle_type: handleType, type_name: typeName };
}

export function deserializeRuntimeEvent(event: RuntimeEvent): DeserializedRuntimeEvent {
  let deserializedKind: DeserializedEventKind | undefined;
  const kind = event.event?.kind;
  if (kind) {
    switch (kind.$case) {
      case 'functionStart':
        deserializedKind = {
          $case: 'functionStart',
          functionStart: {
            name: kind.functionStart.name,
            args: kind.functionStart.args.map((arg) => deserializeValue(arg, wrapHandle)),
          },
        };
        break;
      case 'functionEnd':
        deserializedKind = {
          $case: 'functionEnd',
          functionEnd: {
            name: kind.functionEnd.name,
            durationMs: kind.functionEnd.durationMs,
            result: kind.functionEnd.result
              ? deserializeValue(kind.functionEnd.result, wrapHandle)
              : null,
            error: kind.functionEnd.error ?? null,
          },
        };
        break;
      case 'log':
        deserializedKind = {
          $case: 'log',
          log: {
            data: kind.log.data ? deserializeValue(kind.log.data, wrapHandle) : null,
            level: kind.log.level,
            source: kind.log.source,
          },
        };
        break;
      case 'custom':
        deserializedKind = {
          $case: 'custom',
          custom: {
            name: kind.custom.name,
            data: kind.custom.data ? deserializeValue(kind.custom.data, wrapHandle) : null,
          },
        };
        break;
      case 'setTags':
        deserializedKind = {
          $case: 'setTags',
          setTags: { tags: kind.setTags.tags },
        };
        break;
    }
  }

  return {
    spanId: event.spanId,
    parentSpanId: event.parentSpanId,
    rootSpanId: event.rootSpanId,
    timestampMs: event.timestampMs,
    callStack: event.callStack,
    event: deserializedKind,
  };
}
