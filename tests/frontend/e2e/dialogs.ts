// Only the E2E Vite build aliases the OS picker to deterministic fixture selections.
// Business IPC, events, queues, storage, and inference still use the native backend.
async function selected(command: string, options: unknown): Promise<unknown> {
  const mocks: unknown = Reflect.get(globalThis, "__wdio_mocks__");
  const handler: unknown =
    mocks && typeof mocks === "object" ? Reflect.get(mocks, `plugin:dialog|${command}`) : undefined;
  if (typeof handler !== "function") {
    throw new Error(`E2E native dialog ${command} must be explicitly configured`);
  }
  return handler({ options });
}
export const open = (options?: unknown) => selected("open", options);
export const save = (options?: unknown) => selected("save", options);
export const ask = (message: unknown, options?: unknown) => selected("ask", { message, options });
export const confirm = (message: unknown, options?: unknown) =>
  selected("confirm", { message, options });
export const message = (message: unknown, options?: unknown) =>
  selected("message", { message, options });
