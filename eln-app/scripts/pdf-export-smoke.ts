/** Isolated visual QA: no application server, SQLite, or user-data access.
 * Run: node --import tsx scripts/pdf-export-smoke.ts
 * Open the printed URL, run the fixture, then pass the printed scratch folder
 * to the ignored Rust render_visual_fixture test via LABFLOW_PDF_QA_DIR.
 */
import { createServer } from "vite";
import { mkdtemp, writeFile, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { IncomingMessage, ServerResponse } from "node:http";

const output = await mkdtemp(join(tmpdir(), "labflow-pdf-qa-"));
const middleware = async (
  request: IncomingMessage,
  response: ServerResponse,
) => {
  const page = Number(request.url?.slice(1));
  if (!Number.isInteger(page) || page < 0 || page > 1000) {
    response.statusCode = 400;
    response.end();
    return;
  }
  const path = join(output, `page-${String(page).padStart(4, "0")}.jpg`);
  try {
    if (request.method === "POST") {
      let size = 0;
      const chunks: Buffer[] = [];
      for await (const chunk of request) {
        size += chunk.length;
        if (size > 8 * 1024 * 1024) throw new Error("page too large");
        chunks.push(chunk);
      }
      await writeFile(path, Buffer.concat(chunks));
      response.end("ok");
    } else if (request.method === "GET") {
      response.setHeader("Content-Type", "image/jpeg");
      response.end(await readFile(path));
    } else {
      response.statusCode = 405;
      response.end();
    }
  } catch {
    response.statusCode = 500;
    response.end("QA page failed");
  }
};
const server = await createServer({
  server: { host: "127.0.0.1", port: 1425, strictPort: true },
  plugins: [
    {
      name: "labflow-pdf-qa",
      configureServer(server) {
        server.middlewares.use("/__pdf_qa", middleware);
      },
    },
  ],
});
await server.listen();
console.log(
  `Visual QA: http://127.0.0.1:1425/tests/pdf-export-fixture.html\nScratch directory: ${output}`,
);
