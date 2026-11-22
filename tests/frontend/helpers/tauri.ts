// Provide Tauri internals explicitly in jsdom; convertFileSrc reads them directly and should
// retain asset:// protocol semantics.
type TauriInternals = Record<string, unknown> & {
  convertFileSrc?: (path: string, protocol?: string) => string;
};

export function stubConvertFileSrc() {
  const windowWithInternals = window as unknown as { __TAURI_INTERNALS__?: TauriInternals };
  windowWithInternals.__TAURI_INTERNALS__ = {
    ...windowWithInternals.__TAURI_INTERNALS__,
    convertFileSrc: (path: string) => `asset://localhost/${path}`,
  };
}
