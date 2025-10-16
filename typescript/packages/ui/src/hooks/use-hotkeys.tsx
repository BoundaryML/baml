import { useEffect } from 'react';
/* eslint-disable @typescript-eslint/ban-ts-comment */
// https://github.com/jamiebuilds/tinykeys/issues/191
// @ts-expect-error
import type { KeyBindingMap, KeyBindingOptions } from 'tinykeys';
// @ts-expect-error
import { tinykeys } from 'tinykeys';

export interface UseHotkeysProps extends KeyBindingOptions {
  ref?: React.RefObject<HTMLElement>;
  disableOnInputs?: boolean;
}

function isEventTargetInputOrTextArea(eventTarget: EventTarget | null) {
  if (eventTarget === null) return false;

  const eventTargetTagName = (eventTarget as HTMLElement).tagName.toLowerCase();
  return ['input', 'textarea'].includes(eventTargetTagName);
}

export function useHotkeys(
  keyBindingMap: KeyBindingMap,
  deps?: unknown[],
  props?: UseHotkeysProps,
) {
  useEffect(() => {
    const wrappedBindings =
      (props?.disableOnInputs ?? true)
        ? Object.fromEntries(
            Object.entries(keyBindingMap).map(([key, handler]) => [
              key,
              (event: KeyboardEvent) => {
                if (!isEventTargetInputOrTextArea(event.target)) {
                  // @ts-ignore
                  handler(event);
                }
              },
            ]),
          )
        : keyBindingMap;

    const unsubscribe = tinykeys(
      props?.ref?.current ?? window,
      wrappedBindings,
      props,
    );

    return () => {
      unsubscribe();
    };
  }, [...(deps ?? []), keyBindingMap, props]);
}
