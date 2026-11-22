import { readFileSync, readdirSync } from "node:fs";
import { extname, join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { en } from "@/i18n/en";
import { zh } from "@/i18n/zh";

function rustFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? rustFiles(path) : extname(path) === ".rs" ? [path] : [];
  });
}

function backendErrorCodes() {
  const root = resolve(process.cwd(), "src-tauri/src");
  const errorSource = readFileSync(join(root, "error.rs"), "utf8");
  const builtIn = [
    ...errorSource.matchAll(/AppError::[A-Za-z0-9_]+(?:\([^)]*\)|\s*\{[^}]*\})?\s*=>\s*"([^"]+)"/g),
  ]
    .map((match) => match[1])
    .filter((code) => !["generic", "library_state"].includes(code));
  const coded = rustFiles(root).flatMap((path) => {
    const source = readFileSync(path, "utf8");
    return [...source.matchAll(/AppError::coded\(\s*"([^"]+)"/g)].map((match) => match[1]);
  });
  return [...new Set([...builtIn, ...coded])].sort();
}

describe("backend error localization", () => {
  it("keeps English and Chinese dictionaries structurally identical", () => {
    expect(Object.keys(en.errors).sort()).toEqual(Object.keys(zh.errors).sort());
  });

  it("localizes every stable backend error code", () => {
    const localized = new Set(Object.keys(en.errors));
    const missing = backendErrorCodes().filter((code) => !localized.has(code));

    expect(missing).toEqual([]);
  });
});
