import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { spawnSync } from "node:child_process";

const moduleUrl = pathToFileURL(path.resolve("scripts/build-ort.mjs")).href;
async function fixture(t, arch = "x86_64", version = "1.30.0") {
  const root = await mkdtemp(path.join(tmpdir(), "clipseek-ort-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const bin = path.join(root, "bin");
  await mkdir(bin);
  const tool = `#!${process.execPath}
const fs = require('node:fs'); const path = require('node:path');
const command = path.basename(process.argv[1]); const args = process.argv.slice(2);
fs.appendFileSync(process.env.CALLS, JSON.stringify({command,args})+'\\n');
if(command==='git') { const dest=args.at(-1); fs.mkdirSync(dest,{recursive:true}); fs.writeFileSync(path.join(dest,'VERSION_NUMBER'),process.env.SOURCE_VERSION); }
if(command==='bash') { const build=args[args.indexOf('--build_dir')+1]; fs.mkdirSync(path.join(build,'Release'),{recursive:true}); fs.writeFileSync(path.join(build,'Release','libonnxruntime.1.30.0.dylib'),'synthetic test library'); }
if(command==='lipo') process.stdout.write(process.env.TEST_ARCH+'\\n');
`;
  for (const name of ["git", "bash", "lipo"])
    await writeFile(path.join(bin, name), tool, { mode: 0o755 });
  const calls = path.join(root, "calls");
  const run = () =>
    spawnSync(
      process.execPath,
      [
        "--input-type=module",
        "-e",
        `import {prepareIntelOrt} from ${JSON.stringify(moduleUrl)}; await prepareIntelOrt(${JSON.stringify(root)});`,
      ],
      {
        encoding: "utf8",
        env: { ...process.env, PATH: bin, CALLS: calls, TEST_ARCH: arch, SOURCE_VERSION: version },
      },
    );
  return { root, calls, run };
}

test("Intel runtime builds the matching source version and reuses only a verified cache", async (t) => {
  const f = await fixture(t);
  let result = f.run();
  assert.equal(result.status, 0, result.stderr);
  result = f.run();
  assert.equal(result.status, 0, result.stderr);
  const calls = (await readFile(f.calls, "utf8")).trim().split("\n").map(JSON.parse);
  assert.equal(calls.filter((c) => c.command === "git").length, 1);
  assert.equal(calls.filter((c) => c.command === "bash").length, 1);
  const clone = calls.find((c) => c.command === "git");
  assert.ok(clone.args.includes("v1.30.0"));
  assert.ok(clone.args.includes("https://github.com/microsoft/onnxruntime.git"));
  const build = calls.find((c) => c.command === "bash");
  assert.ok(build.args.includes("CMAKE_OSX_ARCHITECTURES=x86_64"));
  assert.ok(build.args.includes("CMAKE_OSX_DEPLOYMENT_TARGET=14.0"));
  assert.equal(calls.filter((c) => c.command === "lipo").length, 2);
});

test("Intel runtime refuses an ARM64 library", async (t) => {
  const f = await fixture(t, "arm64");
  const result = f.run();
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Expected an Intel-only/);
});

test("Intel runtime refuses a source version mismatch before compilation", async (t) => {
  const f = await fixture(t, "x86_64", "1.22.0");
  const result = f.run();
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Unexpected ONNX Runtime source version/);
  assert.doesNotMatch(await readFile(f.calls, "utf8"), /"command":"bash"/);
});
