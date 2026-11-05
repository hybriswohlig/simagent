// The second drive of the browser smoke test (ADR-0021, addendum of 2026-11-05), after a
// restart of the server over the runs of the first (smoke/smoke.mjs): the town founded there
// with its own values (`eigen`, partnerships from 18, two decades on) is listed with its mark
// and the towns of the base game without one; it opens at the end of year 19 with its mark
// and steps 28 days on into year 20. Any error on the page, in its console or a refusal fails
// it. Usage: node smoke/reopen.mjs <url> <run>
import { chromium } from "playwright-core";

const [url, run] = process.argv.slice(2);
const STEP_MS = 10_000;
/** The town smoke/smoke.mjs founded with own values, and one it founded without. */
const OWN = "eigen";
const PLAIN = "neu";

function fail(message) {
  console.error(`smoke: ${message}`);
  process.exitCode = 1;
}

let browser;
try {
  browser = await chromium.launch({ headless: true, timeout: 30_000 });
} catch (error) {
  fail(`chromium is missing or does not start (run \`cargo xtask browser\`): ${error.message}`);
}

if (browser !== undefined) {
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(STEP_MS);
    const errors = [];
    // The first error on the page ends the test at once.
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
    const entry = (name) => page.getByRole("listitem").filter({ has: page.getByRole("link", { name, exact: true }) });

    await step("the list of runs", page.goto(`${url}#/`));
    await step("the own town marked in the list", entry(OWN).getByText(/eigene Werte: Partnerschaften ab 18 Jahren$/).waitFor());
    for (const name of [run, PLAIN]) {
      await step(`${name} listed`, entry(name).waitFor());
      if ((await entry(name).getByText("eigene Werte").count()) !== 0) {
        throw new Error(`${name} is marked, yet it has no own values`);
      }
    }
    await step("open it", entry(OWN).getByRole("link", { name: OWN, exact: true }).click());
    await step("its page", page.getByRole("heading", { level: 1, name: `Lauf ${OWN}` }).waitFor());
    await step("two decades on", dated(19));
    await step("its mark", page.getByText(/^eigene Werte: Partnerschaften ab 18 Jahren$/).waitFor());
    const step28 = page.getByRole("group", { name: "Zeit" }).getByRole("button", { name: "28 Tage", exact: true });
    await step("28 days on", step28.click());
    await step("in year 20", dated(20));
    if ((await page.getByRole("alert").count()) > 0) {
      throw new Error(`a refusal: ${await page.getByRole("alert").first().textContent()}`);
    }
    if (errors.length > 0) {
      throw new Error(`errors on the page: ${errors.join("; ")}`);
    }
    console.log("smoke: after a restart the town with partnerships from 18 marked in the list and on its page, the towns of the base game unmarked, and 28 days on into year 20");
  } catch (error) {
    fail(error.message);
  } finally {
    await browser.close();
  }
}
