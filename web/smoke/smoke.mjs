// The browser smoke test of the gate (ADR-0021 section 4 and its acceptance): Chromium headless
// opens the run page of a fresh run of the gate scenario at the end of year 28, finds the
// seven curves, triggers a hunger year (its line, for year 29, comes at once), steps one
// year, turns the hereditary diseases on (off in the scenario; its line and the button's new
// label come at once), opens sim 13 and clicks its mother, sim 8, in the pedigree (P11),
// opens the comparison of the run with itself (no life differs), goes back to the run page
// and finds the hunger year's line again (the chronicle kept), then forks the run from its
// year with the hereditary diseases off and lands on the comparison with the fork; last
// it pins the family of sim 13 and finds it on the run page, and founds a town, which stands
// in its first second with its founders and says when its curves begin.
// Sim 13 with mother 8 is in `tests/golden/dummy_pedigree.json`. Any error on the page, in
// its console or a refusal fails it. Usage: node smoke/smoke.mjs <url> <run>
import { chromium } from "playwright-core";

const [url, run] = process.argv.slice(2);
const STEP_MS = 10_000;

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
    const interventions = page.getByRole("group", { name: "Eingriffe" });
    await step("a hunger year", interventions.getByRole("button", { name: "Ein Hungerjahr" }).click());
    await step(
      "its line in the chronicle",
      page.getByText("Jahr 29, 1. Monat, 1. Tag: Ein Hungerjahr kommt: Jahr 29.").waitFor(),
    );
    const year = page.getByRole("group", { name: "Zeit" }).getByRole("button", { name: "Ein Jahr" });
    await step("step one year", year.click());
    await step("the date of year 29", dated(29));
    await step("the steps free again", page.waitForFunction(() => {
      const buttons = [...document.querySelectorAll('[aria-label="Zeit"] button')];
      return buttons.length > 0 && buttons.every((button) => !button.disabled);
    }));
    const on = interventions.getByRole("button", { name: "Erbkrankheiten anschalten" });
    await step("turn the hereditary diseases on", on.click());
    await step("its line", page.getByText("Die Erbkrankheiten sind von nun an angeschaltet.").waitFor());
    await step(
      "the switch's new label",
      interventions.getByRole("button", { name: "Erbkrankheiten ausschalten" }).waitFor(),
    );
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
        ([hash, name]) => {
          const heading = document.querySelector("h1");
          return location.hash === hash && heading !== null && heading.textContent !== name;
        },
        [`#/runs/${run}/persons/8`, before],
      ),
    );
    await step("the comparison with itself", page.goto(`${url}#/runs/${run}/diff/${run}`));
    await step(
      "its heading",
      page.getByRole("heading", { level: 1, name: `Vergleich: ${run} und ${run}` }).waitFor(),
    );
    await step("no life differs", page.getByText("Die beiden Läufe unterscheiden sich in keinem Leben.").waitFor());
    await step("the years of both", page.getByText(`${run} steht im Jahr 29, ${run} im Jahr 29.`).waitFor());
    await step("back to the run", page.goto(`${url}#/runs/${run}`));
    await step(
      "the chronicle kept",
      page.getByText("Jahr 29, 1. Monat, 1. Tag: Ein Hungerjahr kommt: Jahr 29.").waitFor(),
    );
    await step("with the switch's line", page.getByText("Die Erbkrankheiten sind von nun an angeschaltet.").waitFor());
    await step("the fork's name", page.getByRole("textbox", { name: "Name der Gabel" }).fill("gabel"));
    await step("the diseases off", page.getByLabel("Erbkrankheiten in der Gabel").selectOption("off"));
    await step("fork", page.getByRole("button", { name: "Gabel anlegen" }).click());
    await step(
      "the comparison with the fork",
      page.getByRole("heading", { level: 1, name: `Vergleich: ${run} und gabel` }).waitFor(),
    );
    if ((await page.getByRole("alert").count()) > 0) {
      throw new Error(`a refusal: ${await page.getByRole("alert").first().textContent()}`);
    }
    await step("sim 13 again", page.goto(`${url}#/runs/${run}/persons/13`));
    await step("pin its family", page.getByRole("button", { name: "Familie anheften" }).click());
    await step("pinned", page.getByRole("button", { name: "Nicht mehr anheften" }).waitFor());
    await step("back to the run with the family", page.goto(`${url}#/runs/${run}`));
    await step(
      "the pinned family",
      page.getByRole("heading", { level: 2, name: /^Angeheftete Familie: / }).waitFor(),
    );
    await step("the list of runs", page.goto(`${url}#/`));
    await step("a new town's name", page.getByRole("textbox", { name: "Name des Laufs" }).fill("neu"));
    await step("found it", page.getByRole("button", { name: "Neue Stadt gründen" }).click());
    await step("its page", page.getByRole("heading", { level: 1, name: "Lauf neu" }).waitFor());
    await step("its first second", dated(0));
    await step("its founders", page.locator('a[href^="#/runs/neu/persons/"]').first().waitFor());
    await step("no curves yet", page.getByText("Die Kurven beginnen, wenn das erste Jahr vorbei ist.").waitFor());
    if (errors.length > 0) {
      throw new Error(`errors on the page: ${errors.join("; ")}`);
    }
    console.log("smoke: the run page in year 28 and 29 with a hunger and the switch, sim 13, then its mother sim 8, the comparison with itself, the chronicle kept, the comparison with a fork, a pinned family, and a new town in its first second");
  } catch (error) {
    fail(error.message);
  } finally {
    await browser.close();
  }
}
