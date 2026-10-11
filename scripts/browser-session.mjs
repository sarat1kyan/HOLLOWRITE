// Drives the real web client in headless Chromium: register or sign in,
// create/select a character, walk, and save screenshots. Used for the
// cross-client walkthrough and for reproducible captures.
//
// usage: node scripts/browser-session.mjs <url> <name> <password> <outdir> [walk-keys]
import { createRequire } from "node:module";
import { execSync } from "node:child_process";

const require = createRequire(import.meta.url);
let playwright;
try {
  playwright = require("playwright");
} catch {
  playwright = require(execSync("npm root -g").toString().trim() + "/playwright");
}

const [url = "http://127.0.0.1:5173", name = "Bob", password = "bell-tower-123", out = ".", walk = ""] = process.argv.slice(2);
const browser = await playwright.chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => m.type() === "error" && errors.push(m.text()));

await page.goto(url);
await page.waitForSelector("#username, .chars, #cname, canvas", { timeout: 15000 });
if (await page.$("#username")) {
  await page.getByRole("button", { name: "New account" }).click();
  await page.fill("#username", name);
  await page.fill("#password", password);
  await page.getByRole("button", { name: "Create account" }).click();
  await page.waitForTimeout(1500);
  if (await page.$("#username")) {
    await page.getByRole("button", { name: "I have an account" }).click();
    await page.fill("#username", name);
    await page.fill("#password", password);
    await page.getByRole("button", { name: "Sign in" }).click();
  }
}
await page.waitForSelector(".chars, #cname, canvas", { timeout: 15000 });
if (await page.$("#cname")) {
  await page.fill("#cname", name);
  await page.getByLabel("colour 4").click();
  await page.getByRole("button", { name: "Wake" }).click();
  await page.waitForSelector(".chars", { timeout: 10000 });
}
if (await page.$(".chars")) {
  await page.locator(".chars button", { hasText: name }).first().click();
}
await page.waitForSelector("canvas", { timeout: 10000 });
await page.waitForTimeout(1500);
await page.screenshot({ path: `${out}/web-${name}-spawn.png` });
for (const step of walk.split(",").filter(Boolean)) {
  const [key, ms] = step.split(":");
  await page.keyboard.down(key);
  await page.waitForTimeout(Number(ms || 300));
  await page.keyboard.up(key);
}
if (process.env.FIGHT_MS) {
  const until = Date.now() + Number(process.env.FIGHT_MS);
  let shot = false;
  while (Date.now() < until) {
    await page.keyboard.press("KeyJ");
    await page.waitForTimeout(600);
    if (!shot && Date.now() > until - Number(process.env.FIGHT_MS) / 2) {
      await page.screenshot({ path: `${out}/web-${name}-fight.png` });
      shot = true;
    }
  }
}
if (process.env.SAY) {
  await page.keyboard.press("Enter");
  await page.keyboard.type(process.env.SAY);
  await page.keyboard.press("Enter");
}
await page.waitForTimeout(800);
await page.screenshot({ path: `${out}/web-${name}-after.png` });
const hud = await page.locator(".hud").innerText();
console.log(JSON.stringify({ hud, errors }));
if (process.env.HOLD_MS) await page.waitForTimeout(Number(process.env.HOLD_MS));
await browser.close();
