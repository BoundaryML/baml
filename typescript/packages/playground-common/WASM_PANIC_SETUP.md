# WASM Panic Handler Setup Guide

This guide explains how to use the WASM panic handler system implemented in the BAML playground.

## Overview

The panic handler captures WASM runtime panics from Rust and exposes them to the React UI through Jotai atoms. It automatically cancels running tests when a panic is detected.

## Architecture

### Rust Side (`engine/baml-schema-wasm/src/runtime_wasm/mod.rs:81-84`)

The panic hook is set up during WASM initialization:

```rust
panic::set_hook(Box::new(|info| {
    let msg = info.to_string();
    on_wasm_panic(&msg); // notify JS right before abort/unwind
}));
```

This calls `window.__onWasmPanic` before the panic completes, giving the UI a chance to update state.

### JavaScript Side (`atoms.ts:35-46`)

The global handler is defined **before** WASM loads (critical timing!):

```typescript
if (typeof window !== 'undefined') {
  (window as any).__onWasmPanic = (msg: string) => {
    console.error('[WASM Panic]', msg);
    if (globalSetPanic) {
      globalSetPanic(msg);
    } else {
      console.warn('[WASM Panic] Handler called but atom setter not yet initialized');
    }
  };
}
```

### Test Runner Integration (`test-runner.ts:457-464`)

The test runner automatically cancels tests when a panic is detected:

```typescript
useEffect(() => {
  if (panicState && currentAbortController) {
    console.error('[WASM Panic] Detected panic during test run, cancelling tests:', panicState.msg)
    currentAbortController.abort()
    setCurrentAbortController(null)
    setAreTestsRunning(false)
  }
}, [panicState, currentAbortController, setCurrentAbortController, setAreTestsRunning])
```

## Setup in Your Root Component

Add the `useWasmPanicHandler` hook to your root component:

```tsx
import { useWasmPanicHandler } from './shared/baml-project-panel/atoms';
import { WasmPanicNotification } from './shared/baml-project-panel/WasmPanicNotification';

function App() {
  // Wire up the panic handler
  useWasmPanicHandler();

  return (
    <>
      {/* Optional: Display panic notifications */}
      <WasmPanicNotification />

      {/* Your app content */}
      <YourAppContent />
    </>
  );
}
```

## Using Panic State in Components

### Read panic state:

```tsx
import { useAtomValue } from 'jotai';
import { wasmPanicAtom } from './shared/baml-project-panel/atoms';

function MyComponent() {
  const panicState = useAtomValue(wasmPanicAtom);

  if (panicState) {
    return <div>WASM panicked: {panicState.msg}</div>;
  }

  return <div>All good!</div>;
}
```

### Clear panic state:

```tsx
import { useClearWasmPanic } from './shared/baml-project-panel/atoms';

function MyComponent() {
  const clearPanic = useClearWasmPanic();

  return (
    <button onClick={clearPanic}>
      Clear Panic
    </button>
  );
}
```

### React to panics in effects:

```tsx
import { useAtomValue } from 'jotai';
import { wasmPanicAtom } from './shared/baml-project-panel/atoms';
import { useEffect } from 'react';

function TestRunner() {
  const panicState = useAtomValue(wasmPanicAtom);

  useEffect(() => {
    if (panicState) {
      // Cancel running tests, set error state, etc.
      console.error('Test aborted due to WASM panic:', panicState.msg);
    }
  }, [panicState]);

  // ... rest of component
}
```

## How It Works

1. **Rust side** (`engine/baml-schema-wasm/src/runtime_wasm/mod.rs`):
   - Sets up a panic hook with `panic::set_hook`
   - Calls `on_wasm_panic` JS function before the panic completes

2. **JavaScript side** (`atoms.ts`):
   - Defines `window.__onWasmPanic` BEFORE WASM loads
   - Updates the `wasmPanicAtom` when panics occur

3. **React components**:
   - Use `useWasmPanicHandler()` to wire up the atom
   - Subscribe to `wasmPanicAtom` to react to panics
   - Use `useClearWasmPanic()` to dismiss notifications

## Important Notes

- The panic handler **does NOT prevent the panic** - it just gives you a signal before it happens
- WASM will still throw an error after the handler runs
- The handler must be set up before WASM loads (done automatically in `atoms.ts`)
- Call `useWasmPanicHandler()` once in your root component to enable atom updates

## Files Modified

### TypeScript Files:
1. **`typescript/packages/playground-common/src/shared/baml-project-panel/atoms.ts`**
   - Added `wasmPanicAtom` to track panic state
   - Added `useWasmPanicHandler()` hook to wire up the atom
   - Added `useClearWasmPanic()` hook to clear panic state
   - Set up `window.__onWasmPanic` global handler

2. **`typescript/packages/playground-common/src/shared/baml-project-panel/WasmPanicNotification.tsx`** (new file)
   - Example notification component to display panics

3. **`typescript/packages/playground-common/src/shared/baml-project-panel/playground-panel/prompt-preview/test-panel/test-runner.ts`**
   - Added panic detection to `useRunBamlTests()`
   - Automatically cancels tests when panic occurs

4. **`typescript/apps/playground/src/App.tsx`**
   - Added `useWasmPanicHandler()` to wire up panic handling
   - Added `<WasmPanicNotification />` component to display panics

### Rust Files:
1. **`engine/baml-schema-wasm/src/runtime_wasm/mod.rs:81-84`** (already implemented)
   - Panic hook that calls `window.__onWasmPanic` before panic completes
