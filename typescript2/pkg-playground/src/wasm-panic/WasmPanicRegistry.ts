export class WasmPanicRegistry {
  private message = '';

  get() {
    return `${this.message}`;
  }

  clear() {
    this.message = '';
  }

  // Called by our console.error interceptor when it detects a panic
  set_message(value: string) {
    this.message = `RuntimeError: ${value}`;
  }
}
