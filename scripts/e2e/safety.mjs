import { Buffer } from "node:buffer";
import { readFileSync } from "node:fs";
import { createServer } from "node:net";

export function assertAutomationBinary(file) {
  const binary = readFileSync(file);
  for (const marker of ["E2E builds require CLIPSEEK_E2E_ROOT", "org.clipstudio.open.e2e"]) {
    if (!binary.includes(Buffer.from(marker)))
      throw new Error(
        "Refusing to launch a non-isolated E2E binary; rebuild with npm run test:e2e",
      );
  }
}
export async function freePort() {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const port = server.address().port;
  await new Promise((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
  return port;
}
