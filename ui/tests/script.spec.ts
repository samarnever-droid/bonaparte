/**
 * Built-in scripting: the Script dialog speaks the MCP tool surface, runs on
 * the live session, lands in the undo history, and reports per-step results.
 * Real bridge, real session, no mocks.
 */
import { test, expect, type APIRequestContext } from "@playwright/test";

const headers = { "X-Bonaparte-Client": "editor" };

async function command(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers, data });
  const body = await response.text();
  expect(response.ok(), body).toBeTruthy();
  return JSON.parse(body);
}

test("the script dialog runs tools and the editor can undo what it did", async ({
  page,
  request,
}) => {
  test.setTimeout(90_000);
  await request.post("/api/new_project", { headers, data: {} });
  await page.goto("/");
  await expect(page.locator(".sidebar")).toBeVisible();

  await page.getByRole("button", { name: /^File$/ }).click();
  await page.getByRole("button", { name: /Run script/ }).click();

  const editor = page.getByLabel("Script JSON");
  await expect(editor).toBeVisible();
  // The tool chips come from the live describe — one source of truth for
  // what a script may call.
  const chips = page.locator(".tool-chip");
  await expect(chips.first()).toBeVisible({ timeout: 15_000 });
  expect(await chips.count()).toBeGreaterThanOrEqual(10);

  // Clicking a chip appends a step; then run a two-step script.
  await chips.filter({ hasText: "op.apply" }).click();
  await expect(editor).toHaveValue(/"op\.apply"/);
  await editor.fill(
    '[\n  { "tool": "op.apply", "args": { "op": { "type": "renameProject", "name": "scripted!" } } },\n  { "tool": "project.info", "args": {} }\n]',
  );
  await page.getByRole("button", { name: /Run script/ }).last().click();

  await expect(page.locator(".script-step")).toHaveCount(2, { timeout: 20_000 });
  await expect(page.locator(".script-step").first()).toContainText("op.apply");
  await expect(page.locator(".script-step.bad")).toHaveCount(0);

  // Server-side truth moved, and the client store re-synced on its own.
  const state = await command(request, "state");
  expect(state.project.name).toBe("scripted!");
  await expect(page.locator(".topbar, .sidebar").first()).toBeVisible();

  // The scripted rename is one undo step for the human.
  await page.keyboard.press("Escape");
  await page.keyboard.press("Control+z");
  await expect
    .poll(async () => (await command(request, "state")).project.name, { timeout: 15_000 })
    .not.toBe("scripted!");
});

test("a failing tool is reported in the dialog, not swallowed", async ({ page, request }) => {
  test.setTimeout(60_000);
  await request.post("/api/new_project", { headers, data: {} });
  await page.goto("/");
  await page.getByRole("button", { name: /^File$/ }).click();
  await page.getByRole("button", { name: /Run script/ }).click();
  await page
    .getByLabel("Script JSON")
    .fill('[\n  { "tool": "not.a.real.tool", "args": {} }\n]');
  await page.getByRole("button", { name: /Run script/ }).last().click();
  const step = page.locator(".script-step");
  await expect(step).toHaveCount(1, { timeout: 15_000 });
  await expect(step.locator(".step-err")).toContainText(/Unknown tool/i);
});
