// The browser smoke test of the gate (ADR-0021 section 4 and its acceptance): Chromium headless
// opens the run page of a fresh run of the gate scenario at the end of year 28, finds the
// seven curves, steps one year, opens sim 13 and clicks its mother, sim 8, in the pedigree
// (P11). Sim 13 with mother 8 is in `tests/golden/dummy_pedigree.json`. Any error on the
// page, in its console or a refusal fails it. Usage: node smoke/smoke.mjs <url> <run>
import { chromium } from "playwright-core";

const [url, run] = process.argv.slice(2);
const STEP_MS = 10_000;

function fail(message) {
  console.error(`smoke: ${message}`);
  process.exitCode = 1;
}

let browser;
try {
  browser = await chromium.launch({ headless: true });
} catch (error) {
  fail(`chromium is missing or does not start (run \`cargo xtask browser\`): ${error.message}`);
}

if (browser !== undefined) {
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(STEP_MS);
    const errors = [];
    // The first error on the page ends the test at once: a page that throws while it loads
    // must not wait for a step's time limit.
    let broke;
    const broken = new Promise((_, reject) => {
      broke = reject;
    });
    broken.catch(() => {});
    page.on("pageerror", (error) => {
      errors.push(error.message);
      broke(new Error(`the page threw: ${error.message}`));
    });
    page.on("console", (message) => {
      // The server serves no icon; the browser's own request for it is no error of the page.
      if (message.type() === "error" && !message.location().url.endsWith("/favicon.ico")) {
        errors.push(message.text());
      }
    });
    const step = (what, promise) =>
      Promise.race([promise, broken]).catch((error) => {
        throw new Error(`${what}: ${error.message}`);
      });
    const dated = (year) =>
      page.waitForFunction(
        (prefix) => document.querySelector("section > h1 + p")?.textContent?.startsWith(prefix) ?? false,
        `Jahr ${year},`,
      );

    await step("open the run", page.goto(`${url}#/runs/${run}`));
    await step("the run's heading", page.getByRole("heading", { level: 1, name: `Lauf ${run}` }).waitFor());
    await step("the date of year 28", dated(28));
    await step("seven curves", page.locator("figure svg[role=img]").nth(6).waitFor());
    const figures = await page.locator("figure").count();
    if (figures !== 7) {
      throw new Error(`seven curves: ${figures} pictures`);
    }
    const year = page.getByRole("group", { name: "Zeit" }).getByRole("button", { name: "Ein Jahr" });
    await step("step one year", year.click());
    await step("the date of year 29", dated(29));
    await step("the steps free again", page.waitForFunction(() =>
      [...document.querySelectorAll('[aria-label="Zeit"] button')].every((button) => !button.disabled),
    ));
    if ((await page.getByRole("alert").count()) > 0) {
      throw new Error(`a refusal: ${await page.getByRole("alert").first().textContent()}`);
    }
    await step("open sim 13", page.locator(`a[href="#/runs/${run}/persons/13"]`).first().click());
    const mother = page.locator('svg[aria-label="Stammbaum"] a[data-sim="8"]');
    await step("its mother in the pedigree", mother.waitFor());
    const before = await page.locator("h1").textContent();
    await step("click its mother", mother.click());
    await step(
      "the mother's page",
      page.waitForFunction(
        ([hash, name]) => location.hash === hash && document.querySelector("h1")?.textContent !== name,
        [`#/runs/${run}/persons/8`, before],
      ),
    );
    if (errors.length > 0) {
      throw new Error(`errors on the page: ${errors.join("; ")}`);
    }
    console.log("smoke: the run page in year 28 and 29, sim 13, then its mother sim 8");
  } catch (error) {
    fail(error.message);
  } finally {
    await browser.close();
  }
}
