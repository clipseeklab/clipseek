import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import ts from "typescript";

function frontendCommands(source = readFileSync(resolve(process.cwd(), "src/lib/api.ts"), "utf8")) {
  const ast = ts.createSourceFile("api.ts", source, ts.ScriptTarget.Latest, true);
  const commands: string[] = [];
  const visit = (node: ts.Node) => {
    if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === "invoke"
    ) {
      const name = node.arguments[0];
      if (!name || !ts.isStringLiteral(name)) throw new Error("IPC names must be string literals");
      commands.push(name.text);
    }
    ts.forEachChild(node, visit);
  };
  visit(ast);
  return commands;
}

function registeredRustCommands() {
  const source = readFileSync(resolve(process.cwd(), "src-tauri/src/lib.rs"), "utf8");
  const start = source.indexOf("tauri::generate_handler![");
  const end = source.indexOf("])", start);
  if (start < 0 || end < 0) throw new Error("Rust generate_handler! registration was not found");
  const handler = source.slice(start, end);
  return [...handler.matchAll(/(?:\b[a-z_][a-z0-9_]*::)+([a-z_][a-z0-9_]*),/g)].map(
    (match) => match[1],
  );
}

describe("frontend and Rust IPC registration", () => {
  it("extracts command names independently of multiline and nested generic formatting", () => {
    expect(
      frontendCommands(`
      invoke<{ tasks: Record<string, Array<{ id: number }>> }>(
        "index_summary",
      );
      invoke<void>('library_switch', { id: "library" });
    `),
    ).toEqual(["index_summary", "library_switch"]);
  });
  it("keeps every frontend command registered by the Tauri backend", () => {
    const frontend = frontendCommands();
    const registered = registeredRustCommands();

    expect(new Set(frontend).size).toBe(frontend.length);
    expect(new Set(registered).size).toBe(registered.length);
    expect([...frontend].sort()).toEqual([...registered].sort());
  });
});
