// The browser smoke test of the gate (ADR-0021 section 4 and its acceptance): Chromium headless
// opens the run page of a fresh run of the gate scenario at the end of year 28, finds the
// seven curves, triggers a hunger year (its line, for year 29, comes at once), steps one
// year, turns the hereditary diseases on (off in the scenario; its line and the button's new
// label come at once), opens sim 13 and clicks its mother, sim 8, in the pedigree (P11),
// opens the comparison of the run with itself (no life differs), goes back to the run page
// and finds the hunger year's line again (the chronicle kept), then forks the run from its
// year with the hereditary diseases off and lands on the comparison with the fork, steps
// the fork 28 days on and finds the comparison made at the run's second; last it pins the family of sim 13 and finds it on the run page, opens the sheet of portraits
// (ten rows of seven), and founds a town, which stands in its first second with its founders
// and says when its curves begin.
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
    // Once the smoke test asks for a refused pairing, the browser's line about its 409 is
    // expected.
    let refusalAsked = false;
    page.on("console", (message) => {
      // The server serves no icon; the browser's own request for it is no error of the page.
      const where = message.location().url;
      const refusal = refusalAsked && where.endsWith("/api/runs/neu/input") && message.text().includes("409");
      if (message.type() === "error" && !where.endsWith("/favicon.ico") && !refusal) {
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
    const forkYear = page.getByRole("spinbutton", { name: "Ab Jahr" });
    await step("a fork year typed", forkYear.fill("1"));
    const year = page.getByRole("group", { name: "Zeit" }).getByRole("button", { name: "Ein Jahr" });
    await step("step one year", year.click());
    await step("the date of year 29", dated(29));
    await step(
      "the fork year follows the run again",
      page.waitForFunction(() => document.querySelector('input[type="number"]')?.value === "29"),
    );
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
    // The fork 28 days further is compared at the run's second, recomputed up to it.
    await step("the fork's page", page.goto(`${url}#/runs/gabel`));
    await step("the fork's date", dated(29));
    const days = page.getByRole("group", { name: "Zeit" }).getByRole("button", { name: "28 Tage" });
    await step("the fork 28 days on", days.click());
    await step("the fork in year 30", dated(30));
    await step("the comparison again", page.goto(`${url}#/runs/${run}/diff/gabel`));
    await step(
      "compared at the run's second",
      page.getByText(
        new RegExp(`^Verglichen wird zum Stand von ${run} \\(Jahr 29, 13\\. Monat, 28\\. Tag\\); gabel ist dafür bis dorthin nachgerechnet, an ihm selbst ändert sich nichts\\.$`),
      ).waitFor(),
    );
    await step("sim 13 again", page.goto(`${url}#/runs/${run}/persons/13`));
    await step(
      "the kinship's own placeholder",
      page.getByLabel("Verwandtschaft mit").locator("option").first().filter({ hasText: /^Person wählen$/ }).waitFor({ state: "attached" }),
    );
    await step("pin its family", page.getByRole("button", { name: "Familie anheften" }).click());
    await step("pinned", page.getByRole("button", { name: "Nicht mehr anheften" }).waitFor());
    await step("back to the run with the family", page.goto(`${url}#/runs/${run}`));
    await step(
      "the pinned family",
      page.getByRole("heading", { level: 2, name: /^Angeheftete Familie: / }).waitFor(),
    );
    await step(
      "the comparison's placeholder",
      page.getByLabel("Vergleichen mit").locator("option").first().filter({ hasText: /^Lauf wählen$/ }).waitFor({ state: "attached" }),
    );
    // A short run page: the pinned family above the chronicle, which scrolls in its own box,
    // like the living.
    const short = await page.evaluate(() => {
      const headings = [...document.querySelectorAll("h2")].map((h) => h.textContent ?? "");
      const pinned = headings.findIndex((text) => text.startsWith("Angeheftete Familie: "));
      const scrolls = (list) => {
        const found = document.querySelector(list);
        return found !== null && getComputedStyle(found).overflowY === "auto";
      };
      return (
        pinned >= 0 &&
        pinned < headings.indexOf("Chronik") &&
        scrolls('ol[aria-label="Chronik"]') &&
        scrolls('ul[aria-label="Lebende"]')
      );
    });
    if (!short) {
      throw new Error("the run page: the pinned family not above a chronicle and living that scroll");
    }
    await step("the sheet of portraits", page.goto(`${url}#/portraits`));
    await step("its heading", page.getByRole("heading", { level: 1, name: "Bogen der Passbilder" }).waitFor());
    await step("its last portrait", page.getByRole("img", { name: "Helle Iris, männlich, 60 Jahre" }).waitFor());
    const portraits = await page.locator("table svg[role=img]").count();
    if (portraits !== 70) {
      throw new Error(`the sheet: ${portraits} portraits, not ten rows of seven`);
    }
    await step("the list of runs", page.goto(`${url}#/`));
    await step("a new town's name", page.getByRole("textbox", { name: "Name des Laufs" }).fill("neu"));
    await step("found it", page.getByRole("button", { name: "Neue Stadt gründen" }).click());
    await step("its page", page.getByRole("heading", { level: 1, name: "Lauf neu" }).waitFor());
    await step("its first second", dated(0));
    await step("its founders", page.locator('a[href^="#/runs/neu/persons/"]').first().waitFor());
    await step("no curves yet", page.getByText("Die Kurven beginnen, wenn das erste Jahr vorbei ist.").waitFor());
    await step("no fork in year 0, and why", page.getByText("Eine Gabel beginnt frühestens in Jahr 1.").waitFor());
    const forkControls =
      (await page.getByRole("spinbutton", { name: "Ab Jahr" }).count()) +
      (await page.getByRole("button", { name: "Gabel anlegen" }).count());
    if (forkControls !== 0) {
      throw new Error("year 0: the fork form still shows its controls");
    }
    // A pairing the core refuses: an offer of person A was paired elsewhere meanwhile (another
    // tab); after the refusal A's offers are read again and that one is gone.
    const api = (method, path, body) =>
      page.evaluate(
        async ([m, p, b]) => {
          const init = { method: m, headers: { "Content-Type": "application/json" } };
          const answer = await fetch(p, b === null ? init : { ...init, body: JSON.stringify(b) });
          return answer.json();
        },
        [method, path, body ?? null],
      );
    const offers = async (sim) => (await api("GET", `/api/runs/neu/persons/${sim}`)).partner_candidates ?? [];
    let pair = null;
    for (let sim = 1; pair === null && sim <= 40; sim++) {
      const mine = await offers(sim);
      if (mine.length > 0) {
        const other = (await offers(mine[0])).find((candidate) => candidate !== sim);
        pair = other === undefined ? null : [sim, mine[0], other];
      }
    }
    if (pair === null) {
      throw new Error("the new town: no person with an offer that has another offer");
    }
    const [a, x, y] = pair;
    await step("a person with offers", page.goto(`${url}#/runs/neu/persons/${a}`));
    const choice = page.getByRole("combobox", { name: "Mit wem?" });
    await step("the offer", choice.locator(`option[value="${x}"]`).waitFor({ state: "attached" }));
    await step("chosen", choice.selectOption(String(x)));
    const female = (await api("GET", `/api/runs/neu/persons/${x}`)).sex === "female";
    const elsewhere = { arrange_partnership: female ? { woman: x, man: y } : { woman: y, man: x } };
    if ((await api("POST", "/api/runs/neu/input", elsewhere)).format !== "simagent.view.input") {
      throw new Error("the pairing elsewhere was refused");
    }
    refusalAsked = true;
    await step("pair", page.getByRole("button", { name: "Paar stiften" }).click());
    await step("the refusal", page.getByText("Eine der beiden Personen ist schon in einer Partnerschaft.").waitFor());
    await step(
      "the refused offer gone",
      choice.locator(`option[value="${x}"]`).waitFor({ state: "detached" }),
    );
    if (errors.length > 0) {
      throw new Error(`errors on the page: ${errors.join("; ")}`);
    }
    console.log("smoke: the run page in year 28 and 29 with a hunger and the switch, sim 13, then its mother sim 8, the comparison with itself, the chronicle kept, the comparison with a fork, a pinned family above a chronicle that scrolls, the sheet of portraits, and a new town in its first second with a refused pairing");
  } catch (error) {
    fail(error.message);
  } finally {
    await browser.close();
  }
}
