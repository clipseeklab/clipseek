import { readFileSync, readdirSync } from "node:fs";
import { extname, join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

function files(directory: string, extensions: Set<string>): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory()
      ? files(path, extensions)
      : extensions.has(extname(path))
        ? [path]
        : [];
  });
}

describe("Tauri event registration", () => {
  it("keeps every frontend listener backed by a Rust emitter", () => {
    const root = process.cwd();
    const frontendEvents = files(resolve(root, "src"), new Set([".ts", ".tsx"]))
      .flatMap((path) => [
        ...readFileSync(path, "utf8").matchAll(/\blisten(?:<[^;]*?>)?\s*\(\s*"([^"]+)"/g),
      ])
      .map((match) => match[1]);
    const backendEvents = files(resolve(root, "src-tauri/src"), new Set([".rs"]))
      .flatMap((path) => [...readFileSync(path, "utf8").matchAll(/\.emit\(\s*"([^"]+)"/g)])
      .map((match) => match[1]);
    const emitted = new Set(backendEvents);

    expect(frontendEvents.length).toBeGreaterThan(0);
    expect(frontendEvents.filter((event) => !emitted.has(event))).toEqual([]);
  });
});
