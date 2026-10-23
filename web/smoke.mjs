import assert from "node:assert/strict";
import { createReadStream, existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { PNG } from "pngjs";

const root = join(fileURLToPath(new URL("..", import.meta.url)), "target/web");
const pack = join(root, "smoke-pack.zip");
assert(existsSync(pack), `smoke resource pack missing: ${pack}`);
const referencePath = join(fileURLToPath(new URL(".", import.meta.url)), "demo-reference.png");
assert(existsSync(referencePath), `native demo reference missing: ${referencePath}`);

const types = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
};
const server = createServer((request, response) => {
  const path = normalize(join(root, decodeURIComponent(new URL(request.url, "http://localhost").pathname)));
  if (!path.startsWith(root) || !existsSync(path) || !statSync(path).isFile()) {
    response.writeHead(404).end();
    return;
  }
  response.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
  createReadStream(path).pipe(response);
});

const address = await new Promise((resolve) =>
  server.listen(0, "127.0.0.1", () => resolve(server.address())),
);
const browser = await chromium.launch({
  headless: true,
  args: ["--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-angle=swiftshader"],
});
try {
  const page = await browser.newPage({ viewport: { width: 960, height: 540 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.stack ?? String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
    if (message.type() === "log") console.log(`browser: ${message.text()}`);
  });

  await page.goto(`http://127.0.0.1:${address.port}/index.html`);
  const canvas = page.locator("canvas");
  await canvas.waitFor();
  await page.locator('input[type="file"]').waitFor();
  await page.evaluate(() => {
    const input = document.querySelector('input[type="file"]');
    if (input) input.style.display = "none";
  });
  await page.waitForTimeout(1000);
  const before = PNG.sync.read(await canvas.screenshot());

  await page.locator('input[type="file"]').setInputFiles(pack);
  await page.waitForFunction(() => document.body.dataset.packLoaded === "true", null, {
    timeout: 120_000,
  });
  await page.waitForTimeout(1000);
  const canvasSize = await canvas.evaluate((element) => [
    element.width,
    element.height,
    element.clientWidth,
    element.clientHeight,
  ]);
  assert.equal(canvasSize[0], canvasSize[2], "canvas backing store width differs from css");
  assert.equal(canvasSize[1], canvasSize[3], "canvas backing store height differs from css");
  const afterShot = await canvas.screenshot();
  writeFileSync(join(root, "web-demo.png"), afterShot);
  const after = PNG.sync.read(afterShot);
  assert.equal(after.width, before.width);
  assert.equal(after.height, before.height);

  let changed = 0;
  for (let pixel = 0; pixel < after.width * after.height; pixel++) {
    const offset = pixel * 4;
    const difference = Math.abs(after.data[offset] - before.data[offset])
      + Math.abs(after.data[offset + 1] - before.data[offset + 1])
      + Math.abs(after.data[offset + 2] - before.data[offset + 2]);
    if (difference > 12) changed++;
  }
  assert(changed > after.width * after.height * 0.01, `pack upload changed only ${changed} pixels`);

  const reference = PNG.sync.read(readFileSync(referencePath));
  assert.equal(after.width, reference.width, "web demo width differs from native reference");
  assert.equal(after.height, reference.height, "web demo height differs from native reference");
  let diffSum = 0;
  let over16 = 0;
  for (let pixel = 0; pixel < after.width * after.height; pixel++) {
    const offset = pixel * 4;
    let maxDiff = 0;
    for (let channel = 0; channel < 3; channel++) {
      const difference = Math.abs(after.data[offset + channel] - reference.data[offset + channel]);
      diffSum += difference / 3;
      maxDiff = Math.max(maxDiff, difference);
    }
    if (maxDiff > 16) over16++;
  }
  const pixels = after.width * after.height;
  const mae = diffSum / pixels;
  const overFraction = over16 / pixels;
  assert(mae < 1.2, `web demo MAE vs native reference is ${mae.toFixed(3)}`);
  assert(overFraction < 0.025, `web demo has ${(overFraction * 100).toFixed(2)}% pixels over 16`);

  assert.deepEqual(errors, [], `browser errors: ${errors.join("\n")}`);
  console.log(`wasm viewer smoke passed; ${changed} pixels changed after OPFS pack load`);
  console.log(`canvas backing store ${canvasSize[0]}x${canvasSize[1]}, css ${canvasSize[2]}x${canvasSize[3]}`);
  console.log(
    `native demo reference: mae ${mae.toFixed(3)}, over-16 ${(overFraction * 100).toFixed(2)}%`,
  );
} finally {
  await browser.close();
  await new Promise((resolve) => server.close(resolve));
}
