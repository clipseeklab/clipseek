import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { spawnSync } from "node:child_process";

const moduleUrl = pathToFileURL(path.resolve("scripts/build-ort.mjs")).href;
async function fixture(t, arch = "x86_64", version = "1.30.0", corrupt = false) {
  const root = await mkdtemp(path.join(tmpdir(), "clipseek-ort-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const bin = path.join(root, "bin");
  await mkdir(bin);
  const tool = `#!${process.execPath}
const fs = require('node:fs'); const path = require('node:path');
const command = path.basename(process.argv[1]); const args = process.argv.slice(2);
fs.appendFileSync(process.env.CALLS, JSON.stringify({command,args})+'\\n');
if(command==='curl') fs.writeFileSync(args[args.indexOf('--output')+1],process.env.ARCHIVE_CONTENT);
if(command==='tar') { const dest=args[args.indexOf('-C')+1]; fs.mkdirSync(dest,{recursive:true}); fs.writeFileSync(path.join(dest,'VERSION_NUMBER'),process.env.SOURCE_VERSION); fs.mkdirSync(path.join(dest,'cmake'),{recursive:true}); fs.writeFileSync(path.join(dest,'cmake','deps.txt'),''); }
if(command==='bash') { const build=args[args.indexOf('--build_dir')+1]; fs.mkdirSync(path.join(build,'Release'),{recursive:true}); fs.writeFileSync(path.join(build,'Release','libonnxruntime.1.30.0.dylib'),'synthetic test library'); }
if(command==='lipo') process.stdout.write(process.env.TEST_ARCH+'\\n');
`;
  for (const name of ["curl", "tar", "bash", "lipo"])
    await writeFile(path.join(bin, name), tool, { mode: 0o755 });
  const calls = path.join(root, "calls");
  const run = () =>
    spawnSync(
      process.execPath,
      [
        "--input-type=module",
        "-e",
        `import {prepareIntelOrt} from ${JSON.stringify(moduleUrl)}; await prepareIntelOrt(${JSON.stringify(root)}, {url:"http://fixture.invalid/source.tar.gz",sha256:"4d5a9a5bd2ba0b29e2b537866d1d87663534788358e6d5a5b0ea4f570c55c604"});`,
      ],
      {
        encoding: "utf8",
        env: {
          ...process.env,
          PATH: bin,
          CALLS: calls,
          TEST_ARCH: arch,
          SOURCE_VERSION: version,
          ARCHIVE_CONTENT: corrupt ? "tampered" : "synthetic source archive",
        },
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
  assert.equal(calls.filter((c) => c.command === "curl").length, 1);
  assert.equal(calls.filter((c) => c.command === "bash").length, 1);
  const clone = calls.find((c) => c.command === "curl");
  assert.ok(clone.args.includes("--fail"));
  assert.equal(calls.filter((c) => c.command === "tar").length, 1);
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

test("source checksum mismatch aborts before extraction or compilation", async (t) => {
  const f = await fixture(t, "x86_64", "1.30.0", true);
  const result = f.run();
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /source SHA-256 mismatch/);
  const calls = await readFile(f.calls, "utf8");
  assert.doesNotMatch(calls, /"command":"tar"|"command":"bash"/);
});

test("canonical dependency archive URLs preserve upstream references and hashes", async () => {
  const { canonicalDependencyUrls } = await import("../../scripts/build-ort.mjs");
  const input =
    "mp11;https://github.com/boostorg/mp11/archive/refs/tags/boost-1.82.0.zip;9bc9e01dffb64d9e0773b2e44d2f22c51aace063\nother;https://github.com/org/repo/archive/abc123.tar.gz;unchanged";
  const output = canonicalDependencyUrls(input);
  assert.match(
    output,
    /https:\/\/codeload.github.com\/boostorg\/mp11\/zip\/refs\/tags\/boost-1.82.0;9bc9e01dffb64d9e0773b2e44d2f22c51aace063/,
  );
  assert.match(output, /https:\/\/codeload.github.com\/org\/repo\/tar.gz\/abc123;unchanged/);
});

test("canonical archive URLs handle upstream commit archives with a cosmetic filename", async () => {
  const { canonicalDependencyUrls } = await import("../../scripts/build-ort.mjs");
  const sha = "1d8b82b0740839c0de7f1242a3585e3390ff5f33";
  assert.equal(
    canonicalDependencyUrls(
      `https://github.com/eigen-mirror/eigen/archive/${sha}/eigen-${sha}.zip`,
    ),
    `https://codeload.github.com/eigen-mirror/eigen/zip/${sha}`,
  );
});
