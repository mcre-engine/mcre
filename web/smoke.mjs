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
const save = join(root, "smoke-save.zip");
assert(existsSync(save), `smoke world save missing: ${save}`);
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
  page.on("pageerror", (error) => {
    const text = error.stack ?? String(error);
    errors.push(text);
    console.error(`browser page error: ${text}`);
  });
  page.on("console", (message) => {
    if (message.type() === "error") {
      errors.push(message.text());
      console.error(`browser error: ${message.text()}`);
    }
    if (message.type() === "log") console.log(`browser: ${message.text()}`);
  });

  await page.goto(`http://127.0.0.1:${address.port}/index.html`);
  const canvas = page.locator("canvas");
  await canvas.waitFor();
  await page.locator("#pack-input").waitFor();
  await page.locator("#world-input").waitFor();
  await page.evaluate(() => {
    for (const input of document.querySelectorAll('input[type="file"]')) {
      input.style.display = "none";
    }
    document.querySelector("#debug-toggle").style.visibility = "hidden";
  });
  await page.waitForTimeout(1000);
  const before = PNG.sync.read(await canvas.screenshot());

  await page.locator("#pack-input").setInputFiles(pack);
  await page.waitForFunction(() => document.body.dataset.packLoaded === "true", null, {
    timeout: 120_000,
  });
  await page.waitForFunction(() => document.body.dataset.animationTick !== undefined, null, {
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
  await page.evaluate(() => {
    document.querySelector("#debug-toggle").style.visibility = "visible";
  });

  await page.locator("#world-input").setInputFiles(save);
  await page.waitForFunction(() => document.body.dataset.saveLoaded === "true", null, {
    timeout: 120_000,
  });
  await page.waitForTimeout(750);
  const saveShot = await canvas.screenshot();
  writeFileSync(join(root, "web-save.png"), saveShot);
  const saved = PNG.sync.read(saveShot);
  assert.equal(saved.width, after.width);
  assert.equal(saved.height, after.height);
  let saveChanged = 0;
  for (let pixel = 0; pixel < saved.width * saved.height; pixel++) {
    const offset = pixel * 4;
    const difference = Math.abs(saved.data[offset] - after.data[offset])
      + Math.abs(saved.data[offset + 1] - after.data[offset + 1])
      + Math.abs(saved.data[offset + 2] - after.data[offset + 2]);
    if (difference > 12) saveChanged++;
  }
  assert(
    saveChanged > saved.width * saved.height * 0.01,
    `save import changed only ${saveChanged} pixels`,
  );

  const downloadPromise = page.waitForEvent("download");
  await page.locator("#download-save").click();
  const download = await downloadPromise;
  assert.equal(download.suggestedFilename(), "world.zip");
  assert.equal(await download.failure(), null);

  await page.locator("#debug-toggle").click();
  await page.waitForFunction(() => !document.querySelector("#debug-menu").hidden);
  await page.waitForFunction(() =>
    document.querySelector("#debug-info").textContent.includes("Chunks:"),
  );
  await page.locator("#time-of-day").evaluate((input) => {
    input.value = "12000";
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  assert.equal(await page.locator("#time-value").textContent(), "12000 ticks");
  await page.waitForFunction(() =>
    document.querySelector("#debug-info").textContent.includes("Time: 12000"),
  );
  await page.locator("#brightness").evaluate((input) => {
    input.value = "0.75";
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  assert.equal(await page.locator("#brightness-value").textContent(), "0.75");
  await page.locator("#render-distance").evaluate((input) => {
    input.value = "2";
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  assert.equal(await page.locator("#render-distance-value").textContent(), "2 chunks");
  const chunksAtTwoText = await page.locator("#debug-info").textContent();
  const chunksAtTwo = Number(chunksAtTwoText.match(/Chunks: (\d+)/)?.[1]);
  assert(Number.isFinite(chunksAtTwo), `debug info did not report chunks: ${chunksAtTwoText}`);
  await page.locator("#render-distance").evaluate((input) => {
    input.value = "1";
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await page.waitForFunction((previous) => {
    const count = Number(
      document.querySelector("#debug-info").textContent.match(/Chunks: (\d+)/)?.[1],
    );
    return count < previous;
  }, chunksAtTwo);
  await page.locator("#unlit-enabled").check();
  await page.locator("#unlit-enabled").uncheck();
  await page.locator("#fog-enabled").uncheck();
  await page.locator("#fog-enabled").check();
  await page.keyboard.press("F1");
  await page.waitForFunction(() => document.querySelector("#debug-menu").hidden);
  await page.keyboard.press("F1");
  await page.waitForFunction(() => !document.querySelector("#debug-menu").hidden);
  await page.locator("#debug-toggle").click();

  await canvas.click({ position: { x: 480, y: 270 } });
  await page.waitForTimeout(200);
  if (await page.evaluate(() => document.pointerLockElement !== null)) {
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => document.pointerLockElement === null, null, {
      timeout: 2_000,
    });
  }

  assert.deepEqual(errors, [], `browser errors: ${errors.join("\n")}`);
  console.log(
    `wasm viewer smoke passed; ${changed} pixels changed after OPFS pack load, ${saveChanged} after save import`,
  );
  console.log(`canvas backing store ${canvasSize[0]}x${canvasSize[1]}, css ${canvasSize[2]}x${canvasSize[3]}`);
  console.log(
    `native demo reference: mae ${mae.toFixed(3)}, over-16 ${(overFraction * 100).toFixed(2)}%`,
  );
} finally {
  await browser.close();
  await new Promise((resolve) => server.close(resolve));
}
