// The browser smoke test of the gate (ADR-0021 section 4 and its acceptance): Chromium headless
// opens the run page of a fresh run of the gate scenario at the end of year 28, finds the
// five curves with the affected and the inbreeding folded apart, the empty chronicle's hint and
// the step the play button plays, triggers a hunger year (its line, for year 29, comes at once),
// steps one year, unfolds the hereditary diseases and turns them on (off in the scenario; its
// line, the fold's new state and the button's new label come at once), opens the people and
// filters them for blond hair (the count changes), goes back, opens sim 13, which
// stands at one age beside its parents, its mother sim 8 among them, and whose card folds its
// hereditary diseases, and clicks its mother in the pedigree (P11),
// opens the comparison of the run with itself (no life differs), goes back to the run page
// and finds the hunger year's line again (the chronicle kept), then forks the run from its
// year with the hereditary diseases off (unfolded in the form) and lands on the comparison with the fork, finds where
// the fork begins on its page, steps the fork 28 days on and finds the comparison made at the run's second; last it pins the family of sim 13 and finds it on the run page, opens the sheet of portraits with a height under each
// (ten rows of seven), and founds a town, which stands in its first second with its founders
// and says when its curves begin; last the server falls silent and loading again ends the
// message that it does not answer, and once more after a step alone, where the message ends
// by itself once the server answers.
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
    // While the smoke test cuts the server's answers off, the browser's lines about them too.
    let silenced = false;
    page.on("console", (message) => {
      // The server serves no icon; the browser's own request for it is no error of the page.
      const where = message.location().url;
      const refusal = refusalAsked && where.endsWith("/api/runs/neu/input") && message.text().includes("409");
      const cut = silenced && message.text().includes("net::ERR_FAILED");
      if (message.type() === "error" && !where.endsWith("/favicon.ico") && !refusal && !cut) {
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
    await step("five curves", page.locator("figure svg[role=img]").nth(4).waitFor());
    const figures = await page.locator("figure:visible").count();
    if (figures !== 5) {
      throw new Error(`five curves: ${figures} pictures`);
    }
    const folded = page.locator("details", { has: page.locator("summary", { hasText: /^Erbkrankheiten und Inzucht$/ }) });
    await step("the affected and the inbreeding folded", folded.waitFor());
    if ((await folded.locator("figure").count()) !== 2 || (await folded.locator("figure:visible").count()) !== 0) {
      throw new Error("the affected and the inbreeding: not two folded pictures");
    }
    await step(
      "an empty chronicle says when it begins",
      page.getByText("Die Chronik beginnt mit dem nächsten Schritt oder Eingriff.").waitFor(),
    );
    await step("playing 28 days at a time", page.getByRole("button", { name: "Abspielen, je 28 Tage" }).waitFor());
    const interventions = page.getByRole("group", { name: "Eingriffe" });
    await step("a hunger year", interventions.getByRole("button", { name: "Ein Hungerjahr" }).click());
    await step(
      "its line in the chronicle",
      page.getByText("Jahr 29, 1. Monat, 1. Tag: Ein Hungerjahr kommt: Jahr 29.").waitFor(),
    );
    const forkYear = page.getByRole("spinbutton", { name: "Ab Jahr" });
    await step("a fork year typed", forkYear.fill("1"));
    const year = page.getByRole("group", { name: "Zeit" }).getByRole("button", { name: "Ein Jahr", exact: true });
    await step("step one year", year.click());
    await step("the date of year 29", dated(29));
    await step("playing a year at a time", page.getByRole("button", { name: "Abspielen, je ein Jahr" }).waitFor());
    await step(
      "the fork year follows the run again",
      page.waitForFunction(() => document.querySelector('input[type="number"]')?.value === "29"),
    );
    await step("the steps free again", page.waitForFunction(() => {
      const buttons = [...document.querySelectorAll('[aria-label="Zeit"] button')];
      return buttons.length > 0 && buttons.every((button) => !button.disabled);
    }));
    if ((await interventions.getByRole("button", { name: "Erbkrankheiten anschalten" }).count()) > 0) {
      throw new Error("the switch of the hereditary diseases among the interventions");
    }
    await step("unfold the hereditary diseases", page.locator("summary", { hasText: /^Erbkrankheiten: ausgeschaltet$/ }).click());
    const on = page.getByRole("button", { name: "Erbkrankheiten anschalten" });
    await step("turn the hereditary diseases on", on.click());
    await step("its line", page.getByText("Die Erbkrankheiten sind von nun an angeschaltet.").waitFor());
    await step("the fold's new state", page.locator("summary", { hasText: /^Erbkrankheiten: angeschaltet$/ }).waitFor());
    await step("the switch's new label", page.getByRole("button", { name: "Erbkrankheiten ausschalten" }).waitFor());
    if ((await page.getByRole("alert").count()) > 0) {
      throw new Error(`a refusal: ${await page.getByRole("alert").first().textContent()}`);
    }
    await step("open the people", page.getByRole("link", { name: "Personen suchen" }).click());
    const counted = page.getByRole("status").filter({ hasText: /^\d+ von \d+ Lebenden$/ });
    await step("the people counted", counted.waitFor());
    const all = await counted.textContent();
    await step("filter for blond hair", page.getByRole("group", { name: "Haar" }).getByLabel("blond").check());
    await step(
      "the filter counts anew",
      page.waitForFunction((before) => {
        const shown = [...document.querySelectorAll("[role=status]")].map((e) => e.textContent);
        const now = shown.find((text) => / von \d+ Lebenden$/.test(text ?? ""));
        return now !== undefined && now !== before;
      }, all),
    );
    await step("back to the run", page.goBack());
    await step("open sim 13", page.locator(`a[href="#/runs/${run}/persons/13"]`).first().click());
    await step("its card folds its hereditary diseases", page.locator("details > summary", { hasText: /^Erbkrankheiten$/ }).waitFor());
    await step("its family folded", page.locator("details > summary", { hasText: /^Ähnlichkeit in der Familie$/ }).click());
    await step("its family at one age", page.getByRole("heading", { level: 3, name: /^Eltern und Vollgeschwister, mit \d+$/ }).waitFor());
    await step(
      "its mother among them",
      page.locator("figcaption", { hasText: /^Mutter / }).locator(`a[href="#/runs/${run}/persons/8"]`).waitFor(),
    );
    const mother = page.locator('svg[aria-label="Stammbaum"] a[data-sim="8"]');
    await step("its mother in the pedigree", mother.waitFor());
    await step("what she is to it", mother.locator("text", { hasText: /^Mutter · 50,0 %$/ }).waitFor());
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
    await step("unfold the fork's diseases", page.locator("summary", { hasText: /^Erbkrankheiten in der Gabel$/ }).click());
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
    await step(
      "where the fork begins",
      page.getByText("Eine Gabel, neu gerechnet ab dem Anfang von Jahr 29.").waitFor(),
    );
    const days = page.getByRole("group", { name: "Zeit" }).getByRole("button", { name: "28 Tage", exact: true });
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
    // The kinship found by name (Grok's round 46): a name of the list typed in, the core's line.
    const kinField = page.getByPlaceholder("Name eingeben");
    await step("the kinship's search field", kinField.waitFor());
    const named = await page.locator('datalist[id^="kin-"] option').first().getAttribute("value");
    if (named === null) {
      throw new Error("the kinship's search offers no name");
    }
    await step("a name typed", kinField.fill(named));
    await step("what that person is", page.getByRole("status").filter({ hasText: / Prozent$/ }).waitFor());
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
      // A wide family scrolls sideways in its own box.
      const tree = document.querySelector('svg[aria-label="Stammbaum"]')?.parentElement;
      return (
        pinned >= 0 &&
        pinned < headings.indexOf("Chronik") &&
        scrolls('ol[aria-label="Chronik"]') &&
        scrolls('ul[aria-label="Lebende"]') &&
        tree !== undefined &&
        tree !== null &&
        getComputedStyle(tree).overflowX === "auto"
      );
    });
    if (!short) {
      throw new Error("the run page: the pinned family not above a chronicle and living that scroll, or not in a box that scrolls sideways");
    }
    await step("the sheet of portraits", page.goto(`${url}#/portraits`));
    await step("its heading", page.getByRole("heading", { level: 1, name: "Bogen der Passbilder" }).waitFor());
    await step("its last portrait", page.getByRole("img", { name: "Helle Iris, männlich, 60 Jahre" }).waitFor());
    const portraits = await page.locator("table svg[role=img]").count();
    if (portraits !== 70) {
      throw new Error(`the sheet: ${portraits} portraits, not ten rows of seven`);
    }
    // Under each portrait its height in whole centimetres, seen and beneath it (K10 of the
    // third playtest).
    const heights = await page.locator("table td").evaluateAll((cells) =>
      cells.map((cell) => {
        const portrait = cell.querySelector("svg[role=img]");
        const caption = cell.lastElementChild;
        return (
          portrait !== null &&
          caption !== null &&
          !caption.contains(portrait) &&
          caption.checkVisibility() &&
          /^[0-9]+ cm$/.test(caption.textContent ?? "") &&
          caption.getBoundingClientRect().top >= portrait.getBoundingClientRect().bottom - 1
        );
      }),
    );
    if (heights.length !== 70 || heights.includes(false)) {
      throw new Error("the sheet: not each portrait with its height in whole centimetres beneath");
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
    // The cheat mode (ADR-0023, addendum of 2026-10-31): only when it is on does the page of a
    // person with a partner offer to part them, and it asks first. Parted elsewhere meanwhile
    // (another tab), the page's parting is refused, and the reason stays when the reload takes
    // partner and button away (review). Paired again, the page parts them: partner and button
    // go, and the person's chronicle tells of it.
    await step("the paired person", page.goto(`${url}#/runs/neu/persons/${x}`));
    const partnerLine = page.getByText(female ? "Partner:" : "Partnerin:");
    await step("with a partner", partnerLine.waitFor());
    const part = page.getByRole("button", { name: "Paar trennen (Cheat)" });
    if ((await part.count()) !== 0) {
      throw new Error("the cheat mode is off, yet the page offers to part the couple");
    }
    const cheat = page.getByRole("checkbox", { name: "Cheat-Modus" });
    const yes = page.getByRole("button", { name: "Ja, trennen" });
    await step("the cheat mode on", cheat.check());
    await step("part them", part.click());
    await step("asked first", page.getByText("Wirklich trennen?").waitFor());
    await step("not yet", page.getByRole("button", { name: "Abbrechen" }).click());
    await step("part them again", part.click());
    const couple = female ? { woman: x, man: y } : { woman: y, man: x };
    if ((await api("POST", "/api/runs/neu/input", { separate_partnership: couple })).format !== "simagent.view.input") {
      throw new Error("the parting elsewhere was refused");
    }
    await step("yes, too late", yes.click());
    const notPartners = page.getByText("Die beiden sind kein Paar.");
    await step("the refusal", notPartners.waitFor());
    await step("the partner gone after the refusal", partnerLine.waitFor({ state: "detached" }));
    if (!(await notPartners.isVisible())) {
      throw new Error("the reload took the refusal away");
    }
    if ((await api("POST", "/api/runs/neu/input", { arrange_partnership: couple })).format !== "simagent.view.input") {
      throw new Error("the second pairing elsewhere was refused");
    }
    await step("the page again", page.reload());
    await step("with a partner again", partnerLine.waitFor());
    await step("part them at last", part.click());
    await step("yes", yes.click());
    await step("the partner gone", partnerLine.waitFor({ state: "detached" }));
    await step("the button gone", part.waitFor({ state: "detached" }));
    await step("the parting told", page.getByText("trennen sich").first().waitFor());
    await step("the cheat mode off", cheat.uncheck());
    // The server falls silent (its answers cut off in the browser): first the chronicle and the
    // world after a step, then a step itself; once it answers again, "Erneut laden" ends the
    // message that it does not answer (the third playtest). No read of the world answers in
    // between, so none but the one asked again can end it.
    const world = /\/api\/runs\/neu\/world$/;
    const opened = page.waitForResponse(world);
    await step("the new town again", page.goto(`${url}#/runs/neu`));
    await step("its first second again", dated(0));
    await step("its world read", opened);
    silenced = true;
    await page.route(/\/api\/runs\/neu\/events\?/, (route) => route.abort());
    await page.route(world, (route) => route.abort());
    const step28 = page.getByRole("group", { name: "Zeit" }).getByRole("button", { name: "28 Tage", exact: true });
    await step("28 days without the chronicle", step28.click());
    const again = page.getByRole("button", { name: "Erneut laden" });
    await step("asked to load again", again.waitFor());
    await page.route(/\/api\/runs\/neu\/advance$/, (route) => route.abort());
    await step("a step without an answer", step28.click());
    const silence = page.getByText("Der Server antwortet nicht.");
    await step("the server's silence", silence.waitFor());
    await page.unrouteAll();
    await step("load again", again.click());
    await step("the silence over", silence.waitFor({ state: "detached" }));
    // Once more with only a step and the reads of the world cut off (the fourth playtest, K3):
    // nothing is to be loaded again, and once the server answers, the message ends by itself.
    await page.route(/\/api\/runs\/neu\/advance$/, (route) => route.abort());
    await page.route(world, (route) => route.abort());
    await step("a step without an answer again", step28.click());
    await step("the server's silence again", silence.waitFor());
    if (await again.isVisible()) {
      throw new Error("the silence of a step alone asks to load again");
    }
    await page.unrouteAll();
    await step("the silence over by itself", silence.waitFor({ state: "detached" }));
    if (errors.length > 0) {
      throw new Error(`errors on the page: ${errors.join("; ")}`);
    }
    console.log("smoke: the run page in year 28 and 29 with its empty chronicle's hint, the step it plays, a hunger and the folded switch, the people filtered, sim 13 beside its parents in the folded family and with its card, its mother marked in the tree, then its mother sim 8, the comparison with itself, the chronicle kept, the comparison with a fork, where the fork begins, a pinned family above a chronicle that scrolls, the sheet of portraits with their heights, a new town in its first second with a refused pairing and a couple parted in the cheat mode, and the server's silence ended by loading again and by itself");
  } catch (error) {
    fail(error.message);
  } finally {
    await browser.close();
  }
}
