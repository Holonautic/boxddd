import { expect, test } from "@playwright/test";

test("the catalog launches a live falling-stack simulation", async ({ page }) => {
  const runtimeErrors = [];
  const failedRequests = [];
  const responseStatuses = new Map();
  const streamedWasmPaths = new Set([
    "/bevy-testbed/generated/bevy_boxddd_testbed_bg.wasm",
    "/wasm/generated/box3d-sys-v2.wasm",
  ]);

  page.on("console", (message) => {
    if (message.type() === "error") {
      runtimeErrors.push(`console: ${message.text()}`);
    }
  });
  page.on("pageerror", (error) => runtimeErrors.push(`page: ${error.stack || error.message}`));
  page.on("response", (response) => responseStatuses.set(response.url(), response.status()));
  page.on("requestfailed", (request) => {
    failedRequests.push({
      error: request.failure()?.errorText,
      method: request.method(),
      url: request.url(),
    });
  });

  const catalogResponse = await page.goto("/");
  expect(catalogResponse?.ok()).toBe(true);

  const fallingStack = page.getByRole("link", { name: /Falling Stack/ });
  await expect(fallingStack).toBeVisible();
  await fallingStack.click();
  await expect(page).toHaveURL(/\/examples\/falling-stack\/$/);

  const app = page.locator("#bevy-app");
  await expect(app).toHaveAttribute("data-scene-id", "falling-stack");
  await page.waitForFunction(
    () => {
      const state = document.querySelector("#bevy-status")?.dataset.state;
      return window.BOXDDD_BEVY_EXAMPLE_READY === true || state === "error";
    },
    undefined,
    { timeout: 120_000 },
  );
  const runtimeState = await page.evaluate(() => ({
    detail: document.querySelector("#bevy-status span")?.textContent,
    exampleReady: window.BOXDDD_BEVY_EXAMPLE_READY === true,
    sceneId: window.BOXDDD_BEVY_SCENE_ID,
    status: document.querySelector("#bevy-status")?.dataset.state,
    testbedReady: window.BOXDDD_BEVY_TESTBED_READY === true,
  }));
  expect(runtimeState).toMatchObject({
    exampleReady: true,
    sceneId: "falling-stack",
    status: "running",
    testbedReady: true,
  });

  const canvas = page.locator("#bevy-canvas");
  await expect(canvas).toBeVisible();
  const bounds = await canvas.boundingBox();
  expect(bounds).not.toBeNull();
  expect(bounds.width).toBeGreaterThan(640);
  expect(bounds.height).toBeGreaterThan(360);

  const firstFrame = await canvas.screenshot();
  await expect
    .poll(async () => !(await canvas.screenshot()).equals(firstFrame), {
      intervals: [100, 200, 500],
      timeout: 10_000,
    })
    .toBe(true);

  await page.evaluate(() => {
    window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true }));
    window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true }));
  });
  const bfcacheFrame = await canvas.screenshot();
  await expect
    .poll(async () => !(await canvas.screenshot()).equals(bfcacheFrame), {
      intervals: [100, 200, 500],
      timeout: 10_000,
    })
    .toBe(true);

  for (const path of streamedWasmPaths) {
    expect(responseStatuses.get(new URL(path, page.url()).href)).toBe(200);
  }
  const unexpectedFailures = failedRequests.filter(({ error, url }) => {
    const path = new URL(url).pathname;
    // Chromium reports loader-consumed Response.body streams as aborted even
    // after a 200 response. Readiness and changing frames prove instantiation.
    return !(
      error === "net::ERR_ABORTED" &&
      streamedWasmPaths.has(path) &&
      responseStatuses.get(url) === 200
    );
  });
  expect(unexpectedFailures).toEqual([]);
  expect(runtimeErrors).toEqual([]);
});
