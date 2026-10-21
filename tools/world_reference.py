#!/usr/bin/env python3
"""A model of the world of sim-core, written from the ADRs and independent of the Rust
code: the event queue (ADR-0002), the log entries (ADR-0004), the canonical projection
(ADR-0003 5.2), the save (ADR-0006 section 2), death through hazard budgets (ADR-0001)
by senescence and accident from the data (genetics specification 7.1, 7.2, 11.4;
ADR-0017 section 2) and the hereditary diseases of the data (genetics specification 3.1
to 3.4, 7.1; ADR-0017 sections 2, 3 and 5). The goldens and the reference bytes of the
tests come from it.

What the model knows:

- A world runs on its data (ADR-0007): the life course data, the calibration and the
  genome map as documents, by default those under data/ with the pool data/pool/pool.bin
  and its frequencies data/pool/pool_frequencies.json (`REPOSITORY`, the CLI's data since
  slice S5 of the switch to map 4: map 4 and the pool, design note of the switch, section
  14). The map 3 fixture tests/golden/map3 with the repository's life course data
  (`MAP3`, design note of the switch, section 4) is the data of the core tests whose
  subject is a rule, not the repository's map. Test data
  (`death_test_data`, on `MAP3`) differ from them only in the rates of senescence and accident, the
  list `diseases` and, for a scenario that gave a fertility in `Start` before, the
  fertility in the form `window` (design note of the fertility in the world, section 8:
  the rate, the window, the months of pregnancy, no pause, the wish always 40, no
  twins; no separation, design note of the separation, section 7; and the market off,
  `c` = 0, design note of the market, section 7), and for the acceptance scenario the
  market off and no separation (ADR-0017 section 5); `life_test_data` changes values of
  the life course data only (the scenario "market" on `MAP3`; "similar" on the fixture
  tests/golden/data_pairing); the save names the hash of the data a world runs on. The
  map lengths, the sex ratio, the map in the form of the meiosis reference and the
  mutation rates come from the documents too.
- `Start` (world seed, founder tag, world parameters, founders, couples, capacity; no
  causes of death and no fertility any more, ADR-0017 sections 2 and 4) writes
  `core:started`, plans the year end series (insertion number 1, first at the last second
  of year 0, period one year), the series of the market rounds (insertion number 2, first
  at MONTH, period MONTH; design note of the market, section 2), with a capacity the
  emigration series (insertion number 3, below) and then per founder, in list order: the
  creation entry, the first birthday (the first anniversary at or after the start) and the
  deaths in the order of its causes. A couple with a founder under 16 at
  the start (`0 − born < 16 · YEAR`) rejects `Start` with `partner_too_young`
  (`start_rejection`).
- Events run in the order of their key: time, phase (0 domain, 1 observer), micro step,
  class (birth 200, general 500, death 900, observer 1000), subject (kind: world 0, sim 1,
  partnership 2; ID), kind (year end 1, birthday 2, death 3, attempt 4, birth 5,
  separation 6, market round 7), insertion number. In one second: births, the round
  (500, the world), the attempts (500, sims), separations (500, partnerships), deaths.
- A birthday writes `core:birthday` and plans the next one a year later. The year end
  observer sees the clock one second before its own second, with its next occurrence
  already in the queue.
- Death (genetics specification 7.1, design note of the death from the data): the causes
  of every sim, in this order, are `core:senescence`, `core:accident`, then its lethal
  afflictions in data order. Per sim and cause a budget `B = exponential(DEATH_BUDGET,
  1.0)` at the key (tag DEATH_BUDGET, subject lineage key, occasion `str_key` of the
  code, detail 0). The origin is `o = max(born, start)` (a founder from the start, B19; a
  child from its birth), the origin age `x0 = as_years(o - born)`. Senescence (Gompertz
  `m · a · e^(b·x)`, `a` and `b` of the sex): the age at death
  `x = x0 + ln(1 + b · B / (a · m) · e^(−b·x0)) / b`, computed in that order as
  `mortality::senescence_age`, with `m` = 1 exactly without a trait of longevity, else
  `m = exp(−σ_g · ĝ)` (below, the pre-run). Accident (Makeham
  `λ` of the sex): `x = x0 + B / λ`. Both are due at `o + ceil((x − x0) · YEAR)`, never at
  100 000 years or more or beyond the clock (ADR-0002). exp and ln are the correctly
  rounded values (tools/kat_reference.py); each due second is checked not to hang on the
  last bit of exp, ln or the budget (and of the exp of `m`, `robust_seconds`), so the
  pinned libm of the core gives the same second.
- The traits of longevity and of the puberty timing (step 11, the pre-run on the template
  of map 4, design note of the pre-run tool; genetics specification 5.2, 7.2, 11.4;
  ADR-0012, ADR-0017 section 2): with a trait of longevity named (`longevity_trait`, a
  trait of the map without a phenotype row), `g_L` is its genetic value `G = A / σ_ref`,
  or `G = (A − m_ref) / σ_ref` with a `mean_ref` in the map (`genetic_value`, the map of
  the run; Grok GF-1, genetics specification 4.1), `ĝ` is `g_L` held on [−6, 6]
  (`LONGEVITY_INPUT_BOUND`; above 6 is 6, below −6 is −6) and `m = exp(−(σ_g · ĝ))` with
  the correctly rounded exp (`Data.multiplier`; `ĝ = 0` gives `exp(−0)` = 1 exactly).
  With a trait of the puberty timing named (`puberty_trait`, with a phenotype row whose
  child and adult weights are equal bit for bit, ADR-0012 section 3, asserted), `θ_i =
  θ + σ_θ · ẑ_p` (`theta_i`) with `z_p` the phenotype of that trait (`phenotype`, 4.2:
  its genome, its lineage key, its parents' keys or its own, its age at the reading; the
  weights do not depend on the age, so neither does `z_p`); the woman's menarche and both
  factors of an attempt read it (5.6). Without the traits nothing changes (`m` = 1, `z_p`
  = 0).
- Afflictions (genetics specification 3.1, 3.4; ADR-0017 section 3): once, at its
  creation (a founder at `Start`, a child at its birth), a sim gets one affliction per
  disease of the data list `diseases` (in data order) that it has, whatever the switch
  of the hereditary diseases. A disease with a locus carrier needs the genotype:
  `recessive`, every copy the sim has at the locus carries allele 1 and it has at least
  one copy (two autosomal copies; a woman's two X, a man's one X; a man's Y, none for a
  woman; one mt); `dominant`, at least one copy carries allele 1. With the genotype the
  penetrance of its sex decides, `bernoulli(p)` at the key (DISEASE_PENETRANCE = 13,
  lineage key, `str_key` of the code, 0); without it there is no draw. A threshold
  carrier without a liability trait afflicts nobody, without a draw. With a liability
  trait (design note of the threshold disease in the world, step 11; genetics
  specification 3.6, ADR-0019): the data checked it (`data_reference.check_liability`: a
  trait of the map whose data ID is the code, a phenotype row with equal child and adult
  weights, h² the disease's `heritability` and c² 0, bit for bit) and keep `T =
  probit(1 − K)` with the recorded bits of the core's AS241 (`data_reference.threshold`;
  for `K` = 0.01 `T` = 2.32634787404084, bits 40029c5c4630ff0c). `L` is the phenotype
  `z` of the trait (`phenotype`, genetics specification 4.2) at the sim's age at its
  creation (`0 − born` for a founder, 0 for a child; the weights do not depend on the
  age), with `E` at (ENV_INDIVIDUAL, lineage key, `str_key` of the trait = of the code, 0)
  and `C` (weight 0) at its parents' keys or its own; the sim has the disease iff `L > T`,
  strictly, without a penetrance draw. The onset in years is
  the fixed age, or `x = normal(mean, sd)` at the key (DISEASE_ONSET = 14, lineage key,
  `str_key` of the code, 0) held by `min(max(x, low), high)`. `check_afflictions` is the
  load check: every sim holds exactly these afflictions with these onsets.
- The measurement (step 11, design note of the measurement; ADR-0020 section 3):
  `additive_value` is `A_t`, the fold `genetic_value` divides by `sigma_ref` (the same bits
  as before), after subtracting the trait's `mean_ref` if the map has one (Grok GF-1);
  `measure_of` / `World.measure` take the living in ascending lineage key and
  give per trait of the map (map order) the mean and sigma of `A` (both sums from +0.0, the
  divisor `n`, `σ` the correctly rounded root) and per additive locus (map order) the mean
  dose (integer sum, one division); none without a living sim. `measurement_json` is the
  line of `simagent measure`. A read-only view: no draw, event, entry or state.
- A lethal affliction (effect `death` or `death_in_two_steps`) keeps an exposure: `used`,
  the hazard used up in the closed periods of the switch (0.0 at the creation), and
  `since`, when the current period began: the origin (the start for a founder, the
  birth for a child) if the switch is on at the creation, else none. A visible one
  (effect `visible`) has no exposure and never kills.
- The hazard of a disease is a step hazard (before, change_age, after); `death
  { per_year: h }` is (h, 0, h). It acts from the onset on. The age of a sim at `t` is
  `(t - born)` seconds as f64 over the seconds of a year (`as_years`). While its
  exposure runs (since `ts`), a lethal affliction kills at the age
  `a = disease_age(x0, max(B - used, 0), hazard)` with `x0 = max(age(ts), onset)`, the
  closed form of 3.1, at `ts + ceil((a - age(ts)) * YEAR)` (counted from the second
  the period began, so a used-up budget dies in the very second the switch goes on
  again); never if there is no such age, if `a - age(ts)` is 100 000 years or more, or
  if the time is beyond the clock (ADR-0002).
- The deaths of a sim are planned in the order of its causes: senescence, accident, then
  its lethal afflictions whose exposure runs, in data order; so in a tie of one second
  senescence comes before accident before the diseases (insertion order). The death
  writes `core:death` naming the cause code (`core:senescence`, `core:accident` or the
  disease) with the plan and the draw as causes, moves the sim with its afflictions as
  they are to the archive and cancels its other pending events.
- An override block is an input: stamped with the last closed second, acting in the
  next one, one entry `core:param_changed` per change. If the switch flips at `now`,
  every living sim (by ID) updates each lethal affliction (in data order): switched on,
  `since = now`; switched off, the period closes: with `from = max(age(since), onset)`
  and `to = age(now)`, `used` grows by `disease_hazard_used(hazard, from, to)` if
  `to > from`, and `since` is none. Then its pending deaths of a disease are cancelled
  and planned again in data order, by the entry `core:param_changed`. A change that sets the
  switch on then writes the onsets of path 3 (below, the onset of a disease). An override that
  keeps the switch as it is changes nothing but the log (an "on" while it is on finds every
  reached onset reported). The change
  `ParamChange::NutritionYear` (ADR-0013 section 2, `override_nutrition`) sets `N` of one
  year, inserted or replaced, the list kept ascending; only a year from the year of the
  second after the stamp on (`nutrition_year_past`), in the range of `Start`
  (`nutrition_out_of_range`). Only the body mapping reads the list, so it plans and cancels
  nothing: the entry `core:param_changed` and the `WorldParams` of the projection change.
- Couples: after the founders, `Start` forms a partnership per couple of its list (a
  woman and a man by their places among the founders); `ArrangePartnership` forms one
  between two living sims, unless it is rejected (`arrange_rejection`, in this order:
  `unknown_sim`, `sim_not_alive`, `partners_of_wrong_sex`, `already_partnered`, each for
  both, then `partner_too_young` (either under 16 in the second the input acts, the stamp
  plus one) and `partners_too_closely_related` (`φ ≥ 1/8`); a rejected input changes
  nothing and takes no sequence number). Each gets the next PartnershipId and one entry
  `core:partnered` (subjects the woman and the man; cause the input). A death ends the open partnership
  of the sim: it keeps the second of the death and the reason `Death`
  (`ended: Option<Ending { at, by }>`, design note of the separation, section 2).
- Separation (ADR-0018 section 3, design note of the separation): `λ` is
  `core:separation_rate` of the life course data (per year). A woman holds `partnerships`,
  the number of partnerships she has formed (0 for every founder and newborn, always 0
  for a man), right after `births`. When her partnership forms at `f`, `k` is her
  `partnerships`, which then grows by one; after her series (so its insertion number
  comes after the series') the event `separation { partnership }` (class general 500,
  subject the partnership, kind 6) is planned by the entry `core:partnered` at
  `f + max(from_years_ceil(B / λ), 1 s)` with `B = exponential(SEPARATION_BUDGET, 1)` at
  (SEPARATION_BUDGET = 18, her lineage key, occasion `k`, detail his lineage key); nothing
  if `λ` is 0, if `from_years_ceil` says never or the time leaves the clock. A separation
  at `t` ends the partnership (`Ending { at: t, by: Separation }`) and her series (a
  running pregnancy stays: the child is born with its father, and no series follows) and
  writes `core:separated { partnership }` (subjects the woman and the man, causes the plan
  and the draw). The death of either partner cancels the pending separation; one due in
  the second of a death comes first (class 500 before 900).
- The partner market (genetics specification 11.2, ADR-0018 section 1, design note of the
  market): the values of the life course data (`core:market_age` 16, `c`, `m`, the width
  `w` 7, `ρ`, `k` 12, `w_a`, `μ_a`, the limit 1/8, the odds of a cousin 10). A round at
  `t` (`n = t / MONTH`): the living men without an open partnership with `t − born ≥ 16
  · YEAR`, held for the round; every such woman searches iff `bernoulli(p)` at
  (MARKET_SEARCH = 19, her key, `n`, 0) with `p = s_f` (`ρ · s_f` while `t − born < 18 ·
  YEAR`), `s_f = min(1, c · exp(((−0.5) · z) · z))`, `z = (a − m) / w`, `a` her age in
  years (`search_chance`); the searchers in the order of `(rank(MARKET_ORDER = 20,
  WORLD, n, her key), her key)`; each in turn walks the men by `(rank(PAIRING_RANK = 5,
  her key, n, his key), his key)`, skipping `φ ≥ 1/8`, until `k` are kept; the utility
  `U = ((−(w_a · (d · d))) − (K · φ)) + G` with `d = Δa − μ_a`, `Δa` his age minus hers,
  `K = 16 · ln(odds)`, `G = gumbel` of (PAIRING_NOISE = 21, her key, `n`, his key); the
  largest `U` wins, a tie the earlier in the list; if he has no open partnership now the
  partnership forms at `t` as any pairing (entry `core:partnered` with the causes the
  plan of the round, the rule `core:partner_market` and the draw; her series from the
  next month start; the separation), else nothing. `φ` is the kinship of the genetics
  specification 2.7 over the pedigree of every sim, living or archived (the recursion of
  `spec_reference.Pedigree`), exact, counting in each query only the ancestors of the two
  persons up to their great-grandparents (below, "The depth of the kinship"). Each
  search draw keeps `DRAW_MARGIN` from its chance and the best utility `UTILITY_MARGIN`
  from every other, so the pinned libm of the core makes the same choices.
  `market=False` (a switch of this reference only) plans no round: the null twin.
- Similarity (genetics specification 11.2, design note of the similarity): the calibration
  (format 7) names `cognition_trait` and `height_trait`, each a trait of the map with
  phenotype weights or none. For a named trait the utility gets its term before the age
  term: with `dK = z_K,i − z_K,j` and `dH = z_h,i − z_h,j` the penalties in the order of
  11.2 are `w_K · (dK · dK)` (`core:pairing_weight_cognition`), `w_H · (dH · dH)`
  (`core:pairing_weight_height`), `w_a · (d · d)`, `K · φ`, and `U = (((−p1) − p2) − … −
  pn) + G` over the present terms left to right; a term whose trait is not named is left
  out, so without both traits `U` is the market's. `z` is the phenotype of the genetics
  specification 4.2 (ADR-0008 section 3, ADR-0009 sections 3 and 4; `phenotype_parts`) of
  the sim at the round, from its genome, its lineage key, its parents' keys (its own for
  both as a founder) and its age `t − born` in seconds; exp, ln and cos correctly rounded,
  sqrt exact. The woman's once per round, each candidate's once per candidate.
- Fertility (genetics specification 5.6 and 11.3, ADR-0017 section 4, design note of the
  fertility in the world): the data give the form of the curves (`puberty` or `window`),
  the adult rate, the pause after a birth and the months of pregnancy (whole months),
  the wish shape and its tilt. Per woman, pure and not held: her menarche
  `menarche(θ_i, δ, form)` with `δ` the draw FERTILITY_MENARCHE (15; her lineage key,
  occasion 0, detail 0; the form `puberty` only) and her wish `wish(u, shape, tilt)` with
  `u` the draw FERTILITY_WISH (16; the same key). `θ_i` of a sim is that of the curve of
  its sex with `z_p` = 0 without a trait of the puberty timing (the data of these tests
  name none), else with its `z_p` (above). The end of her fertility `E` is `female_decline_to` (puberty)
  or `until` (window). A sim holds `births`, the children it bore alive (0 for every
  founder, every newborn and every man), right after `conceptions`.
- The series of attempts (design note section 4): a woman has at most one monthly series
  (period MONTH). It is planned at `s` if and only if, then, she has an open
  partnership, is not pregnant, `births < wish` and `age(w, s) < E`: after a partnership
  forms at `f` (a couple of `Start` at 0, `ArrangePartnership` in the second after its
  stamp) at `s` = the first month start strictly after `f`, by the entry
  `core:partnered`; after a birth at `b` in an open partnership at `s` = the first month
  start at or after `b + max(pause · MONTH, 1 s)`, by the entry `core:birth`. It ends
  when she conceives, when the partnership ends, or after an attempt at `t` that fails
  when `age(w, t + MONTH) >= E`.
- An attempt at `t` (design note section 5): with `m` her partner in the open
  partnership, `p = chance(adult_rate, female_factor(age(w, t), menarche(w)),
  male_factor(age(m, t), θ_i(m)))`, drawn `bernoulli(p)` at (CONCEPTION_ATTEMPT, her
  lineage key, the month index of `t`, his lineage key); a chance of 0 draws like any
  other and fails. Each draw is checked not to lie within 1e-12 of its chance (and each
  wish draw not within 1e-12 of a threshold), so the pinned libm of the core gives the
  same outcome. On success she conceives her conception number n.
- Twins (genetics specification 11.3, ADR-0017 section 4, design note of the twins): at
  every conception `u = unit(draw)` at (MULTIPLE_BIRTH = 17, the lineage key of the first
  child `child(her, his, n, 0)`, occasion 0, detail 0); with `p_id` =
  `core:twins_identical` and `p_fr` = `core:twins_fraternal` of the life course data,
  identical twins if `u < p_id`, fraternal twins if `u < p_id + p_fr` (the sum as
  written), else one child; no triplets. (A unit draw against a data value needs no
  margin: no libm enters.) Child `i` (0, and 1 for twins) has the key `child(her, his, n,
  i)`. A zygote under a genome key gives, per parent, the start strands
  `uniform_below(MEIOSIS_START_STRAND, 2)` (subject the genome key, occasion 0 mother and 1
  father) and the sex `bernoulli(SEX_CHROMOSOME, p_Y)` (occasion 1, true for a son;
  `p_Y = r / (1 + r)` from `core:sex_ratio_at_birth`, genetics specification 2.3 and
  11.3). One child: one zygote under its own key. Identical twins: one zygote under the
  key of child 0, the same genome and sex for both. Fraternal twins: one zygote per child
  under its own key.
- The entries of a conception, all with the same subjects (her, him): per child in index
  order `core:conception` (its key; causes the plan of the attempt, the attempt draw, the
  draw of MULTIPLE_BIRTH, in this order, the same for both twins); after the entry of a
  child with its own zygote (child 0; child 1 of fraternal twins) the two `core:meiosis`
  of that zygote, her side then his (causes the event of that conception entry and the
  rule `core:meiosis`; payload the genome key of the zygote). Identical twins share the
  two meioses of child 0. The series ends; the pregnancy is state (father, conception
  number, the children as (key, sex, genome) in index order, conceived), and one birth is
  planned `pregnancy_months` months later (class birth 200, by the conception entry of
  child 0). The death of either ends the series; her death ends the pregnancy and forgets
  all its keys.
- Birth: per child in index order a new sim with the next SimId and its creation entry
  `core:birth` (subjects child, mother, father; the plan and the rule `core:gestation`),
  the entries one after the other; each born now, with parents and the conception
  number, `births` 0, `partnerships` 0, its afflictions from its genome and its own key
  with the birth as origin. Then per child in index order its first birthday a year
  later and its deaths from its birth on, planned by its own creation entry. The mother's `births` grows by
  the number of children; then, if she is in an open partnership, her series after a
  birth, planned by the creation entry of child 0. The wish is checked only when a
  series is planned, so twins of the last birth may exceed it by one.

- Households, part 1 (ADR-0018 section 4, design note of the households): the life course
  data (format 2) hold `leave_home_ages`, ten weights for the ages 16 to 25; a person's age
  of leaving is `16 + n` for the smallest `n` with `u < (w_0 + … + w_n) / W` (`u` = unit of
  LEAVE_HOME = 22 at its lineage key, occasion 0, detail 0; pure, not held). A household
  is [formed, ended] under the next HouseholdId; a sim holds its household, `moved_out`
  and its carer (an archived one its last household and `moved_out`). Dependent: living,
  under 18 and not moved out; fit to care: living and not dependent. `Start`, after the
  founders and the couples, writes one `core:household_formed` per household (causes the
  input and the rule `core:household`): the couples in list order (moved out), the
  founders of 18 or more in founder order (moved out), then the others in founder order,
  each alone: moved out if its drawn second of leaving is not after the start, else by
  the orphan rule (not moved out, no carer) with its `leave_home` (kind 8, class general,
  subject the sim, queue payload variant 7) planned by its creation entry. A birth moves
  each child into its mother's household right after its `core:birth` (`core:moved`,
  causes the plan of the birth and the rule; carer the mother) and plans its `leave_home`
  after its deaths. A later pairing founds a household (she moves, then he, then each
  dependent child either cares for, by SimId; both moved out, `leave_home` cancelled, no
  carer); a separation one for the man and the dependent children he cares for (the
  others stay, the woman becomes the carer of any cared for by neither); `leave_home` one
  for the person alone (moved out, no carer). These entries name the entry that caused
  them (`core:partnered`, `core:separated`) or the plan of the event, and the rule. A
  household ends in the second its last member leaves it. The 18th birthday ends the
  carer. A death leaves the household, then each dependent child the dead cared for gets
  the other biological parent, else the partner of the partnership the death ended (each
  if he lives in the child's household and is fit to care), else the orphan rule of part
  1: those children move together into one new household, the eldest first (tie: the
  smaller lineage key), no carer (entries caused by the `core:death`). `check_households`
  holds the load checks.
- Households, part 2 (ADR-0018 section 4 "Waisen", "Pflege", section 7; design note of the
  households, part 2): a sim and an archived sim hold `social_parents` after `parents` (the
  people who became its social parents, in that order, each once, never removed;
  projection 17, core 33). The orphan rule (`_place`) decides once per group (the dependent
  children of one household in one handling, the eldest first by born and lineage key; the
  first is the representative): the living person of 18 or more with the greatest `φ` to
  the representative (exact, `φ > 0`; a tie the smaller lineage key) becomes the carer of
  each child and its social parent unless its biological parent; else the woman of 18 or
  more in an open partnership whom FOSTER_RANK (23; `k_smallest`, `k` = 1, subject the
  representative's lineage key, occasion `floor(t / MONTH)`, the women's lineage keys)
  ranks first becomes the carer, she and then her partner social parents (each unless a
  biological parent); the children move into the carer's household (`core:moved` each,
  eldest first) unless they live there; else part 1's household of their own. Candidates
  are looked up after what the trigger changed. It runs at `Start` for each minor founder
  not past his age of leaving (a group of one, in founder order among the minors; a fostered
  founder has one `core:moved` and no `core:household_formed`; his own household at the
  start has no `core:moved`, as in part 1), at the death of a carer for the children
  without a successor (causes the `core:death` and the rule), and after a carer's
  `leave_home` for the children she cared for who get no other biological parent in their
  household (causes the plan of the `leave_home` and the rule). A step-parent who takes over
  by succession, the woman who takes over at a separation and the other parent get no
  social parent edge. Carer changes and new social parents write no entry.
- The pedigree (design note of the pedigree with kinship and social parents, step 10): a
  read-only view of the state (`World.pedigree_view`, the text of `pedigree.json` by
  `World.pedigree_json` as serde_json writes it, `json_text`, `json_number`): format
  version 2, the persons with their social parents, household, `moved_out`, carer and
  diseases (`affected` by the age at the reference second and the switch history of the
  input log, `World.switch_at`), the partnerships with `ended_by` and their `φ`, the
  households. The kinship query `World.kinship_of(a, b)`: `φ` as f64, None for an unknown
  SimId. Nothing of a run changes (no draw, entry, state or byte).
- Emigration by soft capacity (design note of the emigration, step 11; ADR-0020 section 6):
  `Start` holds `capacity: Option<u64>` after the couples (core 34; the projection stays 17,
  so a run without a capacity keeps its year hashes, its log and its state hash, and only
  its saves change: the header and one byte of `Start`). A capacity of 0 rejects `Start`
  (`capacity_zero`, 23). With a capacity `K`, `Start` plans one periodic event `Emigration
  { capacity: K }` (queue payload variant 8, kind 9, class 950 between the deaths and the
  observer, subject the world) right after the market round series (insertion number 3),
  first at `YEAR − 1`, period YEAR, planned by `core:started`; without one nothing. It runs
  in the last second of every year after the births and deaths of that second: with `P` the
  number of living sims, nothing if `P ≤ K`; else `e = P / K − 1` (f64, held at 0.5) and
  every household that has not ended and has a member of 18 or more (`born + 18 · YEAR ≤
  t`) draws `bernoulli(e)` at (EMIGRATION = 24, the smallest lineage key of its members of
  18 or more, the year `t / YEAR`, 0); in ascending HouseholdId each household that draws
  yes leaves whole: per member by SimId one entry `core:emigrated { household }` (log
  payload 12; causes the plan, the rule `core:capacity`, the draw), the member goes to the
  archive with `LeftBy::Emigration` (variant 1, no fields) at `t`, every event planned for
  it goes, an open partnership ends with `EndedBy::Emigration` (2) and loses its
  separation, a pregnancy goes with the mother; the household ends at `t`. Nobody else
  changes. The pedigree has format 3 (`emigrated` after `cause_of_death`, `ended_by`
  "emigration"). `check_emigration` holds the load checks.
- The depth of the kinship (Grok, H2; ADR-0020 section 4; genetics specification 2.7 and
  11.2; design note of the kinship depth): `World.kinship(x, y)` is
  `spec_reference.Pedigree.query`: the set `S` of `x`, `y` and every ancestor of either up
  to depth 3 (the great-grandparents), each once; within the query a person's mother is
  known iff she is in `S`, its father iff he is in `S`; an unknown parent's term is 0, `F`
  of a person is 0 unless both parents are known, and the recursion keeps the `S` of the
  pair all the way down (no intermediate value is shared between queries). Every read of
  `φ` goes through it: the market's exclusion and penalty, the orphan rule, the rejection
  of `ArrangePartnership`, `_partner`, `check_market`, the pedigree's `kinship` and the
  kinship query. The cache `kinships` holds only final values of pairs (a pair's `S` never
  changes). `World.kinship_query` returns the value with `S` and the pairs the recursion
  visited; with `depth=None` it counts every known ancestor (the rule before H2, only to
  show what the depth changes).
- The founders' draw (Grok F1, P2 (a), P3 (a); design note of the switch to map 4, sections
  2 and 12, slice S2b; core 39): a founder of `Start` keeps every locus it names (both
  copies, as named; naming is per locus, `[0, 0]` included); every other locus is drawn,
  one draw per founder, locus and copy, loci independent: `u = unit` of (the world seed,
  FOUNDER_ALLELE = 28, the founder's lineage key, `str_key` of the locus, the copy as a
  gamete: autosome 0 and 1, X 0 and 1 for a woman and 0 for a man, Y 1 for a man, mt 0),
  the allele 1 iff `u < q`; `q = d_ref / 2` at an additive locus, the data's frequency at
  the seven loci of the pool on data with frequencies, and no draw (the reference allele)
  at every other locus and at the seven loci on data without frequencies
  (`founder_alleles_drawn`). On map 3 (no additive locus, no frequencies) nothing is drawn.
  `fixed_alleles` names every locus of a map (the reference where the caller names none).
- Pool persons in the town (step 11; design note of the arrival, version 2; ADR-0020
  sections 4 and 5; tools/arrival_reference.py holds the scenarios and NOTES of it):
  - The data may hold a pool (`Data(..., pool=bytes)`): the bytes of a pool file read with
    `pool_reference.read_pool` (all the reader's rules), then the data checks
    `pool.carer_not_member`, `pool.sex` (members and unborn children: the paternal sex
    chromosome of the genome against the sex), `pool.locus` (one of the seven loci missing
    from the map), `pool.frozen_hash` (the pool's `frozen_hash` is not the data hash of
    these documents before the pool is appended; design note of the pool's records, section
    6), then the frequencies the data carry with the pool (`Data(..., pool=bytes,
    frequencies={locus: q})`, slice S2a of the switch to map 4): `pool.frequencies_loci`
    (exactly the seven loci), `pool.frequency_range` (`0 < q < 1`), in this order
    (`PoolDataRejected`). Without a pool the data hash stays and there are no frequencies;
    with one it is xxh3 of the postcard of the definitions followed by the postcard of
    `(pool_seed, pool_hash, frequencies)` (`pool_hash` = xxh3 of the file's bytes, the
    frequencies a sequence of (locus ID, q as f64) ascending by ID;
    `data_reference.pool_suffix`); `frozen_hash` stays the hash of the definitions alone.
  - `Start` holds `pool: Option<PoolStart>` after `capacity` (`PoolStart { pool_hash: u64,
    n_start: u64, immigration: bool, households: Vec<PoolHousehold> }`, each household
    exactly as the file holds it, `pool_reference.encode_household`). Rejected after the
    old checks (the couples, then `capacity_zero`), the first that applies:
    `start_pool_and_founders` (24), `start_pool_missing` (25), `start_pool_n_start` (26),
    `start_pool_households` (27). The draw (`pool_draw`): per household not yet used `rank
    = rank(world seed, POOL_RANK = 27, SubjectKey::WORLD, occasion, candidate)` with the
    candidate the smallest lineage key of a member of 18 or more at `E_p`; ascending by
    `(rank, candidate)`, taken until the members (unborn children do not count) reach or
    exceed the number; `Start` with occasion 0 and `n_start` (a pool too small gives all its
    households, decision of NOTES.md).
  - Immigration: with `immigration` and `q = (n_start + 50) div 100 > 0`, `Start` plans
    the periodic event `Immigration { persons: q }` (queue payload variant 9, kind 10,
    class 100, subject the world) right after its other series (after the emigration's),
    first at second 0, period YEAR, planned by `core:started`. At year `y` it draws the
    unused households with occasion `y`; if they cannot reach `q`, nobody arrives and the
    run stops with the invariant `pool.empty` (`Invariant`).
  - An arrival (`_arrive`), per household in rank order: (1) per member in its order a new
    SimId, `core:arrived { lineage_key }` (log payload 13; subjects the sim; causes the
    input or the plan of the event) and its state (`born = born_pool + Δ`, `Δ = A − E_p`;
    the pool's counts, `moved_out`, afflictions with the exposure from `A` if the switch is
    on; `parents` None, `social_parents` [], the carer the member with the pool's carer
    key; `origins[sim]` = PoolOrigin); (2) a new household, `core:household_formed`
    (subjects ascending, causes the input or the plan, and the rule), no `core:moved`; (3)
    the open partnership, `core:partnered` (subjects woman, man; cause her `core:arrived`)
    formed at `formed_pool + Δ`, nothing else; (4) the pregnancies in member order
    (`father` the SimId if he has arrived, `pool_father` his key); (5) per member, by its
    `core:arrived`: the carried conception series, birth, separation (the woman's), leaving
    home, each at `t_pool + Δ` (`pool.carried_before_arrival` below `A`), the next birthday
    and the deaths. What the Immigration schedules at its own second goes into the next
    micro step (ADR-0002: in a handler at `(t, p, m)` a time `t` of the same phase is micro
    `m + 1`; inputs and later seconds micro 0).
  - Draws (note section 6.2): `E` and `C` with the pool seed for a pool person (`C` at its
    PoolOrigin keys, the self-parent rule only if both are None), wish and menarche with
    the pool seed; a town person whose two biological parents are pool persons has `E`
    with the world seed and `C` with the pool seed at their keys; everyone else the world
    seed. Every other draw is the world seed's. Death budgets: detail the arrival year of
    an arrival (0 else), origin `A` for an arrival.
  - Kinship over lineage keys (`kinship_by_keys`), order `(0, born_pool)` for an arrival and
    a record of the pool, `(1, born)` for everyone born in the town; the orphan succession
    finds the other parent of a pool person by its PoolOrigin keys in its household.
  - A pool child (born of a pool pregnancy): `core:birth` without the father if he has no
    SimId, `parents` None, PoolOrigin (genome key of the pool, born_pool None, arrived the
    birth, arrival year the calendar year of the birth, its mother's key, `pool_father`).
  - State and versions: `origins` (Sim and ArchivedSim `pool: Option<PoolOrigin>`, the
    last field of each), `Pregnancy { father: Option<SimId>, pool_father:
    Option<LineageKey>, conception, children, conceived }`, `pool_used: Vec<u32>` (indices
    into the pool's households, arrival order) the last field of the projection; projection
    18, core 37, save format 2, pedigree format 4 (`pool` per person, last).
- The village start mode (step 11, P4 village; ADR-0020 section 8 "Das Dorf, genau (G4)";
  design note of the village, sections 5 and 6, Grok V1 (a) and V2 (a); tools/
  village_reference.py prints its values): `PoolStart` holds `village: bool` after
  `immigration` (postcard a bool byte; the scenario file `"pool": {"n_start": 80,
  "immigration": false, "village": true}`); with `village` false nothing changes. With
  `village` true the draw is of persons (`village_draw`): the candidates of a sex are the
  pool's members (not its unborn children) with `18 · YEAR <= E_p − born < 41 · YEAR`; per
  sex the `n_start / 2` smallest `(rank, lineage key)` with `rank = rank(world seed, POOL_RANK
  = 27, SubjectKey::WORLD, occasion 1, the lineage key)` (ADR-0001 section 5). The households
  of `PoolStart` are one per chosen person (`village_households`), the women in rank order,
  then the men in rank order (the note leaves the order open; decision of this reference):
  members `[person]`, no partnership, no pregnancies, the person as the file holds it with
  `conception_at`, `birth_at`, `separation_at`, `leave_home_at` and `carer` None and
  `moved_out` true (`village_person`; genome, birth, keys, counts and afflictions unchanged). Rejected after
  `start_pool_n_start`, the first that applies: `n_start` odd (`start_pool_village_n_start`,
  28), immigration on (`start_pool_village_immigration`, 29), a sex with fewer candidates
  than `n_start / 2` (`start_pool_village_candidates`, 30); then `households` not those
  (`start_pool_households`). The arrival is `_arrive` of these households at second 0 (step
  5 has no carried plan to plan); `pool_used` stays empty (no household of the file arrives;
  decision of the coordinator, 2026-10-19); `pool_member` of a chosen person is its record
  as it arrived. `village` is not a field of the CLI's scenario file (only xtask builds a
  village `Start`); a resolved scenario's pool has `village` false. Core 41 (the new byte changes every save of a pool start; year
  and log hashes of a town stay).
- The onset of a disease (ADR-0017 section 3, addendum of 2026-10-20, paths 1 to 3; Grok P1
  to P3 and Q1): per affliction `B = born + ceil(onset · YEAR)` (`onset_second`: the product as
  an f64, its ceiling), `E` the creation second (`creation_second`: a founder 0, an arrival
  its arrival second, a child born in the town, a pool child too, its birth). The creating
  step (`_start`, `_birth`, `_arrive`) runs `_onset_step` right after the sim's creation
  entry (`core:founder_added`, `core:birth`, `core:arrived`), before anything else of it and
  before the next sim's creation entry: per affliction in data order, `B <= E` writes
  `core:disease_onset { disease, since_year, age_years }` (log payload 14: the code as a
  string, `B div YEAR`, `floor(onset)`; subjects the sim; cause the creation entry) if the
  switch in effect then is on (path 1); `B > E` plans `DiseaseOnset { sim, disease }` (queue
  payload 10: the SimId and the `str_key` of the code; kind 11, class general, subject the
  sim) at `B` whatever the switch, by the creation entry (path 2). The event writes the entry
  with its plan as the cause if the switch is on, else nothing. Death and emigration remove it
  with the sim's other plans. Path 3 (`_fork_onsets`, core 43): an override block whose change
  sets the switch on, acting in `T`, writes in its input context, after its
  `core:param_changed` and the deaths it plans again, for every sim of the active population
  by SimId and each affliction in data order with `B <= T` and no own open event (open: `B > E`
  and `B >= T`; an onset exactly in `T` with `B > E` is left to its event, which runs after the
  input) the same entry, caused by the input, if none of its earlier reporting moments had the
  switch on (`onset_reported`: the creation if `B <= E` with the switch it saw, `Start`'s own
  value for what `Start` made, else the switch at `E` after the inputs of `E`; the own event at
  `B` if `B > E` with the switch at `B` after the inputs of `B`; every earlier override that set
  the switch on after `E` (in `E` for what `Start` made) if `B <= E`, after `B` if `B > E`,
  inputs of one second in their sequence). It reads only `B`, `E`, the active population and
  the switch history of the input log; no state, no event. `check_onsets` is the load check:
  exactly one open event at `B` per living sim and affliction with `B > E` and `B` after the
  clock, none else; `check_carried_plans` (C4) counts the path-1 entries between the
  `core:arrived` entries of a household.

Usage: .venv/bin/python tools/world_reference.py (prints the data hashes, the checks of
the step hazard and pure values of the market; the other tools use the model)
"""

import copy
import itertools
import math
from fractions import Fraction
from pathlib import Path

import mpmath
import xxhash

import data_reference

from postcard_reference import (
    FEMALE,
    MALE,
    MATERNAL,
    PATERNAL,
    X,
    Y_CHROMOSOME,
    afflictions as afflictions_bytes,
    capacity as capacity_bytes,
    couple,
    genome_of,
    flag,
    founder,
    household as household_bytes,
    lineage_key,
    param_change,
    param_change_nutrition,
    partnership,
    mutation,
    segment_meiosis,
    seq,
    signed,
    string,
    variant,
    varint,
    world_params,
)
from rng_reference import (
    bernoulli,
    chacha_key,
    child_key,
    exponential_reference,
    founder_key,
    gumbel_reference,
    k_smallest,
    normal_reference,
    rank,
    str_key,
    unit,
    words,
)

from kat_reference import cos_reference, exp_reference, ln_reference

import meiosis_reference

# The kinship of the genetics specification 2.7 (the recursion of tools/spec_reference.py
# over a pedigree, exact as fractions). That module sets the precision of mpmath to 40
# digits when it is imported; the correctly rounded exp and ln here need the 60 of
# tools/kat_reference.py, so the precision is put back (Pedigree reads no mpmath).
_PRECISION = mpmath.mp.dps
from spec_reference import KINSHIP_DEPTH, Pedigree  # noqa: E402

mpmath.mp.dps = _PRECISION
assert mpmath.mp.dps == 60

ROOT = Path(__file__).resolve().parent.parent


YEAR = 364 * 86_400
MONTH = 28 * 86_400
NEVER_YEARS = 100_000.0
# The range of the clock (ADR-0002): a time beyond it is never.
MAX_ABS_SECS = 1_000_000 * YEAR
# tests/golden/rng_tags.txt
MEIOSIS_START_STRAND, SEX_CHROMOSOME, CONCEPTION_ATTEMPT, DEATH_BUDGET = 1, 2, 3, 4
DISEASE_PENETRANCE, DISEASE_ONSET = 13, 14
# New with the fertility in the world (design note of the fertility in the world, section
# 1; genetics specification 11.7).
FERTILITY_MENARCHE, FERTILITY_WISH = 15, 16
# New with the twins (design note of the twins, section 1; genetics specification 11.7).
MULTIPLE_BIRTH = 17
# New with the separation (design note of the separation, section 1; ADR-0018 section 3).
SEPARATION_BUDGET = 18
# The partner market (design note of the market, section 1; genetics specification 11.7):
# PAIRING_RANK is old, the other three are new.
PAIRING_RANK = 5
MARKET_SEARCH, MARKET_ORDER, PAIRING_NOISE = 19, 20, 21
# SubjectKey::WORLD, the subject of the worldwide rank MARKET_ORDER (ADR-0018 section 1).
WORLD_KEY = 0
# The draws of the environment of a phenotype (genetics specification 4.2, ADR-0009 section
# 3): the shared one `C_t` and the individual one `E_t`.
ENV_SHARED, ENV_INDIVIDUAL = 10, 11
# The rule a pairing of the market names as its cause (design note of the market, section
# 3, step 6).
PARTNER_MARKET_RULE = "core:partner_market"
# The margin the best utility keeps from every other of the list, so that the last bits
# of `gumbel` (2^-45 against the reference, ADR-0018 section 6) and of `ln` cannot change
# the choice.
UTILITY_MARGIN = 1e-9

# The draw of the age of leaving home (design note of the households, section 1; ADR-0018
# section 4): subject the lineage key, occasion 0, detail 0, `u = unit`.
LEAVE_HOME = 22
# The rule every entry of the households names (design note of the households, section 3).
HOUSEHOLD_RULE = "core:household"
# The draw of the foster household (design note of the households, part 2, section 1;
# ADR-0018 section 4 "Pflege"): the rank of `k_smallest` with `k` = 1, subject the lineage
# key of the group's representative, occasion the absolute month index of the placement,
# candidates the lineage keys of the women.
FOSTER_RANK = 23
# The draw of a founder's allele in the pre-run (design note of the pre-run tool, section 2;
# ADR-0020 section 3): subject the founder's lineage key, occasion `str_key` of the locus,
# detail the copy (0 maternal, 1 paternal), `u = unit`, the allele 1 if `u < 0.5`
# (tools/prerun_reference.py draws them).
PRERUN_ALLELE = 25
# The draw of a founder's unnamed allele (Grok F1; design note of the switch to map 4,
# sections 2 and 12, slice S2b): a founder of `Start` keeps every locus it names; every other
# locus is drawn, one draw per founder, locus and copy: seed the world seed, subject the
# founder's lineage key, occasion `str_key` of the locus, detail the copy as a gamete (0
# maternal, 1 paternal), `u = unit`, the allele 1 iff `u < q` (`founder_alleles_drawn`). A
# tag of its own, since the pre-run draws at 0.5 and F1 at `d_ref / 2` (ADR-0001: one tag per
# place that draws).
FOUNDER_ALLELE = 28

# The projection of the households (design note of the households, section 2): the
# counter of HouseholdId after that of PartnershipId, the households after the
# partnerships, per sim its household, `moved_out` and its carer after `partnerships`, per
# archived sim its last household and `moved_out` (16); per sim and archived sim its social
# parents after `parents` (17, part 2, section 2). The core version goes up with the market
# (30), the similarity terms (31), the households (32: the life course data of format 2
# change every data hash, and the projection) and the households, part 2 (33).
# The emigration (design note of the emigration, section 1): the field `capacity` of
# `Start` changes every save (34); the projection stays 17 (the capacity is held only in the
# payload of the planned event, the input log is not projected).
# Pool persons in the town (design note of the arrival, sections 9 and 10): `pool` of Sim and
# ArchivedSim, `Pregnancy.father` an option with `pool_father` after it, the used households
# of the pool (projection 18); the core version and the save format rise with it (37, 2).
# The frequencies of the pool in the data hash (S2a) and the founders' draw (F1, S2b) change
# results without a new field: core 39, the projection stays 18 (design note of the switch
# to map 4, section 12: one bump for both slices).
# The repository's data become map 4 with the pool (S5, design note of the switch, section
# 14): every golden of the repository's data moves, core 40 (ADR-0003 6.4, also for data
# alone).
# The village start mode (design note of the village, section 6): `PoolStart.village` changes
# every save of a pool start (the save holds `Start`), core 41; the projection stays 18.
# The onset of a disease as an entry (ADR-0017 section 3, addendum of 2026-10-20, paths 1 and
# 2): a new event `DiseaseOnset` in the queue and a new log payload change logs, queues, saves
# and state hashes without a new field: core 42, the projection stays 18.
# Path 3 of the onset (the addendum, "Drei Wege" (3)): an override that sets the switch on
# writes the reached onsets nothing reported yet, a new rule without a new field: core 43, the
# projection stays 18.
PROJECTION_VERSION = 18
CORE_VERSION = 43

FORMAT_VERSION = 2
# The pedigree's format (design note of the arrival, section 8): `pool` per person.
PEDIGREE_FORMAT_VERSION = 4
# The pool in the town (design note of the arrival, sections 3 and 4): the purpose tag of
# the draw of the households (the next free tag after POOL_ALLELE, 26), the event
# `Immigration { persons }` (class 100 before births, kind 10, the queue payload after
# Emigration, 8), the log payload `core:arrived { lineage_key }` (after core:emigrated, 12),
# the reject codes of `Start` (after capacity_zero, 23) in the note's order.
POOL_RANK = 27
IMMIGRATION_CLASS = 100
IMMIGRATION_KIND = 10
IMMIGRATION_VARIANT = 9
ARRIVED_VARIANT = 13
START_POOL_AND_FOUNDERS, START_POOL_MISSING, START_POOL_N_START, START_POOL_HOUSEHOLDS = 24, 25, 26, 27
# The village start mode (design note of the village, section 6): its reject codes of `Start`
# (after start_pool_households, 27, in the order they are checked), the occasion of the draw
# of the persons, the age window at the freeze in whole years, `[18, 41)`.
START_POOL_VILLAGE_N_START, START_POOL_VILLAGE_IMMIGRATION, START_POOL_VILLAGE_CANDIDATES = 28, 29, 30
VILLAGE_OCCASION = 1
VILLAGE_AGES = (18, 41)
# The onset of a disease (ADR-0017 section 3, addendum of 2026-10-20; Grok P1 to P3 and Q1,
# protocol sections 21 and 22): the event `DiseaseOnset { sim, disease }` (class general 500,
# subject the sim, kind 11, the queue payload after Immigration, 9; `disease` the `str_key` of
# the disease's code, as the cause of `Death`) and the log payload `core:disease_onset {
# disease, since_year, age_years }` (after core:arrived, 13: the code as the other codes, then
# two i64).
DISEASE_ONSET_KIND = 11
DISEASE_ONSET_VARIANT = 10
DISEASE_ONSET_ENTRY = 14
# The largest micro step (ADR-0002).
MAX_MICRO_STEP = 64
# The lines the code lists of tests/golden (protected) need for the arrival; they are not
# there yet, so the check of the code lists below leaves these reject codes out
# (tools/arrival_reference.py prints every new line).
NEW_CODE_LINES_OF_THE_ARRIVAL = (START_POOL_AND_FOUNDERS, START_POOL_MISSING, START_POOL_N_START, START_POOL_HOUSEHOLDS)
# The same for the village's reject codes (tools/village_reference.py prints their lines).
NEW_CODE_LINES_OF_THE_VILLAGE = (START_POOL_VILLAGE_N_START, START_POOL_VILLAGE_IMMIGRATION, START_POOL_VILLAGE_CANDIDATES)
# The margin a draw keeps from what it is compared with (the chance of an attempt, a
# threshold of the wish), so that a last bit of libm in the core cannot flip it.
DRAW_MARGIN = 1e-12

NONE = b"\x00"

# Event classes and kinds (tests/golden/event_codes.txt).
BIRTH, GENERAL, DEATH, OBSERVER = 200, 500, 900, 1000
# The emigration (design note of the emigration, sections 2 and 5): the class after the
# deaths and before the observer, the kind, the queue payload `Emigration { capacity }` (the
# next variant after LeaveHome, 7), the log payload `core:emigrated { household }` (the next
# after Moved, 11), the tag of the draw, the rule of the entries, the reject code of a
# capacity of 0 (the next after PartnersTooCloselyRelated, 22).
EMIGRATION_CLASS = 950
EMIGRATION_KIND = 9
EMIGRATION_VARIANT = 8
EMIGRATED_VARIANT = 12
EMIGRATION = 24
CAPACITY_RULE = "core:capacity"
CAPACITY_ZERO = 23
# The held largest rate (design note section 2).
EMIGRATION_MAX_RATE = 0.5
YEAR_END_KIND, BIRTHDAY_KIND, DEATH_KIND, ATTEMPT_KIND, BIRTH_KIND = 1, 2, 3, 4, 5
# New with the separation (design note of the separation, section 3).
SEPARATION_KIND = 6
# New with the market (design note of the market, section 2): a periodic world event.
MARKET_ROUND_KIND = 7
# The queue payload of the round: the next variant after Separation (5), without fields.
MARKET_ROUND_PAYLOAD = variant(6)
# New with the households (design note of the households, section 3): `leave_home { sim }`,
# class general, subject the sim, kind 8, the next queue payload after MarketRound (6).
LEAVE_HOME_KIND = 8
LEAVE_HOME_VARIANT = 7
# The log payloads of the households, the next variants after Separated (9):
# `core:household_formed { household }` and `core:moved { household }`.
HOUSEHOLD_FORMED_VARIANT, MOVED_VARIANT = 10, 11
WORLD_SUBJECT, SIM_SUBJECT, PARTNERSHIP_SUBJECT = 0, 1, 2
# Reject codes of the inputs (tests/golden/log_codes.txt); the two of the market come at
# the end of the list (design note of the market, section 4).
UNKNOWN_FOUNDER, UNKNOWN_SIM, SIM_NOT_ALIVE, PARTNERS_OF_WRONG_SEX, ALREADY_PARTNERED = 13, 9, 10, 11, 12
PARTNER_TOO_YOUNG, PARTNERS_TOO_CLOSELY_RELATED = 21, 22
REJECT_NAMES = {
    START_POOL_AND_FOUNDERS: "start_pool_and_founders",
    START_POOL_MISSING: "start_pool_missing",
    START_POOL_N_START: "start_pool_n_start",
    START_POOL_HOUSEHOLDS: "start_pool_households",
    START_POOL_VILLAGE_N_START: "start_pool_village_n_start",
    START_POOL_VILLAGE_IMMIGRATION: "start_pool_village_immigration",
    START_POOL_VILLAGE_CANDIDATES: "start_pool_village_candidates",
    CAPACITY_ZERO: "capacity_zero",
    UNKNOWN_SIM: "unknown_sim",
    SIM_NOT_ALIVE: "sim_not_alive",
    PARTNERS_OF_WRONG_SEX: "partners_of_wrong_sex",
    ALREADY_PARTNERED: "already_partnered",
    UNKNOWN_FOUNDER: "unknown_founder",
    PARTNER_TOO_YOUNG: "partner_too_young",
    PARTNERS_TOO_CLOSELY_RELATED: "partners_too_closely_related",
}
# EndedBy (design note of the separation, section 2): the variant index after the time;
# Emigration appended (design note of the emigration, section 5).
ENDED_BY_DEATH, ENDED_BY_SEPARATION, ENDED_BY_EMIGRATION = 0, 1, 2
ENDED_BY_NAMES = ("death", "separation", "emigration")
# LeftBy of an archived sim (design note of the emigration, section 5): Death { cause } is
# variant 0, Emigration (no fields) variant 1. In the archive of this model the cause key is
# None for an emigrant.
LEFT_BY_DEATH, LEFT_BY_EMIGRATION = 0, 1
# The causes of death every sim has before its diseases, in this order (design note of
# the death from the data, section 2).
SENESCENCE, ACCIDENT = "core:senescence", "core:accident"


def some(data: bytes) -> bytes:
    return b"\x01" + data


def hash64(data: bytes) -> int:
    return xxhash.xxh3_64_intdigest(data, seed=0)


# ---------------------------------------------------------------------------------------
# Data


def number(entry) -> float:
    """A calibrated number of the data: its value as f64."""
    return float(entry["value"])


def only(enum) -> tuple:
    """(name, fields) of an externally tagged variant with fields."""
    ((name, fields),) = enum.items()
    return name, fields


class Disease:
    """One entry of the calibration's list `diseases` (ADR-0017 section 1), without its
    texts: the code, the carrier (a locus with the penetrance per sex; or a threshold with
    its trait of liability as `PhenotypeTrait` and its threshold `T`, or neither for a
    threshold without a liability trait), the onset and the step hazard (None for a
    visible disease). `calibration` and `genome_map` are the stripped documents (the row
    and the trait of a named liability)."""

    def __init__(self, entry: dict, loci: dict, calibration: dict, genome_map: dict):
        self.code = entry["code"]
        self.key = str_key(self.code)
        kind, carrier = only(entry["carrier"])
        self.liability, self.threshold = None, None
        if kind == "locus":
            self.locus = loci[carrier["locus"]]
            assert self.locus["mark"] == "disease", f"{self.code}: the locus is no disease locus"
            self.penetrance = {
                FEMALE: number(carrier["penetrance_female"]),
                MALE: number(carrier["penetrance_male"]),
            }
        else:
            assert kind == "threshold"
            self.locus = None
            # A named trait of liability (design note of the threshold disease, section 1;
            # `data_reference.check_liability` checked it when the documents were hashed):
            # its phenotype (trait, row) and `T = probit(1 − K)` with the recorded bits of
            # the core's AS241 (`data_reference.threshold`), kept with the data.
            if carrier["liability"] is not None:
                (row,) = [row for row in calibration["traits"] if row["trait"] == carrier["liability"]]
                self.liability = PhenotypeTrait(carrier["liability"], genome_map, row)
                assert self.liability.id == self.code and self.liability.weights[0] == self.liability.weights[2]
                assert self.liability.weights[1] == self.liability.weights[3] == 0.0
                self.threshold = data_reference.threshold(number(carrier["prevalence"]))
        kind, onset = only(entry["onset"])
        if kind == "fixed":
            self.onset = ("fixed", number(onset["age"]))
        else:
            assert kind == "normal"
            self.onset = ("normal", *(number(onset[name]) for name in ("mean", "sd", "low", "high")))
        effect = entry["effect"]
        if effect == "visible":
            self.hazard = None
        else:
            kind, numbers = only(effect)
            if kind == "death":
                h = number(numbers["per_year"])
                self.hazard = (h, 0.0, h)
            else:
                assert kind == "death_in_two_steps"
                self.hazard = tuple(number(numbers[name]) for name in ("before", "change_age", "after"))


class FertilityForm:
    """The calibration's `fertility` (ADR-0017 section 1; design note of the fertility
    curves, section 1) without its texts. `kind` "puberty": the twelve numbers of the
    genetics specification 5.6 as attributes of their names (`menarche_mean` to
    `male_decline_to`; years, `male_floor` a fraction). `kind` "window" (test data only,
    ADR-0017 section 5): `start` (the field `from`) and `until` in years."""

    def __init__(self, entry: dict):
        self.kind, fields = only(entry)
        names = data_reference.FERTILITY_FIELDS[data_reference.FERTILITY_FORMS.index(self.kind)]
        assert set(fields) == set(names), f"fertility {self.kind}: fields {sorted(fields)}"
        if self.kind == "puberty":
            for name in names:
                setattr(self, name, number(fields[name]))
        else:
            self.start, self.until = number(fields["from"]), number(fields["until"])


class TraitTerms:
    """A trait of the map as its genetic value reads it (genetics specification 4.1; ADR-0008
    sections 1 and 3): its data ID, `sigma_ref`, `mean_ref` (Grok GF-1: the f64 of the map's
    optional field, or None without it, `data_reference.mean_ref_of`) and the groups of the
    fold `A` (first the loci that load on it, then per included trait in the order of
    `includes` the loci that load on that one; each group in map order, each locus with its
    coefficient `λ · β`, or `(w · λ) · β` in an included group, and `d_ref`). Every trait of
    the map has these, with a phenotype row or without (the measurement, design note of the
    measurement, section 1, reads every trait)."""

    def __init__(self, trait_id: str, genome_map: dict):
        traits = {entry["id"]: entry for entry in genome_map["traits"]}
        entry = traits[trait_id]
        self.id = trait_id
        self.sigma_ref = float(entry["sigma_ref"])
        assert self.sigma_ref > 0.0
        self.mean_ref = data_reference.mean_ref_of(entry)

        def group(target: str, include_weight):
            loci = []
            for locus in genome_map["loci"]:
                action = locus["action"]
                if isinstance(action, str):
                    continue
                ((kind, weights),) = action.items()
                assert kind in ("additive", "threshold") and locus["chromosome"] == "autosome"
                for load in weights["loads"]:
                    if load["trait"] != target:
                        continue
                    lam, beta = float(load["weight"]), float(weights["effect"])
                    coefficient = lam * beta if include_weight is None else (include_weight * lam) * beta
                    loci.append((locus, coefficient, float(weights["d_ref"])))
            return loci

        self.groups = [group(trait_id, None)] + [
            group(include["trait"], float(include["weight"])) for include in entry["includes"]
        ]


class PhenotypeTrait(TraitTerms):
    """A trait of the map read as a phenotype (genetics specification 4.1, 4.2; ADR-0008
    sections 1 and 3, ADR-0009): its terms (`TraitTerms`), its `str_key` (the occasion of
    its environment draws) and its phenotype weights `(h²_child, c²_child, h²_adult,
    c²_adult)` (the calibration's row)."""

    def __init__(self, trait_id: str, genome_map: dict, row: dict):
        super().__init__(trait_id, genome_map)
        self.key = str_key(trait_id)
        self.weights = tuple(number(row[name]) for name in data_reference.WEIGHTS)


def dose(person: dict, locus: dict) -> float:
    """The dose `d` of the locus (genetics specification 2.1): the copies with allele 1, as
    f64."""
    return float(sum(copies_at(person, locus)))


def additive_value(person: dict, trait: TraitTerms) -> float:
    """`A_t` (ADR-0008 section 3; design note of the measurement, section 2: the sum
    `genetic_value` divides): folded from +0.0, group by group, each locus in map order
    adding `coefficient · (d − d_ref)`."""
    acc = 0.0
    for loci in trait.groups:
        for locus, coefficient, d_ref in loci:
            acc = acc + coefficient * (dose(person, locus) - d_ref)
    return acc


def genetic_value(person: dict, trait: TraitTerms) -> float:
    """`G_t = (A_t − m_t,ref) / σ_t,ref` (genetics specification 4.1, Grok GF-1; ADR-0008
    section 3): `additive_value` minus the trait's `mean_ref` (one subtraction), then divided
    once by `sigma_ref`. Without a `mean_ref` in the map it is `A_t / σ_t,ref` exactly as
    before (no subtraction at all, the same bits)."""
    value = additive_value(person, trait)
    if trait.mean_ref is not None:
        value = value - trait.mean_ref
    return value / trait.sigma_ref


def measure_of(persons: list, data) -> dict:
    """The measurement of the genic spread and the doses (design note of the measurement,
    section 1; ADR-0020 section 3) over `persons`, a list of (lineage key, genome) of the
    living in any order, or None for no person. The persons are taken in ascending lineage
    key (keys are distinct). `n` the number of persons, as f64 exactly. Per trait of the
    map (map order, `data.traits`): `A_i = additive_value`, `m = (Σ A_i) / n` and `v = (Σ
    (A_i − m)²) / n` (the square one product `x · x`), both sums folded from +0.0 in that
    order, `σ = sqrt(v)` (correctly rounded, as `sim_math::sqrt`). Per locus of
    `data.measured_loci` (decision 1 of the notes: the loci whose action is `additive`, in
    map order): `(Σ d_i) / n` with the doses summed as integers and one division in f64.
    Returns {"living": n, "traits": [(trait, mean, sigma)], "loci": [(locus, mean_dose)]}."""
    ordered = sorted(persons, key=lambda person: person[0])
    n = len(ordered)
    if n == 0:
        return None
    assert len({key for key, _ in ordered}) == n, "lineage keys are distinct"
    assert n <= 2**53
    count = float(n)
    traits = []
    for trait in data.traits:
        values = [additive_value(genome, trait) for _, genome in ordered]
        total = 0.0
        for value in values:
            total = total + value
        mean = total / count
        squares = 0.0
        for value in values:
            deviation = value - mean
            squares = squares + deviation * deviation
        traits.append((trait.id, mean, math.sqrt(squares / count)))
    loci = []
    for locus in data.measured_loci:
        doses = sum(sum(copies_at(genome, locus)) for _, genome in ordered)
        loci.append((locus["id"], float(doses) / count))
    return {"living": n, "traits": traits, "loci": loci}


def measurement_json(clock: int, measurement: dict) -> str:
    """The line `simagent measure <run dir>` prints (design note of the measurement, section
    2), without its newline: compact JSON, keys in the note's order, floats by
    `json_number` (serde_json's shortest round trip), `clock` the last closed second."""
    return json_text(
        {
            "format_version": MEASURE_FORMAT_VERSION,
            "clock": clock,
            "living": measurement["living"],
            "traits": [{"trait": trait, "mean": mean, "sigma": sigma} for trait, mean, sigma in measurement["traits"]],
            "loci": [{"locus": locus, "mean_dose": mean_dose} for locus, mean_dose in measurement["loci"]],
        }
    )


# The format of the measurement's JSON line (design note of the measurement, section 2).
MEASURE_FORMAT_VERSION = 1


def held_weight(value: float) -> float:
    """A weight held in 0 to 1 before its root (ADR-0009 section 4): 0 if it is not greater
    than 0 (so -0.0 becomes +0.0), 1 if it is greater than 1."""
    if not value > 0.0:
        return 0.0
    return 1.0 if value > 1.0 else value


def standard_normal(seed: int, tag: int, subject: int, occasion: int, detail: int) -> float:
    """`normal(0, 1)` at the key (tools/rng_reference.py: `0 + 1 · (sqrt(−2 ln(u0)) ·
    cos(2π u1))` with the correctly rounded ln and cos)."""
    draw = words(chacha_key(seed, tag, subject, occasion, detail))
    return normal_reference(draw[0], draw[1], 0.0, 1.0)[1]


def phenotype_parts(seed: int, trait: PhenotypeTrait, person: dict, key: int, parents, age_seconds: int, c_seed=None) -> dict:
    """The phenotype `z_t` of a sim at the age `age_seconds` (a duration from its birth)
    and its parts (genetics specification 4.2, ADR-0009 sections 3 and 4): `x =
    as_years(age)`, `s = 1 / (1 + exp((12 − x) / 2))` with the correctly rounded exp, `h2 =
    h2_child + (h2_adult − h2_child) · s` and `c2` likewise, `e2 = (1 − h2) − c2`, each held in
    0 to 1; `C` = normal(ENV_SHARED, the mother's lineage key, `str_key` of the trait, the
    father's lineage key), with its own key for both without parents (a founder); `E` =
    normal(ENV_INDIVIDUAL, its own key, `str_key` of the trait, 0); `z = ((sqrt(h2) · G) +
    (sqrt(c2) · C)) + (sqrt(e2) · E)`. `parents` is (mother's key, father's key) or None.
    `seed` draws `E`, `c_seed` (`seed` if None) draws `C` (design note of the arrival,
    section 6.2: a pool person, or a town person of two pool persons, draws `C` with the
    pool seed)."""
    g = genetic_value(person, trait)
    x = as_years(age_seconds)
    s = 1.0 / (1.0 + exp_reference((12.0 - x) / 2.0)[1])
    h2_child, c2_child, h2_adult, c2_adult = trait.weights
    h2 = h2_child + (h2_adult - h2_child) * s
    c2 = c2_child + (c2_adult - c2_child) * s
    e2 = (1.0 - h2) - c2
    h2, c2, e2 = held_weight(h2), held_weight(c2), held_weight(e2)
    mother, father = (key, key) if parents is None else parents
    shared = standard_normal(seed if c_seed is None else c_seed, ENV_SHARED, mother, trait.key, father)
    own = standard_normal(seed, ENV_INDIVIDUAL, key, trait.key, 0)
    z = ((math.sqrt(h2) * g) + (math.sqrt(c2) * shared)) + (math.sqrt(e2) * own)
    return {"G": g, "x": x, "s": s, "h2": h2, "c2": c2, "e2": e2, "C": shared, "E": own, "z": z, "env": (mother, father)}


def phenotype(seed: int, trait: PhenotypeTrait, person: dict, key: int, parents, age_seconds: int, c_seed=None) -> float:
    """`z_t` of `phenotype_parts`."""
    return phenotype_parts(seed, trait, person, key, parents, age_seconds, c_seed)["z"]


def libm_margin(name: str, x: float) -> float:
    """How far the exact value of exp, ln or cos at `x` lies from the rounding boundary of
    its f64, in ulps of the result (0.5 at an f64, 0 at a midpoint): only where this is
    small can a libm that is not correctly rounded differ in the last bit."""
    function = {"exp": (exp_reference, mpmath.exp), "ln": (ln_reference, mpmath.log), "cos": (cos_reference, mpmath.cos)}[name]
    kind, value = function[0](x)
    if kind == "E":
        return 0.5
    with mpmath.workdps(60):
        exact = function[1](mpmath.mpf(x))
        return float(0.5 - abs(exact - mpmath.mpf(value)) / mpmath.mpf(math.ulp(value)))


def phenotype_margins(seed: int, trait: PhenotypeTrait, key: int, parents, age_seconds: int) -> list:
    """The margins (`libm_margin`) of the exp of `s` and of the ln and cos of both
    environment draws of a phenotype."""
    from rng_reference import TAU, unit_open0

    x = as_years(age_seconds)
    margins = [("exp", libm_margin("exp", (12.0 - x) / 2.0))]
    mother, father = (key, key) if parents is None else parents
    for tag, address in ((ENV_SHARED, (mother, trait.key, father)), (ENV_INDIVIDUAL, (key, trait.key, 0))):
        draw = words(chacha_key(seed, tag, *address))
        margins.append(("ln", libm_margin("ln", unit_open0(draw[0]))))
        margins.append(("cos", libm_margin("cos", TAU * unit(draw[1]))))
    return margins


# The ranges `mortality` accepts (crates/sim-core/src/mortality.rs, a contract of the
# merged pure functions): a world never calls them outside, the data keep inside.
GOMPERTZ_A_RANGE = (1e-15, 1.0)
GOMPERTZ_B_RANGE = (1e-3, 1.0)
MAX_SIGMA_G = 5.0
MULTIPLIER_RANGE = (1e-14, 1e14)
MAX_BUDGET = 1000.0
ACCIDENT_RANGE = (1e-9, 1000.0)


def within(x: float, bounds: tuple) -> bool:
    return bounds[0] <= x <= bounds[1]


def whole_months(entry) -> int:
    """A number of the life course data in months that the world counts in whole months
    (the pause after a birth, the months of pregnancy): its value as an integer. This
    model takes whole months only (a fraction would need a rounding rule)."""
    assert entry["unit"] == "month", entry
    value = number(entry)
    assert value >= 0.0 and value.is_integer(), f"{value} is no whole number of months"
    return int(value)


def whole_number(entry, unit: str) -> int:
    """A number of the life course data the market counts whole (the market age in years,
    the number of candidates): its value as an integer."""
    assert entry["unit"] == unit, entry
    value = number(entry)
    assert value >= 0.0 and value.is_integer(), f"{value} is no whole number"
    return int(value)


class PoolDataRejected(Exception):
    """The data check of a pool (design note of the arrival, section 2; design note of the
    pool's records, section 6; design note of the switch to map 4, section 12) rejects it:
    `code` is `pool.carer_not_member`, `pool.sex`, `pool.locus`, `pool.frozen_hash`,
    `pool.frequencies_loci` or `pool.frequency_range`."""

    def __init__(self, code: str, detail: str = ""):
        super().__init__(f"{code} {detail}".strip())
        self.code = code


class Invariant(Exception):
    """An invariant break of the core (a panic with its code): `pool.empty`,
    `pool.carried_before_arrival`."""

    def __init__(self, code: str, detail: str = ""):
        super().__init__(f"{code} {detail}".strip())
        self.code = code


# The seven loci of the pool (design note of the pool file, section 3), in its table's order.
POOL_LOCI = data_reference.POOL_LOCI


def genome_from_raw(raw: tuple) -> dict:
    """A genome as the pool file holds it (`pool_reference.decode_genome`: three maternal and
    three paternal autosome words, the maternal X, (0, the paternal X) or (1, the Y), the mt)
    in the form of tools/meiosis_reference.py with `paternal_sex` (the form of a child of
    the meiosis); its bytes are the file's (asserted)."""
    import pool_reference

    maternal, paternal, x_maternal, (kind, second), mt = raw

    def rows(words_) -> list:
        return [[(words_[(24 * s + i) // 64] >> ((24 * s + i) % 64)) & 1 for i in range(24)] for s in range(8)]

    def slots(value: int, n: int) -> list:
        assert value >> n == 0
        return [(value >> i) & 1 for i in range(n)]

    person = {
        "autosomes_maternal": rows(maternal),
        "autosomes_paternal": rows(paternal),
        "x_maternal": slots(x_maternal, 16),
        "paternal_sex": {"x": slots(second, 16)} if kind == 0 else {"y": slots(second, 8)},
        "mt": slots(mt, 8),
    }
    assert genome_of(person) == pool_reference.encode_genome(raw)
    return person


class DataPool:
    """The pool a world's data hold (design note of the arrival, section 2): the file's bytes
    read by `pool_reference.read_pool` (every rule of the reader; `pool_reference.Rejected`),
    then the data checks in their order, each over the whole pool before the next:
    `pool.carer_not_member` (a member's carer key is not a member of its household),
    `pool.sex` (the paternal sex chromosome of a member's or an unborn child's genome, X for
    a woman, Y for a man, differs from its sex), `pool.locus` (one of the seven loci missing
    from the map), `pool.frozen_hash` (the file's `frozen_hash` differs from `data_hash`, the
    hash of the data's documents before the pool's bytes are appended; the file's `data_hash`
    is not read), then the frequencies the data carry with the pool (S2a,
    `data_reference.frequencies_rejection`): `pool.frequencies_loci` (not exactly the seven
    loci), `pool.frequency_range` (a `q` not in (0, 1)). Holds the seed, `E_p`, the hash (xxh3
    of the bytes), the frequencies {locus: q}, the households, each
    household's encoding (`pool_reference.encode_household`, what `Start` carries), its
    candidate (the smallest lineage key of a member of 18 or more at `E_p`, which the reader
    guarantees) and its number of members, and indices of the members, the unborn children
    and the records by lineage key."""

    def __init__(self, data: bytes, loci: dict, data_hash: int, frequencies: dict):
        import pool_reference

        self.bytes = data
        self.file = pool_reference.read_pool(data)
        self.hash = hash64(data)
        self.seed = self.file["pool_seed"]
        self.frozen_at = self.file["frozen_at"]
        self.households = self.file["households"]
        for index, household in enumerate(self.households):
            keys = {member["lineage_key"] for member in household["members"]}
            for member in household["members"]:
                if member["carer"] is not None and member["carer"] not in keys:
                    raise PoolDataRejected("pool.carer_not_member", f"household {index} member {member['lineage_key']:016x}")
        for index, household in enumerate(self.households):
            persons = list(household["members"]) + [child for pregnancy in household["pregnancies"] for child in pregnancy["children"]]
            for person in persons:
                if person["genome"][3][0] != person["sex"]:
                    raise PoolDataRejected("pool.sex", f"household {index} person {person['lineage_key']:016x}")
        missing = [locus for locus in POOL_LOCI if locus not in loci]
        if missing:
            raise PoolDataRejected("pool.locus", " ".join(missing))
        if self.file["frozen_hash"] != data_hash:
            raise PoolDataRejected("pool.frozen_hash", f"pool {self.file['frozen_hash']:016x} data {data_hash:016x}")
        self.frozen_hash = self.file["frozen_hash"]
        code = data_reference.frequencies_rejection(frequencies)
        if code is not None:
            raise PoolDataRejected(code, " ".join(f"{locus}={q!r}" for locus, q in sorted(frequencies.items())))
        self.frequencies = {locus: float(frequencies[locus]) for locus in sorted(frequencies)}
        self.encoded = [pool_reference.encode_household(household) for household in self.households]
        self.candidates = []
        self.sizes = []
        self.members, self.unborn, self.records = {}, {}, {}
        for index, household in enumerate(self.households):
            adults = [m["lineage_key"] for m in household["members"] if self.frozen_at - m["born"] >= 18 * YEAR]
            assert adults, "the reader guarantees a member of 18 or more"
            self.candidates.append(min(adults))
            self.sizes.append(len(household["members"]))
            for member in household["members"]:
                assert member["lineage_key"] not in self.members
                self.members[member["lineage_key"]] = (index, member)
            for pregnancy in household["pregnancies"]:
                for child in pregnancy["children"]:
                    assert child["lineage_key"] not in self.unborn
                    self.unborn[child["lineage_key"]] = (index, pregnancy, child)
        assert len(set(self.candidates)) == len(self.candidates), "the candidates are unique"
        for record in self.file["ancestors"]:
            self.records[record["lineage_key"]] = record


class Data:
    """The definitions a world runs on: the life course data, the calibration and the
    genome map as documents (parsed JSON with their texts, in the order of their data
    IDs), their data hash (tools/data_reference.py), the loci
    by ID, the diseases in data order, the rates of death per sex (Gompertz `a` and `b`,
    the Makeham term `λ`), `σ_g` and the trait of longevity, the fertility (its form, the
    end of a woman's fertility `E`, the trait of the puberty timing and per sex the curve
    `(θ, σ_θ)` of 5.2 that `θ_i` reads; from the life course data the adult rate, the
    pause after a birth and the months of pregnancy, the wish shape and its tilt, and the
    rates of twins `(p_id, p_fr)`), and what the meiosis reference reads:
    the map lengths, the sex ratio at birth, the map in its form and the mutation rates; the
    market's values with the weights of the similarity terms and the traits of cognition
    and height (`PhenotypeTrait` or None). With a pool (`pool`, the file's bytes) the data
    carry the pool's frequencies (`frequencies`, {locus: q}; S2a, design note of the switch
    to map 4, section 12): `Data::with_pool(definitions, pool_bytes, frequencies)`. They travel
    with the pool: data without a pool carry none (`pool_frequencies` None)."""

    def __init__(self, demography: dict, calibration: dict, genome_map: dict, pool=None, frequencies=None):
        definitions = data_reference.definitions_bytes(demography, calibration, genome_map)
        self.hash = xxhash.xxh3_64_intdigest(definitions, seed=0)
        assert self.hash == data_reference.documents_hash(demography, calibration, genome_map)
        self.loci = {locus["id"]: locus for locus in genome_map["loci"]}
        # The pool of the data (design note of the arrival, section 2) with its frequencies,
        # or None. The data hash with a pool: the definitions' postcard followed by the
        # postcard of `(pool_seed, pool_hash, frequencies)` (`data_reference.pool_suffix`).
        self.pool = None
        self.pool_frequencies = None
        if pool is not None:
            self.pool = DataPool(pool, self.loci, self.hash, {} if frequencies is None else dict(frequencies))
            self.pool_frequencies = self.pool.frequencies
            self.hash = data_reference.data_hash_with_pool(definitions, self.pool.seed, self.pool.hash, self.pool.frequencies)
        else:
            assert frequencies is None, "the frequencies travel with the pool"
        # What the measurement reads (design note of the measurement, section 1): every trait
        # of the map in map order as its terms, and the loci of the mean doses, those whose
        # action is `additive`, in map order (decision 1 of the notes of the measurement: a
        # threshold locus is not an additive locus; `threshold_loci` holds those apart).
        stripped_map = data_reference.strip(genome_map)
        self.traits = [TraitTerms(entry["id"], stripped_map) for entry in stripped_map["traits"]]
        self.measured_loci = [
            locus for locus in stripped_map["loci"] if isinstance(locus["action"], dict) and "additive" in locus["action"]
        ]
        self.threshold_loci = [
            locus for locus in stripped_map["loci"] if isinstance(locus["action"], dict) and "threshold" in locus["action"]
        ]
        life = data_reference.strip(demography)
        self.adult_rate = number(life["values"]["core:fertility_adult_rate"])
        self.pause_months = whole_months(life["values"]["core:fertility_pause"])
        self.pregnancy_months = whole_months(life["values"]["core:pregnancy_length"])
        assert self.pregnancy_months >= 1
        self.wish_tilt = number(life["values"]["core:fertility_wish_tilt"])
        self.wish_shape = [number(entry) for entry in life["fertility_wish_shape"]]
        # The table of leaving home (design note of the households, section 1): ten weights
        # for the ages 16 to 25, each in [0, 1], at least one above 0 (data_reference checks
        # the same when it hashes the documents).
        self.leave_home_weights = [number(entry) for entry in life["leave_home_ages"]]
        assert len(self.leave_home_weights) == 10, self.leave_home_weights
        assert all(0.0 <= weight <= 1.0 for weight in self.leave_home_weights), self.leave_home_weights
        assert any(weight > 0.0 for weight in self.leave_home_weights), self.leave_home_weights
        self.twins = (number(life["values"]["core:twins_identical"]), number(life["values"]["core:twins_fraternal"]))
        assert all(0.0 <= rate <= 1.0 for rate in self.twins), self.twins
        # `λ` of the separation, per year (design note of the separation, section 1); 0 is
        # a world without separation.
        self.separation_rate = number(life["values"]["core:separation_rate"])
        assert math.isfinite(self.separation_rate) and self.separation_rate >= 0.0, self.separation_rate
        # The market (design note of the market, section 1; genetics specification 11.2):
        # the market age in whole years, `c`, `m`, the width `w`, `ρ`, `k`, `w_a`, `μ_a`, the
        # limit of the kinship and the odds of a cousin.
        market = life["values"]
        self.market_age = whole_number(market["core:market_age"], "year")
        self.market_height = number(market["core:market_search_height_female"])
        self.market_center = number(market["core:market_search_center_female"])
        self.market_width = number(market["core:market_search_width"])
        self.market_minor = number(market["core:market_search_factor_minor"])
        self.market_candidates = whole_number(market["core:market_candidates"], "count")
        self.pairing_weight_age = number(market["core:pairing_weight_age"])
        self.pairing_gap = number(market["core:pairing_age_gap_preferred"])
        self.kinship_limit = number(market["core:pairing_kinship_limit"])
        self.cousin_odds = number(market["core:pairing_cousin_odds"])
        # The weights of the similarity terms (genetics specification 11.2; design note of the
        # similarity, section 1): per squared difference of the phenotypes.
        self.pairing_weight_cognition = number(market["core:pairing_weight_cognition"])
        self.pairing_weight_height = number(market["core:pairing_weight_height"])
        assert self.market_age == 16 and self.kinship_limit == 0.125, "the note fixes 16 years and 1/8"
        for weight in (self.pairing_weight_cognition, self.pairing_weight_height):
            assert math.isfinite(weight) and weight >= 0.0, weight
        # `c` is a fraction like the other shares of the market: data with `c` outside [0, 1]
        # are rejected (the core's loader does the same; `search_chance` caps `s` at 1, so
        # without this bound a `c` above 1 would still run here).
        assert math.isfinite(self.market_height) and 0.0 <= self.market_height <= 1.0, self.market_height
        assert math.isfinite(self.market_center) and self.market_width > 0.0 and math.isfinite(self.market_width)
        assert 0.0 < self.market_minor <= 1.0 and self.market_candidates >= 1
        assert math.isfinite(self.pairing_weight_age) and self.pairing_weight_age >= 0.0
        assert math.isfinite(self.pairing_gap) and math.isfinite(self.cousin_odds) and self.cousin_odds >= 1.0
        stripped = data_reference.strip(calibration)
        self.diseases = [Disease(entry, self.loci, stripped, data_reference.strip(genome_map)) for entry in stripped["diseases"]]
        values = stripped["values"]
        self.senescence = {
            sex: (number(values[f"core:senescence_a_{name}"]), number(values[f"core:senescence_b_{name}"]))
            for sex, name in ((FEMALE, "female"), (MALE, "male"))
        }
        self.accident = {sex: number(values[f"core:accident_{name}"]) for sex, name in ((FEMALE, "female"), (MALE, "male"))}
        self.sigma_g = number(values["core:longevity_sigma_g"])
        for a, b in self.senescence.values():
            assert within(a, GOMPERTZ_A_RANGE) and within(b, GOMPERTZ_B_RANGE), (a, b)
        assert all(within(rate, ACCIDENT_RANGE) for rate in self.accident.values())
        assert within(self.sigma_g, (0.0, MAX_SIGMA_G))
        # With a trait of longevity `m = e^(−σ_g · ĝ)` reads the genetic value `g_L` of the
        # trait (genetics specification 7.2, 11.4; ADR-0017 section 2): its terms
        # (`TraitTerms`, no phenotype row needed); without one `m` = 1 exactly (11.1).
        self.longevity_trait = stripped["longevity_trait"]
        self.longevity = None if self.longevity_trait is None else TraitTerms(self.longevity_trait, stripped_map)
        # Likewise `θ_i` with a trait of the puberty timing reads its phenotype `z_p`
        # (genetics specification 5.2; ADR-0012 section 3: its weights do not depend on the
        # age, asserted bit for bit); without one `z_p` = 0 and `θ_i = θ` exactly (11.1).
        self.puberty_trait = stripped["puberty_trait"]
        self.puberty = self.phenotype_trait(self.puberty_trait, stripped, genome_map)
        if self.puberty is not None:
            h2_child, c2_child, h2_adult, c2_adult = self.puberty.weights
            assert data_reference.f64_bits(h2_child) == data_reference.f64_bits(h2_adult), "the puberty trait's h2 depends on the age"
            assert data_reference.f64_bits(c2_child) == data_reference.f64_bits(c2_adult), "the puberty trait's c2 depends on the age"
        # The traits of the similarity terms of the market (design note of the similarity,
        # sections 1 and 2): each a trait of the map with phenotype weights, or None (the
        # term is left out, 11.1).
        self.cognition_trait = self.phenotype_trait(stripped["cognition_trait"], stripped, genome_map)
        self.height_trait = self.phenotype_trait(stripped["height_trait"], stripped, genome_map)
        self.fertility = FertilityForm(stripped["fertility"])
        # The end of her fertility (design note of the fertility in the world, section 2):
        # from this age on her factor stays 0.
        if self.fertility.kind == "puberty":
            self.fertility_end = self.fertility.female_decline_to
        else:
            self.fertility_end = self.fertility.until
        self.puberty_curve = {
            sex: (number(values[f"core:growth_theta_{name}"]), number(values[f"core:growth_theta_sd_{name}"]))
            for sex, name in ((FEMALE, "female"), (MALE, "male"))
        }
        self.map_lengths = {
            "autosome_female": values["core:map_length_autosome_female"]["value"],
            "autosome_male": values["core:map_length_autosome_male"]["value"],
            "x_female": values["core:map_length_x_female"]["value"],
        }
        self.sex_ratio = values["core:sex_ratio_at_birth"]["value"]
        self.genome_map = [
            {
                "id": locus["id"],
                "chromosome": locus["chromosome"],
                "segment": locus["segment"]
                if locus["chromosome"] == "autosome"
                else (8 if locus["chromosome"] == "x" else 0),
                "slot": locus["slot"],
                "mark": locus["mark"],
                "direction": locus["mutation"],
            }
            for locus in genome_map["loci"]
        ]
        self.rates = {
            "standard": values["core:mutation_rate"]["value"],
            "disease": values["core:mutation_rate_disease"]["value"],
        }

    def multiplier(self, person: dict, nudge: int = 0) -> float:
        """`m` of the Gompertz term (7.2): 1 exactly without a trait of longevity
        (`exp(−0.0)` is 1, 11.1); with one `m = exp(−(σ_g · ĝ))`, `ĝ` the genetic value
        `g_L` of the trait held on [−6, 6] (11.4, ADR-0017 section 2), exp correctly rounded;
        `nudge` moves an inexact exp by whole ulps (the check of `robust_seconds`)."""
        if self.longevity is None:
            return 1.0
        held = longevity_input(genetic_value(person, self.longevity))
        kind, m = exp_reference(-(self.sigma_g * held))
        if kind == "P":
            m = step_ulps(m, nudge)
        return m

    @staticmethod
    def phenotype_trait(named, calibration: dict, genome_map: dict):
        """The trait a field of the calibration names, as `PhenotypeTrait`, or None."""
        if named is None:
            return None
        (row,) = [row for row in calibration["traits"] if row["trait"] == named]
        return PhenotypeTrait(named, data_reference.strip(genome_map), row)

    def similarity_traits(self) -> list:
        """The present similarity terms in the order of 11.2: (name, trait, weight) for
        cognition, then height, each only if its trait is named."""
        terms = [("cognition", self.cognition_trait, self.pairing_weight_cognition), ("height", self.height_trait, self.pairing_weight_height)]
        return [term for term in terms if term[1] is not None]


def repository_data() -> Data:
    """The repository's data as the CLI builds them (slice S5): the documents under data/, the
    pool data/pool/pool.bin and its frequencies data/pool/pool_frequencies.json
    (`Data::with_pool`)."""
    files = data_reference.pool_files(ROOT / "data")
    assert files is not None, "the repository's data hold the pool (data/pool/)"
    pool, frequencies = files
    return Data(*data_reference.documents(ROOT / "data"), pool=pool, frequencies=frequencies)


# The map 3 fixture (design note of the switch, section 4): tests/golden/map3 holds the map 3
# and its calibration as data/ held them before S5, byte for byte; with the repository's life
# course data its data hash is that of data/ before the switch. The data of every core test
# whose subject is a rule (tests/common `files()`), and of `death_test_data` and
# `life_test_data`.
MAP3_DIR = ROOT / "tests" / "golden" / "map3"


def test_value(value: float, unit: str) -> dict:
    """A number of test data with its texts (they do not enter the data hash)."""
    return {
        "value": value,
        "unit": unit,
        "source": "test data (ADR-0017 section 5)",
        "as_of": "2026-10-11",
        "note": "test value, not calibrated",
    }


# The wish of the test data with the window: always 40 (41 weights for 0 to 40 children,
# all 0 but the last), so the wish never stops a series (design note of the fertility in
# the world, section 8; ADR-0017 section 5).
TEST_WISH = 40


def window_fertility(demography: dict, calibration: dict, fertility: tuple) -> None:
    """The fertility of a scenario that gave one in `Start` (design note of the fertility in
    the world, section 8), written into the documents: `fertility` is (per month `r`, from
    `a`, until `b` years, `k` months) as the old `Start.fertility`; the form `window` from
    `a` until `b`, `core:fertility_adult_rate` = `r`, `core:pregnancy_length` = `k`,
    `core:fertility_pause` 0, the wish shape of `TEST_WISH` + 1 weights all 0 but the last
    1 (the tilt stays), both rates of twins 0, `core:separation_rate` 0 (the old model
    knows no separation, ADR-0017 section 5; design note of the separation, section 7) and
    `core:market_search_height_female` (`c`) 0: no woman ever searches, the round runs and
    writes nothing (design note of the market, section 7)."""
    per_month, start, until, months = fertility
    assert per_month > 0.0
    calibration["fertility"] = {
        "window": {"from": test_value(float(start), "year"), "until": test_value(float(until), "year")}
    }
    values = demography["values"]
    values["core:fertility_adult_rate"]["value"] = per_month
    values["core:pregnancy_length"]["value"] = float(months)
    values["core:fertility_pause"]["value"] = 0.0
    values["core:twins_identical"]["value"] = 0.0
    values["core:twins_fraternal"]["value"] = 0.0
    values["core:separation_rate"]["value"] = 0.0
    values["core:market_search_height_female"]["value"] = 0.0
    demography["fertility_wish_shape"] = [test_value(0.0, "fraction") for _ in range(TEST_WISH)] + [
        test_value(1.0, "fraction")
    ]


def death_test_data(
    accident: float,
    senescence_a: float = 1e-15,
    senescence_b: float = 0.001,
    diseases=None,
    fertility=None,
    life_values=None,
    wish_shape=None,
    data_dir: Path = MAP3_DIR,
) -> Data:
    """Test data for the deaths (design note of the death from the data, section 4): the
    repository's life course data, calibration and map with the Makeham term `accident`
    and the Gompertz `a`, `b` for both sexes, and `diseases` (a list of disease
    documents) instead of the repository's six if given. A scenario that had causes of `Start` takes the rate of
    its first cause as `accident` and that of its second as `senescence_a` with `b` =
    0.001; without a second cause `a` = 1e-15 and `b` = 0.001, senescence practically
    off (its death lies some 20 000 years ahead). A scenario that had a fertility in
    `Start` gives it as `fertility` (`window_fertility`); otherwise the fertility is the
    repository's. `life_values` (a dict of code to value) sets values of the life course
    data after that, `wish_shape` (a list of weights) the wish shape. The base is the map 3
    fixture (`MAP3_DIR`, the data of the core tests) unless `data_dir` names another
    directory."""
    demography, calibration, genome_map = data_reference.documents(data_dir)
    values = calibration["values"]
    for sex in ("female", "male"):
        values[f"core:accident_{sex}"]["value"] = accident
        values[f"core:senescence_a_{sex}"]["value"] = senescence_a
        values[f"core:senescence_b_{sex}"]["value"] = senescence_b
    if diseases is not None:
        calibration["diseases"] = diseases
    if fertility is not None:
        window_fertility(demography, calibration, fertility)
    for code, value in (life_values or {}).items():
        demography["values"][code]["value"] = value
    if wish_shape is not None:
        demography["fertility_wish_shape"] = [test_value(float(weight), "fraction") for weight in wish_shape]
    return Data(demography, calibration, genome_map)


def life_test_data(life_values: dict, data_dir: Path = MAP3_DIR) -> Data:
    """The map 3 fixture's data (or those of the fixture `data_dir`; both take the
    repository's life course data) with values of the life course data changed
    (`life_values`, a dict of code to value) and nothing else: the rates of death, the
    diseases and the fertility stay those of the directory (the scenario "market", design
    note of the market, section 7; "similar" on tests/golden/data_pairing, design note of
    the similarity, section 4)."""
    demography, calibration, genome_map = data_reference.documents(data_dir)
    for code, value in life_values.items():
        demography["values"][code]["value"] = value
    return Data(demography, calibration, genome_map)


def dummy_disease(onset_years: float) -> dict:
    """The disease of the walking skeleton as data (design note section 6, ADR-0017
    section 5): `test:dummy_disease` on `core:disease_ar`, penetrance 1 for both sexes,
    the onset fixed at `onset_years`, death with 0.3 per year."""

    def calibrated(value: float, unit: str) -> dict:
        return {
            "value": value,
            "unit": unit,
            "source": "test data of the walking skeleton (ADR-0017 section 5)",
            "as_of": "2026-10-10",
            "note": "test value, not calibrated",
        }

    return {
        "code": "test:dummy_disease",
        "carrier": {
            "locus": {
                "locus": "core:disease_ar",
                "penetrance_female": calibrated(1.0, "fraction"),
                "penetrance_male": calibrated(1.0, "fraction"),
            }
        },
        "onset": {"fixed": {"age": calibrated(onset_years, "year")}},
        "effect": {"death": {"per_year": calibrated(0.3, "per_year")}},
    }


MAP3 = Data(*data_reference.documents(MAP3_DIR))
assert MAP3.hash == 0xADAB_5027_57B4_045C, f"{MAP3.hash:016x}"
# The "fork data" of the acceptance scenario `fork_no_density` (ADR-0017 section 5, a core
# test on the map 3 fixture): the old model, accident 0.02 per year, senescence practically off (`a` = 1e-12, `b` = 0.1),
# the dummy disease from 18; the fertility `window` from 18 until 45 with 0.05 per month,
# ten months of pregnancy, no pause, the wish always 40, no twins (design note of the
# fertility in the world, section 8); the market off (`c` = 0) and no separation.
FORK_FERTILITY = (0.05, 18, 45, 10)
FORK_LIFE_VALUES = {"core:market_search_height_female": 0.0, "core:separation_rate": 0.0}
FORK_DATA = death_test_data(
    0.02, 1e-12, 0.1, [dummy_disease(18.0)], fertility=FORK_FERTILITY, life_values=FORK_LIFE_VALUES
)
# The repository's data (`REPOSITORY`) and their data hash (`DATA_HASH`, ADR-0007) are read on
# first use: with the pool, `DataPool` runs the pool's reader (tools/pool_reference.py), which
# imports tools that import this module, so they cannot be built while it loads. A tool that
# never asks for them never reads the 8 MB pool.
_REPOSITORY = []


def repository() -> Data:
    """The repository's data as the CLI builds them (`repository_data`), read once; their data
    hash is checked against their own reference (`data_reference.repository_hash`: the
    definitions, then the pool's seed and hash and the frequencies), and the definitions alone
    must hash to the pool's `frozen_hash`."""
    if not _REPOSITORY:
        data = repository_data()
        assert data.hash == data_reference.repository_hash(ROOT / "data")
        assert data.pool.frozen_hash == data_reference.data_hash(ROOT / "data")
        _REPOSITORY.append(data)
    return _REPOSITORY[0]


def __getattr__(name: str):
    """`REPOSITORY`, `DATA_HASH` (the repository's data hash with the pool and its
    frequencies) and `EMPTY_SAVE` (the save of a world that never started on them), read on
    first use (`repository`)."""
    if name == "REPOSITORY":
        return repository()
    if name == "DATA_HASH":
        return repository().hash
    if name == "EMPTY_SAVE":
        return empty_save(repository().hash)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


# ---------------------------------------------------------------------------------------
# Genomes


def founder_genome(sex: bytes, genes, loci=None) -> dict:
    """The genome of a founder in the form of tools/meiosis_reference.py, from its
    alleles: the pair at `core:disease_ar` (from `genotype`) or a list of (locus, copies).
    The copies are maternal then paternal; their number follows the chromosome and the
    sex: two on an autosome, two on a woman's X and one on a man's, one on a man's Y,
    one mt. Every slot not named holds the reference allele."""
    loci = repository().loci if loci is None else loci
    if isinstance(genes, tuple):
        genes = [("core:disease_ar", list(genes))]
    maternal = [[0] * 24 for _ in range(8)]
    paternal = [[0] * 24 for _ in range(8)]
    x_maternal, x_paternal, y, mt = [0] * 16, [0] * 16, [0] * 8, [0] * 8
    for locus_id, copies in genes:
        locus = loci[locus_id]
        chromosome, slot = locus["chromosome"], locus["slot"]
        if chromosome == "autosome":
            assert len(copies) == 2, f"{locus_id}: two copies on an autosome"
            maternal[locus["segment"]][slot], paternal[locus["segment"]][slot] = copies
        elif chromosome == "x":
            assert len(copies) == (2 if sex == FEMALE else 1), f"{locus_id}: the X copies of the sex"
            x_maternal[slot] = copies[0]
            if sex == FEMALE:
                x_paternal[slot] = copies[1]
        elif chromosome == "y":
            assert sex == MALE and len(copies) == 1, f"{locus_id}: one Y copy, men only"
            y[slot] = copies[0]
        else:
            assert len(copies) == 1, f"{locus_id}: one mt copy"
            mt[slot] = copies[0]
    person = {"autosomes_maternal": maternal, "autosomes_paternal": paternal, "x_maternal": x_maternal, "mt": mt}
    if sex == FEMALE:
        person["x_paternal"] = x_paternal
    else:
        person["paternal_sex"] = {"y": y}
    return person


def named_alleles(genes) -> list:
    """The loci a founder names, as `Start` lists them (`postcard_reference.founder_alleles`):
    the pair at `core:disease_ar` (from `genotype`; (0, 0) names nothing) or the list of
    (locus, copies) as given, a pair `[0, 0]` included."""
    if isinstance(genes, tuple):
        return [] if genes == (0, 0) else [("core:disease_ar", list(genes))]
    return [(locus, list(copies)) for locus, copies in genes]


def founder_gametes(locus: dict, sex: bytes) -> list:
    """The copies a founder of `sex` has at `locus`, as gamete numbers in the order of its
    `copies` (design note of the switch to map 4, section 2): autosome 0 and 1; X 0 and 1 for a
    woman, 0 for a man; Y 1 for a man, none for a woman; mt 0."""
    chromosome = locus["chromosome"]
    if chromosome == "autosome":
        return [0, 1]
    if chromosome == "x":
        return [0, 1] if sex == FEMALE else [0]
    if chromosome == "y":
        return [1] if sex == MALE else []
    assert chromosome == "mt", chromosome
    return [0]


def fixed_alleles(loci: dict, sex: bytes, named=()) -> list:
    """Alleles that name every locus of the map (`loci` by ID, in map order) a founder of
    `sex` carries: the copies of `named` (a list of (locus, copies)) where it names the locus,
    the reference allele on every copy elsewhere (`[0, 0]`, one `[0]` on a man's X, his Y and
    the mt; a woman names no Y locus). Under F1 such a founder draws nothing, so it has the
    genome its named alleles alone gave before F1 (the convention of the pins for the test
    fixtures with additive loci, design note of the switch to map 4, section 2, I-d)."""
    named = dict(named_alleles(list(named)))
    assert set(named) <= set(loci), sorted(set(named) - set(loci))
    out = []
    for locus_id, locus in loci.items():
        gametes = founder_gametes(locus, sex)
        if not gametes:
            assert locus_id not in named, f"{locus_id}: no copy for this sex"
            continue
        copies = list(named.get(locus_id, [0] * len(gametes)))
        assert len(copies) == len(gametes), (locus_id, copies)
        out.append((locus_id, copies))
    return out


def founder_allele_q(data, locus: dict):
    """`q` of a founder's draw at an unnamed locus (Grok F1, P2 (a)): `d_ref / 2` at an
    additive locus (the map's `d_ref`, divided once in f64, exact); at one of the seven loci of
    the pool the data's frequency if the data carry frequencies (data with a pool); None (the
    reference allele, no draw) at every other locus: a recessive or dominant locus, a
    threshold locus, and the seven loci on data without frequencies."""
    action = locus["action"]
    if isinstance(action, dict) and "additive" in action:
        return float(action["additive"]["d_ref"]) / 2.0
    frequencies = data.pool_frequencies
    if frequencies is not None and locus["id"] in frequencies:
        return frequencies[locus["id"]]
    return None


def founder_allele_unit(seed: int, key: int, locus_id: str, gamete: int) -> float:
    """`u = unit` of the draw (the world seed, FOUNDER_ALLELE, the founder's lineage key,
    `str_key` of the locus, the gamete), as PRERUN_ALLELE is drawn
    (`prerun_reference.alleles_of`)."""
    return unit(words(chacha_key(seed, FOUNDER_ALLELE, key, str_key(locus_id), gamete))[0])


def founder_alleles_drawn(data, seed: int, key: int, sex: bytes, genes) -> tuple:
    """A founder's alleles under F1: (the named loci as `Start` lists them, the drawn loci as
    (locus, copies) in map order, the draws as (locus, gamete, q, u, allele) in map order then
    gamete order). Every locus the founder names keeps its copies (both copies as named,
    Grok P3 (a): naming is per locus); every other locus whose `q` is not None
    (`founder_allele_q`) is drawn, one draw per copy, loci independent: the allele 1 iff `u <
    q`."""
    named = named_alleles(genes)
    names = {locus for locus, _ in named}
    drawn, draws = [], []
    for locus_id, locus in data.loci.items():
        if locus_id in names:
            continue
        q = founder_allele_q(data, locus)
        gametes = founder_gametes(locus, sex)
        if q is None or not gametes:
            continue
        copies = []
        for gamete in gametes:
            u = founder_allele_unit(seed, key, locus_id, gamete)
            allele = 1 if u < q else 0
            copies.append(allele)
            draws.append((locus_id, gamete, q, u, allele))
        drawn.append((locus_id, copies))
    return named, drawn, draws


def drawn_founder_genome(data, seed: int, key: int, sex: bytes, genes) -> dict:
    """The genome of a founder of `Start` under F1 (`founder_alleles_drawn`): its named loci
    and its drawn loci, every other slot the reference allele (`founder_genome`)."""
    named, drawn, _ = founder_alleles_drawn(data, seed, key, sex, genes)
    return founder_genome(sex, named + drawn, data.loci)


def copy_genome(person: dict) -> dict:
    """A genome of the form of tools/meiosis_reference.py, copied (identical twins have
    equal genomes, each its own)."""
    return copy.deepcopy(person)


def from_father(person: dict) -> tuple:
    """("x" or "y", slots) of the sex chromosome a sim has from its father."""
    if "x_paternal" in person:
        return "x", person["x_paternal"]
    return only(person["paternal_sex"])


def copies_at(person: dict, locus: dict) -> list:
    """The alleles a sim has at the locus, one per copy it has (genetics specification
    2.1, 3.4)."""
    chromosome, slot = locus["chromosome"], locus["slot"]
    if chromosome == "autosome":
        segment = locus["segment"]
        return [person["autosomes_maternal"][segment][slot], person["autosomes_paternal"][segment][slot]]
    name, second = from_father(person)
    if chromosome == "x":
        return [person["x_maternal"][slot]] + ([second[slot]] if name == "x" else [])
    if chromosome == "y":
        return [second[slot]] if name == "y" else []
    assert chromosome == "mt"
    return [person["mt"][slot]]


def has_genotype(action, copies: list) -> bool:
    """Whether the genotype can carry the disease (3.4): recessive, every copy carries
    allele 1 and there is at least one; dominant, at least one copy carries it."""
    if action == "recessive":
        return len(copies) >= 1 and all(allele == 1 for allele in copies)
    assert action == "dominant", f"a disease locus acts recessive or dominant, not {action}"
    return any(allele == 1 for allele in copies)


# ---------------------------------------------------------------------------------------
# The step hazard (genetics specification 3.1)


def disease_age(x0: float, budget: float, hazard: tuple):
    """The age at which a budget is used up from the age x0 on (3.1): at or after the
    change `x0 + B / h1`; before it `H = h0 * (s - x0)`, with a positive `h0` and
    `B <= H` the age `x0 + B / h0`, else `s + (B - H) / h1`. An `h0` of 0 is the old edge:
    nobody dies before `s`. Without `h1` a rest beyond `H` is never used up (None)."""
    before, change, after = hazard
    if x0 < change:
        used = before * (change - x0)
        if before > 0.0 and budget <= used:
            return x0 + budget / before
        if after == 0.0:
            return None
        return change + (budget - used) / after
    if after == 0.0:
        return None
    return x0 + budget / after


def disease_hazard_used(hazard: tuple, from_: float, to: float) -> float:
    """The hazard used up from the age `from_` to the age `to` (3.1): `h0` on the part
    before the change, `h1` on the part after it."""
    before, change, after = hazard
    assert 0.0 <= from_ <= to and math.isfinite(to)
    return before * max(0.0, min(to, change) - min(from_, change)) + after * max(0.0, to - max(from_, change))


def as_years(seconds: int) -> float:
    """SimDuration::as_years: the seconds exactly as f64, over the seconds of a year."""
    assert abs(seconds) <= 2**53
    return float(seconds) / 31_449_600.0


def from_years_ceil(years: float):
    """SimDuration::from_years_ceil (ADR-0002): None (never) at 100 000 years or more,
    else the seconds rounded up."""
    assert math.isfinite(years) and years > -NEVER_YEARS
    if years >= NEVER_YEARS:
        return None
    return math.ceil(years * 31_449_600.0)


# The ages at death and the hazard used up of crates/sim-core/tests/disease_hazard.rs: the
# values there come from Python's math, independent of this model (W3.1).
CYSTIC = (0.02 / 18.0, 18.0, 0.014_322_280_437_445_644)
HAEMOPHILIA = (0.000_555_555_555_555_555_6, 18.0, 0.005)
for origin, budget, hazard, want in [
    (0.0, 0.01, CYSTIC, 9.0),
    (0.0, math.log(2.0), CYSTIC, 65.0),
    (20.0, 0.5, CYSTIC, 54.910_641_652_620_384),
    (10.0, 0.05, CYSTIC, 20.870_430_535_882_12),
    (5.0, 0.1, (0.0, 18.0, 0.04), 20.5),
    (45.0, 1.0, (0.04, 0.0, 0.04), 70.0),
    (3.0, 0.004, HAEMOPHILIA, 10.2),
]:
    assert abs(disease_age(origin, budget, hazard) - want) <= 1e-13 * want, (origin, budget, want)
assert disease_age(5.0, 0.0, (0.0, 18.0, 0.04)) == 18.0
assert disease_age(5.0, 0.0, CYSTIC) == 5.0 and disease_age(20.0, 0.0, CYSTIC) == 20.0
assert disease_age(0.0, 1.5, (0.1, 10.0, 0.0)) is None and disease_age(12.0, 0.1, (0.1, 10.0, 0.0)) is None
assert disease_age(0.0, 0.0, (1e-9, 0.25, 0.0)) == 0.0
for low, high, want in [(0.0, 18.0, 0.02), (10.0, 30.0, 0.180_756_254_138_236_62), (25.0, 40.0, 0.214_834_206_561_684_64), (7.0, 7.0, 0.0)]:
    assert abs(disease_hazard_used(CYSTIC, low, high) - want) <= 1e-15 + 1e-13 * want, (low, high)


# ---------------------------------------------------------------------------------------
# Fertility (genetics specification 5.6 and 11.3; design note of the fertility curves,
# section 2): pure functions, ages in years as f64 (11.1: `(t − born).as_years()`). The
# world reads them in its attempts (design note of the fertility in the world).


def month_start_after(t: int) -> int:
    """The smallest `k · MONTH > t`, k an integer (design note of the fertility in the
    world, section 2)."""
    return (t // MONTH + 1) * MONTH


def month_start_at_or_after(t: int) -> int:
    """The smallest `k · MONTH >= t`, k an integer."""
    return -(-t // MONTH) * MONTH


assert month_start_after(0) == MONTH and month_start_after(-1) == 0 and month_start_after(MONTH - 1) == MONTH
assert month_start_at_or_after(0) == 0 and month_start_at_or_after(1) == MONTH and month_start_at_or_after(-1) == 0
assert month_start_at_or_after(-MONTH - 1) == -MONTH and month_start_after(-MONTH) == 0

# The bound of the inputs of the individual curve (ADR-0012 section 1).
CURVE_INPUT_BOUND = 6.0


def smoothstep(u: float) -> float:
    """`s(u)` of 5.1 and 6.2 as `growth::puberty_progress` evaluates it: 0 for `u <= 0`,
    1 for `u >= 1`, else `3.0 * (u * u) - 2.0 * ((u * u) * u)`."""
    if u <= 0.0:
        return 0.0
    if u >= 1.0:
        return 1.0
    return 3.0 * (u * u) - 2.0 * ((u * u) * u)


def theta_i(curve: tuple, z_p: float) -> float:
    """The puberty timing `θ_i = θ + σ_θ · ẑ_p` (5.2) on the curve `(θ, σ_θ)` of a sex
    (`Data.puberty_curve`), with `ẑ_p` the phenotype `z_p` held on [−6, 6] (above 6 is 6,
    below −6 is −6, ADR-0012 section 1): `IndividualCurve::new(curve, 0.0, z_p).theta`.
    Without a trait of the puberty timing `z_p` is 0 and `θ_i = θ` exactly (11.1)."""
    theta, sigma_theta = curve
    if z_p > CURVE_INPUT_BOUND:
        z_p = CURVE_INPUT_BOUND
    elif z_p < -CURVE_INPUT_BOUND:
        z_p = -CURVE_INPUT_BOUND
    return theta + sigma_theta * z_p


# The bound of `g_L` where death and `bio_age` read it (genetics specification 11.4,
# ADR-0017 section 2, `LONGEVITY_INPUT_BOUND`, a constant like `CURVE_INPUT_BOUND`).
LONGEVITY_INPUT_BOUND = 6.0


def longevity_input(g: float) -> float:
    """`ĝ`: `g_L` held on [−6, 6] (above 6 is 6, below −6 is −6, else `g` itself)."""
    assert math.isfinite(g), g
    if g > LONGEVITY_INPUT_BOUND:
        return LONGEVITY_INPUT_BOUND
    if g < -LONGEVITY_INPUT_BOUND:
        return -LONGEVITY_INPUT_BOUND
    return g


def menarche(theta: float, delta: float, form: FertilityForm) -> float:
    """The menarche `m_i` of a woman with the puberty timing `theta` (5.6): `m = θ_i + δ`,
    then held, `m = max(m, θ_i + menarche_low)`, then `m = min(m, θ_i + menarche_high)`.
    `δ` is `menarche_delta`; the menarche is a pure function, not state."""
    assert form.kind == "puberty"
    m = theta + delta
    m = max(m, theta + form.menarche_low)
    return min(m, theta + form.menarche_high)


def menarche_delta(seed: int, key: int, form: FertilityForm) -> float:
    """`δ = normal(menarche_mean, menarche_sd)` at the key (FERTILITY_MENARCHE, the
    woman's lineage key, occasion 0, detail 0; 11.7)."""
    draw = words(chacha_key(seed, FERTILITY_MENARCHE, key, 0, 0))
    return normal_reference(draw[0], draw[1], form.menarche_mean, form.menarche_sd)[1]


def female_factor(age: float, m: float, form: FertilityForm) -> float:
    """The woman's factor `F_w` at the age `age` (5.6). `puberty`: the ramp
    `s((age − m) / female_ramp)` from her menarche `m`, times the decline, 1 for
    `age <= female_decline_from`, 0 for `age >= female_decline_to`, else
    `(to − age) / (to − from)`. `window`: 1 if `from <= age < until`, else 0 (`m` is not
    read)."""
    if form.kind == "window":
        return 1.0 if form.start <= age < form.until else 0.0
    ramp = smoothstep((age - m) / form.female_ramp)
    start, end = form.female_decline_from, form.female_decline_to
    if age <= start:
        decline = 1.0
    elif age >= end:
        decline = 0.0
    else:
        decline = (end - age) / (end - start)
    return ramp * decline


def male_factor(age: float, theta: float, form: FertilityForm) -> float:
    """The man's factor `F_m` at the age `age` with his puberty timing `theta` (5.6).
    `puberty`: the ramp `s((age − (θ_i − male_lead)) / male_ramp)`, times the decline, 1
    for `age <= male_decline_from`, `male_floor` for `age >= male_decline_to`, else
    `1.0 - (1.0 - floor) * ((age - from) / (to - from))`. `window`: 1."""
    if form.kind == "window":
        return 1.0
    ramp = smoothstep((age - (theta - form.male_lead)) / form.male_ramp)
    start, end, floor = form.male_decline_from, form.male_decline_to, form.male_floor
    if age <= start:
        decline = 1.0
    elif age >= end:
        decline = floor
    else:
        decline = 1.0 - (1.0 - floor) * ((age - start) / (end - start))
    return ramp * decline


def chance(adult_rate: float, f_w: float, f_m: float) -> float:
    """The chance of an attempt (11.3): `(adult_rate * f_w) * f_m`."""
    return (adult_rate * f_w) * f_m


def wish_weights(shape: list, alpha: float) -> tuple:
    """`(w, total)` of the wish (11.3): `w_n = p_n * exp(α * n)` with `n` as f64 and the
    correctly rounded exp (`sim_math::exp`), `total = w_0 + w_1 + …` in order."""
    weights = [p * exp_reference(alpha * float(n))[1] for n, p in enumerate(shape)]
    total = 0.0
    for w in weights:
        total += w
    return weights, total


def wish_thresholds(shape: list, alpha: float) -> list:
    """The thresholds of the wish (11.3): `acc += w_n / total` in order, `acc` after each
    `n`."""
    weights, total = wish_weights(shape, alpha)
    thresholds, acc = [], 0.0
    for w in weights:
        acc += w / total
        thresholds.append(acc)
    return thresholds


def wish(u: float, shape: list, alpha: float) -> int:
    """The wish of a woman (11.3): the smallest `n` with `u < acc` after `n`
    (`wish_thresholds`), the last `n` if none."""
    for n, acc in enumerate(wish_thresholds(shape, alpha)):
        if u < acc:
            return n
    return len(shape) - 1


def wish_unit(seed: int, key: int) -> float:
    """`u = unit(draw)` of the wish at the key (FERTILITY_WISH, the woman's lineage key,
    occasion 0, detail 0; 11.7)."""
    return unit(words(chacha_key(seed, FERTILITY_WISH, key, 0, 0))[0])


SINGLE, IDENTICAL, FRATERNAL = "single", "identical", "fraternal"


def multiple_unit(seed: int, first_child: int) -> float:
    """`u = unit(draw)` of the twins at the key (MULTIPLE_BIRTH, the lineage key of the
    first child of the conception, occasion 0, detail 0; 11.7)."""
    return unit(words(chacha_key(seed, MULTIPLE_BIRTH, first_child, 0, 0))[0])


def multiple_kind(u: float, twins: tuple) -> str:
    """The kind of a conception (11.3, design note of the twins section 1): identical if
    `u < p_id`, fraternal if `u < p_id + p_fr` (the sum computed as written), else one
    child."""
    p_id, p_fr = twins
    if u < p_id:
        return IDENTICAL
    if u < p_id + p_fr:
        return FRATERNAL
    return SINGLE


assert multiple_kind(0.0, (0.0, 0.0)) == SINGLE and multiple_kind(0.0039, (0.004, 0.010)) == IDENTICAL
assert multiple_kind(0.004, (0.004, 0.010)) == FRATERNAL and multiple_kind(0.014, (0.004, 0.010)) == SINGLE
assert multiple_kind(0.3, (0.3, 0.7)) == FRATERNAL and multiple_kind(0.9999999, (0.3, 0.7)) == FRATERNAL


def wish_mean(shape: list, alpha: float) -> float:
    """The expected wish `Σ n · w_n / total`: the products `n · w_n` summed in order,
    divided by `total` once."""
    weights, total = wish_weights(shape, alpha)
    acc = 0.0
    for n, w in enumerate(weights):
        acc += float(n) * w
    return acc / total


def check_fertility_spec(form: FertilityForm, curves: dict, shape: list) -> None:
    """The values the genetics specification states, by hand: `s(1/2) = 3/4 − 2/8 = 1/2`,
    so 1.5 years after the menarche (before 30) the woman's factor is 0.5 [W5.6a]; with
    40 the ramp is over for `m <= 37` and the decline is `(50 − 40) / (50 − 30) = 0.5`
    [W5.6b]; with 57.5 the man's ramp is over and his decline is
    `1 − 0.6 · 12.5 / 25 = 0.7` [W5.6c]; at `α` = 0 the wish has the shape's mean
    `0.15 + 0.80 + 0.75 + 0.40 = 2.10` [W11.3a]."""
    assert form.kind == "puberty"
    assert smoothstep(0.5) == 0.5
    assert female_factor(12.5 + 1.5, 12.5, form) == 0.5
    for tenth in range(60, 280):
        m = tenth / 10.0
        assert abs(female_factor(m + 1.5, m, form) - 0.5) <= 1e-12, m
    for tenth in range(60, 371):
        assert female_factor(40.0, tenth / 10.0, form) == 0.5, tenth
    for curve in curves.values():
        for z_p in (-7.0, -6.0, -1.0, 0.0, 1.0, 6.0, 7.0):
            assert male_factor(57.5, theta_i(curve, z_p), form) == 0.7, (curve, z_p)
    assert abs(wish_mean(shape, 0.0) - 2.10) <= 1e-12


# The wish shape p_0 to p_4 of the genetics specification 11.3 [K11.3d].
WISH_SHAPE_SPEC = [0.10, 0.15, 0.40, 0.25, 0.10]
check_fertility_spec(MAP3.fertility, MAP3.puberty_curve, WISH_SHAPE_SPEC)


# ---------------------------------------------------------------------------------------
# Leaving home (design note of the households, section 1; ADR-0018 section 4): a pure
# function of the person's lineage key and the data, not state.


def leave_home_unit(seed: int, key: int) -> float:
    """`u = unit(draw)` at (LEAVE_HOME, the lineage key, occasion 0, detail 0)."""
    return unit(words(chacha_key(seed, LEAVE_HOME, key, 0, 0))[0])


def leave_home_thresholds(weights: list) -> list:
    """`(w_0 + … + w_n) / W` for n = 0 to 9: the partial sums left to right, each divided
    by `W`, the sum of the ten weights left to right."""
    total = 0.0
    for weight in weights:
        total = total + weight
    thresholds, acc = [], 0.0
    for weight in weights:
        acc = acc + weight
        thresholds.append(acc / total)
    return thresholds


def leave_home_age(u: float, weights: list) -> int:
    """The age of leaving home in whole years: `16 + n` for the smallest `n` with `u <
    (w_0 + … + w_n) / W`; 25 (`n` = 9) if no partial sum exceeds `u`."""
    for n, threshold in enumerate(leave_home_thresholds(weights)):
        if u < threshold:
            return 16 + n
    return 16 + 9


# Dario's table (2026-10-11): `W` is 1 exactly, and reading the shape like the wish of 11.3
# (`acc += w_n / W`) gives the same ten thresholds bit for bit.
assert leave_home_thresholds([0.02, 0.03, 0.30, 0.20, 0.15, 0.10, 0.08, 0.05, 0.04, 0.03]) == [
    0.02, 0.05, 0.35, 0.55, 0.7000000000000001, 0.8, 0.88, 0.93, 0.9700000000000001, 1.0
]
assert leave_home_age(0.0, [0.0] * 9 + [1.0]) == 25 and leave_home_age(0.5, [1.0] + [0.0] * 9) == 16
assert leave_home_age(0.35, [0.02, 0.03, 0.30, 0.20, 0.15, 0.10, 0.08, 0.05, 0.04, 0.03]) == 19


# ---------------------------------------------------------------------------------------
# The partner market (genetics specification 11.2; design note of the market, section 3):
# pure functions of the data, ages as `(t − born).as_years()`.


def search_chance(age_seconds: int, data: Data) -> float:
    """The chance that a single woman of `age_seconds` (at least the market age) searches in
    a round (step 2): `a = as_years(age)`, `z = (a − m) / w`, `s = c · exp(((−0.5) · z) · z)`
    with the correctly rounded exp, `s_f = s` if `s < 1` else 1, times `ρ` while she is
    under 18 (integer seconds)."""
    a = as_years(age_seconds)
    z = (a - data.market_center) / data.market_width
    s = data.market_height * exp_reference(((-0.5) * z) * z)[1]
    s_f = s if s < 1.0 else 1.0
    return data.market_minor * s_f if age_seconds < 18 * YEAR else s_f


def kinship_weight(data: Data) -> float:
    """`K = 16 · ln(cousin_odds)` with the correctly rounded ln (step 5)."""
    return 16.0 * ln_reference(data.cousin_odds)[1]


def penalties(gap: float, phi: float, data: Data, similarity=()) -> list:
    """The penalty terms of a candidate in the order of 11.2 (design note of the similarity,
    section 2): the similarity terms given (`w_K · (dK · dK)` if a trait of cognition is
    named, then `w_H · (dH · dH)` if one of height is: `similarity` is the list of the
    present (weight, difference) pairs in that order), then `w_a · (d · d)` with `d = Δa −
    μ_a`, then `K · φ`."""
    d = gap - data.pairing_gap
    terms = [weight * (difference * difference) for weight, difference in similarity]
    return terms + [data.pairing_weight_age * (d * d), kinship_weight(data) * phi]


def utility_of(terms: list, noise: float) -> float:
    """`U = (((−p1) − p2) − … − pn) + G` over the present terms, left to right."""
    acc = -terms[0]
    for term in terms[1:]:
        acc = acc - term
    return acc + noise


def utility(gap: float, phi: float, noise: float, data: Data, similarity=()) -> float:
    """The utility of a candidate (step 5; design note of the similarity, section 2). Without
    similarity terms `U = ((−(w_a · (d · d))) − (K · φ)) + G`, the market note exactly."""
    return utility_of(penalties(gap, phi, data, similarity), noise)


def ranked(seed: int, tag: int, subject: int, n: int, keys: list) -> list:
    """The keys ascending by `(rank(tag, subject, n, key), key)` (ADR-0001 section 5,
    `rank = absorb(RANK, 0, [seed, tag, subject, n, key])`)."""
    return sorted(keys, key=lambda key: (rank(seed, tag, subject, n, key), key))


def noise_of(seed: int, her_key: int, n: int, his_key: int) -> float:
    """`G = gumbel(w)` of the draw PAIRING_NOISE (her key, `n`, his key): `0 − ln(0 −
    ln(unit_open(w)))` with the correctly rounded ln (tools/rng_reference.py)."""
    return gumbel_reference(words(chacha_key(seed, PAIRING_NOISE, her_key, n, his_key))[0])[1]


# Spec 11.2 by hand: a cousin's penalty is ln(10) = 2.303 [W11.2a], so with the same rest a
# stranger is chosen ten times as often; at φ = 1/32 the odds are √10 = 3.162 [W11.2b].
assert abs(kinship_weight(MAP3) / 16.0 - math.log(10.0)) <= 1e-15
assert abs(utility(3.0, 0.0625, 0.0, MAP3) - utility(3.0, 0.0, 0.0, MAP3) + math.log(10.0)) <= 1e-14
assert abs(math.exp(kinship_weight(MAP3) / 32.0) - 3.1622776601683795) <= 1e-14
assert search_chance(18 * YEAR, life_test_data({"core:market_search_height_female": 0.0})) == 0.0


def budget_of(seed: int, key: int, code: str, detail: int = 0) -> float:
    """The budget `B` of the cause or disease with this code for this lineage key; `detail`
    is the arrival year of an arrival, 0 for everyone else (design note of the arrival,
    section 6.4)."""
    return exponential_reference(words(chacha_key(seed, DEATH_BUDGET, key, str_key(code), detail))[0], 1.0)[1]


def step_ulps(value: float, ulps: int) -> float:
    """`value` moved by whole ulps (the check that a result does not hang on the last bit
    of a libm function)."""
    for _ in range(abs(ulps)):
        value = math.nextafter(value, math.inf if ulps > 0 else -math.inf)
    return value


def senescence_age(x0: float, budget: float, a: float, b: float, m: float, nudge=(0, 0)) -> float:
    """`mortality::senescence_age` (genetics specification 7.1): the age at which the
    Gompertz term `m · a · e^(b·x)` acting from `x0` on uses up `budget`, computed as
    `x0 + ln(1 + b · B / (a · m) · e^(−b·x0)) / b` in this order. exp and ln are correctly
    rounded; `nudge` moves their results by whole ulps (exact results stay)."""
    assert x0 >= 0.0 and math.isfinite(x0) and within(budget, (0.0, MAX_BUDGET))
    assert within(a, GOMPERTZ_A_RANGE) and within(b, GOMPERTZ_B_RANGE) and within(m, MULTIPLIER_RANGE)
    kind, e = exp_reference(0.0 - b * x0)
    if kind == "P":
        e = step_ulps(e, nudge[0])
    ahead = b * budget / (a * m) * e
    kind, log = ln_reference(1.0 + ahead)
    if kind == "P":
        log = step_ulps(log, nudge[1])
    return x0 + log / b


def accident_age(x0: float, budget: float, per_year: float) -> float:
    """`mortality::accident_age` (7.1): `x0 + B / λ`."""
    assert x0 >= 0.0 and math.isfinite(x0) and within(budget, (0.0, MAX_BUDGET))
    assert within(per_year, ACCIDENT_RANGE)
    return x0 + budget / per_year


# The ages at death of crates/sim-core/tests/mortality.rs (W7.1, from Python's math there,
# independent of this model): the closed forms here meet them.
for origin, budget, (a, b), m, want in [
    (0.0, 1.0, (3.25e-6, 0.1182), 1.0, 88.845_227_436_585_18),
    (30.0, 0.5, (8.73e-6, 0.1118), 1.0, 78.434_825_532_710_79),
    (0.0, 2.0, (3.25e-6, 0.1182), exp_reference(-0.724 * 1.5)[1], 103.897_040_684_885_16),
    (64.0, 0.01, (8.73e-6, 0.1118), exp_reference(0.724 * 2.0)[1], 64.207_782_683_835_45),
]:
    assert abs(senescence_age(origin, budget, a, b, m) - want) <= 1e-13 * want, (origin, budget, want)
assert abs(accident_age(20.0, 0.7, 1.5e-4) - 4_686.666_666_666_667) <= 1e-13 * 4_686.666_666_666_667
assert senescence_age(40.0, 0.0, 8.73e-6, 0.1118, 1.0) == 40.0 and accident_age(40.0, 0.0, 2.0e-4) == 40.0


class Affliction:
    """A disease a sim has: the disease, the onset in years and, for a lethal one, the
    exposure [used, since]."""

    def __init__(self, disease: Disease, onset: float, since):
        self.disease = disease
        self.onset = onset
        self.exposure = None if disease.hazard is None else [0.0, since]

    def triple(self) -> tuple:
        return (self.disease.key, self.onset, None if self.exposure is None else tuple(self.exposure))


class Event:
    def __init__(self, time, cls, subject, kind, insertion, payload, period, plan, micro=0):
        self.time = time
        self.cls = cls
        self.subject = subject
        self.kind = kind
        self.insertion = insertion
        self.payload = payload
        self.period = period
        self.plan = plan
        # The micro step in (time, phase) (ADR-0002): 0 unless a handler planned it at its own
        # second and phase (then one more than its own); a periodic event's next occurrence is
        # a later second, micro 0.
        self.micro = micro

    def phase(self) -> int:
        return 1 if self.cls >= OBSERVER else 0

    def key(self):
        return (self.time, self.phase(), self.micro, self.cls, self.subject[0], self.subject[1], self.kind, self.insertion)

    def queue_bytes(self) -> bytes:
        """(time, micro, seq, payload, period), ADR-0002."""
        period = NONE if self.period is None else some(signed(self.period))
        return signed(self.time) + varint(self.micro) + varint(self.insertion) + self.payload + period


def death_payload(sim: int, code_key: int) -> bytes:
    return variant(2, varint(sim), varint(code_key))


def death_key(payload: bytes) -> int:
    """The key of the cause in the payload of a planned death: three varints, the variant
    2, the sim and the key."""
    numbers, value, shift = [], 0, 0
    for byte in payload:
        value |= (byte & 0x7F) << shift
        shift += 7
        if not byte & 0x80:
            numbers.append(value)
            value, shift = 0, 0
    assert shift == 0 and len(numbers) == 3 and numbers[0] == 2, payload.hex()
    return numbers[2]


def onset_second(born: int, onset: float) -> int:
    """`B = born + ⌈onset · YEAR⌉` (ADR-0017 section 3, addendum): the product as an f64, its
    ceiling, then an int; the second the card reads "affected since year …" from."""
    return born + math.ceil(onset * YEAR)


def onset_payload(sim: int, disease_key: int) -> bytes:
    """The queue payload `DiseaseOnset { sim, disease }` (variant 10): the SimId, then the
    `str_key` of the disease's code."""
    return variant(DISEASE_ONSET_VARIANT, varint(sim), varint(disease_key))


def onset_of_payload(payload: bytes) -> tuple:
    """(sim, disease key) of the payload of a planned `DiseaseOnset`: three varints, the
    variant 10, the sim and the key."""
    numbers, value, shift = [], 0, 0
    for byte in payload:
        value |= (byte & 0x7F) << shift
        shift += 7
        if not byte & 0x80:
            numbers.append(value)
            value, shift = 0, 0
    assert shift == 0 and len(numbers) == 3 and numbers[0] == DISEASE_ONSET_VARIANT, payload.hex()
    return numbers[1], numbers[2]


def onset_entry_payload(code: str, b: int, onset: float) -> bytes:
    """The log payload `core:disease_onset { disease, since_year, age_years }` (variant 14): the
    code as a string (as `core:death` names its cause), the calendar year of `B` (`⌊B / YEAR⌋`)
    and the onset in whole years (`⌊onset⌋`), both i64."""
    return variant(DISEASE_ONSET_ENTRY, string(code), signed(b // YEAR), signed(math.floor(onset)))


class World:
    """A started world. `switch` is the switch of the hereditary diseases at the start,
    `founders` a list of (born, sex, alleles) with the sex as postcard bytes and the
    alleles as for `founder_genome`, `couples` a list of (woman, man) by their places
    among the founders, `data` the definitions it runs on (the repository's by default,
    or test data). A sim is [born, lineage key, sex, genome (form of
    tools/meiosis_reference.py), afflictions, conceptions, births, parents, partnerships,
    household, moved_out, carer]; an archived one (born, left, key of the cause, lineage
    key, sex, genome, afflictions, conceptions, births, parents, partnerships, household,
    moved_out) (`partnerships` after `births` in the projection, then the household fields,
    then the parents); a partnership [woman, man, formed, ended] with `ended` None or (at,
    ENDED_BY_DEATH or ENDED_BY_SEPARATION); a household [formed, ended] with `ended` None or
    the time; a pregnancy, by the mother, (father, conception number, children as
    [(lineage key, sex, genome)] in index order, conceived). `households` False is a
    switch of this reference only (no household, no `leave_home`, no entry of them): the
    world of the similarity, for the check that the households change no story."""

    def __init__(self, seed: int, tag: str, switch: bool, founders: list, couples=(), data=None, market=True, households=True, capacity=None, pool=None, nutrition=()):
        self.seed = seed
        self.tag = tag
        self.switch = switch
        # The nutrition years of `WorldParams` (ADR-0013 section 1): (year, index) strictly
        # ascending by year, each index in [0, 1]; a year not named has `N` = 1. Only the body
        # mapping reads them (tools/census_reference.py), so they change no event of a run;
        # they are part of `Start` and of the projection. Empty by default (every older tool).
        self.nutrition = [(int(year), float(index)) for year, index in nutrition]
        assert all(a[0] < b[0] for a, b in zip(self.nutrition, self.nutrition[1:])), "nutrition_order"
        assert all(0.0 <= index <= 1.0 and -1_000_000 <= year <= 999_999 for year, index in self.nutrition), "nutrition_out_of_range"
        self.data = repository() if data is None else data
        # `market` False is a switch of this reference only, not of the core: no round is
        # planned (the null twin of the market, design note of the market, section 7).
        self.market = market
        # The soft capacity `K` of `Start` (design note of the emigration, section 1), or None.
        # An emigrating household needs households: the switch of this reference that turns
        # them off is for worlds without a capacity only.
        self.capacity = capacity
        assert households or capacity is None, "the emigration needs the households"
        # `pool` is the field `pool` of `Start` (design note of the arrival, section 3): None or
        # a PoolStart {"pool_hash", "n_start", "immigration", "households"} (the households as
        # the file holds them; `pool_start` makes the one the CLI fills in).
        assert households or pool is None, "the arrival needs the households"
        rejection = start_rejection(founders, couples, self.data, capacity, pool, seed)
        assert rejection is None, f"Start is rejected: {REJECT_NAMES[rejection]}"
        self.pool_start = pool
        # The village start mode (design note of the village, section 6): the persons arrive
        # one per household as `PoolStart` holds them; `village_records` holds each chosen
        # person's record as it arrived (`pool_member`), by lineage key (not state).
        self.village = pool is not None and pool.get("village", False)
        self.village_records = {}
        # The state of the pool persons (design note of the arrival, sections 6 and 9): per sim,
        # living or archived, its PoolOrigin (a dict with `genome_key`, `born_pool`, `arrived`,
        # `arrival_year`, `mother`, `father`, `social_parents`), none for a town person; per
        # running pregnancy of the pool its `pool_father` (the father's lineage key; the
        # pregnancy's `father` is then the SimId or None); the households of the pool used, as
        # indices into the file, in arrival order. All three are state (projected).
        self.origins = {}
        self.pool_fathers = {}
        self.used = []
        # The lineage keys of every sim, living or archived (an index, not state); the entry
        # `core:arrived` of each arrival, its carried times shifted (what the checks compare
        # with the pool's) and every arrival and immigration as a record, for the stories; not
        # state. `current` is the event whose handler runs (None in an input), for the micro
        # steps (ADR-0002).
        self.by_key = {}
        self.arrived_by = {}
        self.carried = {}
        self.arrivals = []
        self.immigrations = []
        self.current = None
        # The pedigree of every sim, living or archived, for the kinship (spec 2.7): not
        # state, it follows from the parents; a cache of its values, keyed by the pair.
        self.pedigree = Pedigree()
        self.kinships = {}
        # Every event that ran (time, kind), for the coverage of the goldens; every round
        # with at least one single woman in the market age (its record, see
        # `_market_round`); the census at each year end (`census`); not state.
        self.ran = []
        self.fired = []
        self.rounds = []
        self.census_hashes = []
        # The payload variant of every log entry, in log order (the coverage of the log
        # kinds); not state.
        self.entry_kinds = []
        # The accepted inputs ArrangePartnership as (the second they acted, woman, man), and
        # the couples of Start as SimIds: what a loader reads from the input log.
        self.input_pairings = [(0, woman + 1, man + 1) for woman, man in couples]
        # The couples of Start as SimIds (the loader reads them from the input log).
        self.start_couples = [(woman + 1, man + 1) for woman, man in couples]
        self.closed = -1
        self.queue = []
        self.next_insertion = 1
        self.next_event_id = 1
        self.next_sim_id = 1
        self.next_partnership_id = 1
        self.partnerships = {}
        self.pregnancies = {}
        self.sims = {}
        self.archive = {}
        self.log = []
        self.year_hashes = []
        # Every attempt (time, woman, man, chance, success) and every conception (time,
        # woman, man, conception number, the draw u of MULTIPLE_BIRTH, the kind, the
        # children as (key, sex)), for the stories of the tools; not state.
        self.attempts = []
        self.multiples = []
        # Every separation planned (partnership, k, the budget B, due or None) and every
        # one that came (time, partnership, its entry, the plan (by, planned_at), the key of
        # the draw), for the stories of the tools; not state.
        self.separation_plans = []
        self.separations = []
        # The households (design note of the households): the switch of this reference, the
        # counter, the households; every household entry as (time, kind, household, sims,
        # what caused it) and every change of a carer as (time, child, old, new, why), for
        # the stories of the tools; the creation entry of each sim (the plan of its
        # `leave_home`); not state.
        self.households_on = households
        # The arguments and every call that changes the world (advance, an accepted
        # ArrangePartnership, an override), so that `without_households` can run the same
        # world again with the switch off; not state.
        self.args = dict(seed=seed, tag=tag, switch=switch, founders=founders, couples=couples, data=data, market=market, capacity=capacity, pool=pool, nutrition=nutrition)
        self.script = []
        self.next_household_id = 1
        self.households = {}
        self.home_log = []
        self.carer_log = []
        # Per separation (time, woman, man, the dependent children who stay with her, those
        # who move with him), for the stories; not state.
        self.separation_homes = []
        self.separation_details = []
        # Every application of the orphan rule (part 2, section 3) as a record: the second,
        # what triggered it, the group, its representative, the candidates, the stage, the
        # carer, the social parents added per child, the household, the entries; not state.
        self.placements = []
        self.created_by = {}
        # The switch of the hereditary diseases as the input log holds it (design note of
        # the pedigree, section 1): (the second from which it holds, the value), the value
        # of `Start` from 0, each override block from one second after its stamp; what the
        # pedigree's `affected` reads. Not state (the save holds the input log).
        self.switch_history = [(0, switch)]
        # Every run of the event `Emigration` (design note of the emigration, sections 2 and
        # 3) as a record: the second, `P`, `K`, `e`, the draws per household and who left;
        # not state.
        self.emigrations = []
        # The onsets of diseases (ADR-0017 section 3, addendum, paths 1 to 3) as records: per
        # entry written or event fired `(second, sim, code, path, the entry's id or None, its
        # causes as postcard bytes)`, path 1 at the creation, 2 by the event (None: the event
        # fired with the switch off and wrote nothing), 3 by an override that set the switch
        # on; per event planned `(second planned, sim, code, B)` in `onset_plans`; per such
        # override in `fork_onsets` its second, its sequence number, the entries it wrote
        # (sim, code, id) and how many afflictions it passed over as reported before, with an
        # open event, or not reached. For the stories; not state.
        self.onsets = []
        self.onset_plans = []
        self.fork_onsets = []
        start = variant(
            0,
            varint(seed),
            string(tag),
            world_params(switch, self.nutrition),
            seq([founder(born, sex, genes) for born, sex, genes in founders]),
            seq([couple(woman, man) for woman, man in couples]),
            capacity_bytes(capacity),
            pool_start_bytes(pool),
        )
        self.inputs = [signed(-1) + varint(1) + start]
        # The draws of the founders' unnamed alleles (F1), per sim: (locus, gamete, q, u,
        # allele); for the stories, not state (the genome holds the alleles).
        self.founder_draws = {}
        self._start(founders)
        for woman, man in couples:
            self._partner(woman + 1, man + 1, [variant(2, varint(1))], 0, at_start=True)
        if self.households_on:
            self._house_founders(couples)
        if pool is not None:
            # The founders from the pool (design note of the arrival, sections 3 and 5): the
            # drawn households of the input, in rank order, at second 0 of year 0, caused by
            # the input; in the input's context every time is micro step 0.
            # The village (design note of the village, section 6): the single households of the
            # input, women then men in rank order, the same way; no household of the file is
            # used.
            if self.village:
                self._arrive([], 0, 0, [variant(2, varint(1))], "start", households=pool["households"])
            else:
                draw = pool_draw(self.data.pool, seed, 0, pool["n_start"], [])
                self._arrive(draw, 0, 0, [variant(2, varint(1))], "start")

    # ------------------------------------------------------------------ afflictions

    def afflictions_of(self, key: int, sex: bytes, person: dict, origin: int, parents, age_seconds: int, c_seed=None) -> list:
        """The afflictions of a sim at its creation, in data order (design note
        section 1; design note of the threshold disease, section 2). `parents` is (the
        mother's lineage key, the father's) or None for a founder, `age_seconds` its age at
        the creation (`0 − born` for a founder at `Start`, 0 for a child at its birth)."""
        found = []
        for disease in self.data.diseases:
            if disease.locus is not None:
                if not has_genotype(disease.locus["action"], copies_at(person, disease.locus)):
                    continue
                word = words(chacha_key(self.seed, DISEASE_PENETRANCE, key, disease.key, 0))[0]
                if not bernoulli(word, disease.penetrance[sex]):
                    continue
            elif disease.liability is not None:
                # The threshold disease (3.6): `L` the phenotype of its trait at the age of
                # the creation; the sim has it iff `L > T`, strictly; no penetrance draw.
                liability = phenotype(self.seed, disease.liability, person, key, parents, age_seconds, c_seed)
                if not liability > disease.threshold:
                    continue
            else:
                # A threshold carrier without a liability trait afflicts nobody (11.1).
                continue
            if disease.onset[0] == "fixed":
                onset = disease.onset[1]
            else:
                _, mean, sd, low, high = disease.onset
                draw = words(chacha_key(self.seed, DISEASE_ONSET, key, disease.key, 0))
                _, x = normal_reference(draw[0], draw[1], mean, sd)
                onset = min(max(x, low), high)
            found.append(Affliction(disease, onset, origin if self.switch else None))
        return found

    def creation_of(self, sim: int) -> tuple:
        """What the afflictions of a living or archived sim follow from (ADR-0017 section 3):
        (lineage key, sex, genome, the parents' lineage keys or None for a founder, the
        origin of its exposure, its age at the creation in seconds, the afflictions it
        holds). A founder is created at `Start` (second 0), a child at its birth."""
        if sim in self.sims:
            born, key, sex, person, afflictions = self.sims[sim][:5]
        else:
            born, _, _, key, sex, person, afflictions = self.archive[sim][:7]
        parents = self.parent_keys(sim)
        origin = 0 if parents is None else born
        return key, sex, person, parents, origin, origin - born, afflictions

    # ------------------------------------------------------------------ pool persons

    def is_pool_person(self, sim: int) -> bool:
        """An arrival or a pool child, living or archived (design note of the arrival,
        section 1). A world started without a pool has none: a PoolOrigin there is only for
        `check_pool` to reject; every other rule reads the sim as a town person (review of
        the reference, NOTES2.md section 5b; no valid state of such a world has one)."""
        return sim in self.origins and self.pool_start is not None

    def is_arrival(self, sim: int) -> bool:
        return self.is_pool_person(sim) and self.origins[sim]["born_pool"] is not None

    def origin_of(self, sim: int) -> int:
        """Where the hazard integral of its deaths starts (design note of the arrival,
        section 6.4): the arrival second `A` for an arrival, else `max(born, START)`."""
        if self.is_arrival(sim):
            return self.origins[sim]["arrived"]
        return max(self.born_of(sim), 0)

    def death_detail(self, sim: int) -> int:
        """The detail of its DEATH_BUDGET draws: the arrival year of an arrival, 0 else."""
        return self.origins[sim]["arrival_year"] if self.is_arrival(sim) else 0

    def own_seed(self, sim: int) -> int:
        """The seed of its wish and menarche offset (design note of the arrival, section 6.2):
        the pool seed for a pool person, the world seed else."""
        return self.data.pool.seed if self.is_pool_person(sim) else self.seed

    def c_seed_of_child(self, mother: int, father: int) -> int:
        """The seed of `C` of a town person born of these two (section 6.2): the pool seed if
        both are pool persons, else the world seed."""
        if self.is_pool_person(mother) and self.is_pool_person(father):
            return self.data.pool.seed
        return self.seed

    def env_of(self, sim: int) -> tuple:
        """(seed of `E`, seed of `C`, the parents' keys `C` reads or None) of a living or
        archived sim (design note of the arrival, section 6.2): a pool person draws both with
        the pool seed at its PoolOrigin keys (its own for both only if both are None); a town
        person of two pool persons `E` with the world seed and `C` with the pool seed at their
        keys; anyone else both with the world seed at its parents' keys (its own without)."""
        if self.is_pool_person(sim):
            origin = self.origins[sim]
            parents = (origin["mother"], origin["father"])
            assert (parents[0] is None) == (parents[1] is None), "both PoolOrigin parents or none"
            return self.data.pool.seed, self.data.pool.seed, None if parents[0] is None else parents
        parents = self.parent_keys(sim)
        if parents is None:
            return self.seed, self.seed, None
        mother, father = (self.sims[sim][7] if sim in self.sims else self.archive[sim][9])[:2]
        return self.seed, self.c_seed_of_child(mother, father), parents

    def pool_afflictions(self, items: list, origin: int) -> list:
        """The afflictions of a pool person (the pool's `(disease, onset)` in data order) as
        the town holds them: a lethal one with its exposure from `origin` if the switch is on
        (design note of the arrival, sections 6.1 and 6.3)."""
        by_key = {disease.key: disease for disease in self.data.diseases}
        order = [disease.key for disease in self.data.diseases]
        keys = [key for key, _ in items]
        assert all(key in by_key for key in keys) and keys == sorted(keys, key=order.index), "the pool's afflictions in data order"
        return [Affliction(by_key[key], onset, origin if self.switch else None) for key, onset in items]

    def biological_keys(self, sim: int) -> tuple:
        """The lineage keys of its biological parents (each None if unknown): its PoolOrigin
        keys for a pool person, its parents' keys for a town person."""
        if self.is_pool_person(sim):
            return self.origins[sim]["mother"], self.origins[sim]["father"]
        parents = self.parent_keys(sim)
        return (None, None) if parents is None else parents

    def is_biological_parent(self, child: int, person: int) -> bool:
        """Whether `person` is a biological parent of `child`: by SimId for a town person, by
        lineage key for a pool person (decision of NOTES.md: the orphan rule's "unless its
        biological parent" and the load check of the social parents read it so)."""
        if self.is_pool_person(child):
            return self.key_of(person) in [key for key in self.biological_keys(child) if key is not None]
        parents = self.sims[child][7] if child in self.sims else self.archive[child][9]
        return parents is not None and person in parents[:2]

    def pool_member(self, sim: int) -> dict:
        """The pool's record of an arrival (its member entry in the file); in the village its
        record as it arrived (`village_person`: no carried plan, moved out)."""
        if self.village:
            return self.village_records[self.key_of(sim)]
        return self.data.pool.members[self.key_of(sim)][1]

    def carried_leave_home(self, sim: int):
        """The carried `leave_home` of an arrival: the pool's `leave_home_at + Δ`, or None."""
        member = self.pool_member(sim)
        if member["leave_home_at"] is None:
            return None
        return member["leave_home_at"] + self.origins[sim]["arrived"] - self.data.pool.frozen_at

    def pool_partnerships_before(self, sim: int) -> int:
        """An arrival's partnerships of the pool that did not come along: the pool's count less
        one if her household brought her open partnership."""
        member = self.pool_member(sim)
        if self.village:
            # No partnership comes along to the village: the pool's count is the one before.
            return member["partnerships"]
        index = self.data.pool.members[self.key_of(sim)][0]
        partnership = self.data.pool.households[index]["partnership"]
        brought = partnership is not None and partnership[0] == member["lineage_key"]
        return member["partnerships"] - (1 if brought else 0)

    def _immigration(self, now: int, plan: bytes, q: int) -> None:
        """The event `Immigration` at the first second of year `y` (design note of the
        arrival, section 4): the unused households in the order of `POOL_RANK` with occasion
        `y`, taken until their members reach or exceed `q`; if all of them cannot reach `q`,
        nobody arrives and the run stops (`pool.empty`)."""
        pool = self.data.pool
        year = now // YEAR
        free = [index for index in range(len(pool.households)) if index not in self.used]
        record = {"t": now, "year": year, "q": q, "free": len(free), "free_members": sum(pool.sizes[i] for i in free)}
        self.immigrations.append(record)
        if record["free_members"] < q:
            record["empty"] = True
            raise Invariant("pool.empty", f"year {year} needs {q}, {record['free_members']} left")
        draw = pool_draw(pool, self.seed, year, q, self.used)
        record["draw"] = draw
        self._arrive(draw, now, year, [plan], "immigration")

    def _arrive(self, draw: list, arrival: int, year: int, causes: list, why: str, households=None) -> None:
        """The arrival of the drawn households at `arrival` (`A`) of year `year`, step by step
        (design note of the arrival, section 5), per household in rank order; `causes` the
        input of `Start` or the plan of the event. With `households` (the village, design note
        of the village, section 6) those households instead, in their order, none of the file
        used (`draw` empty). Returns nothing; each household's record goes to `arrivals`."""
        pool = self.data.pool
        delta = arrival - pool.frozen_at
        rule = variant(3, string(HOUSEHOLD_RULE))
        if households is None:
            arriving = [(index, pool.households[index]) for index in draw]
        else:
            assert not draw and self.village
            arriving = [(None, household) for household in households]
            for household in households:
                for member in household["members"]:
                    assert member["lineage_key"] not in self.village_records
                    self.village_records[member["lineage_key"]] = member
        for index, household in arriving:
            if index is not None:
                assert index not in self.used
                self.used.append(index)
            members = household["members"]
            record = {"t": arrival, "year": year, "why": why, "index": index, "delta": delta, "sims": [], "entries": {}}
            before = self.next_event_id
            # 1. The members in the household's order: a SimId, `core:arrived`, the state.
            sims = []
            for member in members:
                sim = self.next_sim_id
                self.next_sim_id += 1
                key = member["lineage_key"]
                assert key not in self.by_key, "lineage_key_unique"
                entry = self._entry(arrival, [variant(0, varint(sim))], causes, variant(ARRIVED_VARIANT, varint(key)))
                sex = FEMALE if member["sex"] == 0 else MALE
                person = genome_from_raw(member["genome"])
                afflictions = self.pool_afflictions(member["afflictions"], arrival)
                self.sims[sim] = [
                    member["born"] + delta, key, sex, person, afflictions, member["conceptions"], member["births"], None,
                    member["partnerships"], None, member["moved_out"], None, [],
                ]
                self.origins[sim] = {
                    "genome_key": member["genome_key"],
                    "born_pool": member["born"],
                    "arrived": arrival,
                    "arrival_year": year,
                    "mother": member["mother"],
                    "father": member["father"],
                    "social_parents": list(member["social_parents"]),
                }
                self.arrived_by[sim] = entry
                self.pedigree.add(sim)
                self.by_key[key] = sim
                sims.append(sim)
                # Its onsets right after its `core:arrived` (paths 1 and 2), before the next
                # member's `core:arrived` and before any other entry or plan of it.
                self._onset_step(sim, entry, arrival)
            mine = {member["lineage_key"]: sim for member, sim in zip(members, sims)}
            for member, sim in zip(members, sims):
                if member["carer"] is not None:
                    self.sims[sim][11] = mine[member["carer"]]
            record["sims"] = sims
            # 2. The household: `core:household_formed` with the members ascending, no move.
            number_ = self.next_household_id
            self.next_household_id += 1
            self.households[number_] = [arrival, None]
            formed = self._home_entry(arrival, sorted(sims), causes + [rule], HOUSEHOLD_FORMED_VARIANT, number_)
            self.home_log.append((arrival, "formed", number_, sorted(sims), f"arrival ({why})", formed))
            for sim in sims:
                self.sims[sim][9] = number_
            record["household"] = number_
            # 3. The open partnership: `core:partnered` caused by her `core:arrived`, formed at
            # `formed_pool + Δ`, nothing else.
            partnership_number = None
            if household["partnership"] is not None:
                woman_key, man_key, formed_pool = household["partnership"]
                woman, man = mine[woman_key], mine[man_key]
                assert self.sims[woman][2] == FEMALE and self.sims[man][2] == MALE
                partnership_number = self.next_partnership_id
                self.next_partnership_id += 1
                subjects = [variant(0, varint(woman)), variant(0, varint(man))]
                self._entry(arrival, subjects, [variant(0, varint(self.arrived_by[woman]))], variant(5, varint(partnership_number)))
                self.partnerships[partnership_number] = [woman, man, formed_pool + delta, None]
                record["partnership"] = partnership_number
            # 4. The pregnancies in member order: the father's SimId if he has arrived (living
            # or archived), else None; `pool_father` his key.
            for pregnancy in household["pregnancies"]:
                mother = mine[pregnancy["mother"]]
                assert mother not in self.pregnancies
                father = self.by_key.get(pregnancy["father"])
                children = [(child["lineage_key"], FEMALE if child["sex"] == 0 else MALE, genome_from_raw(child["genome"])) for child in pregnancy["children"]]
                for child in pregnancy["children"]:
                    assert child["lineage_key"] not in self.by_key, "lineage_key_unique"
                self.pregnancies[mother] = (father, pregnancy["conception"], children, pregnancy["conceived"] + delta)
                self.pool_fathers[mother] = pregnancy["father"]
            # 5. Per member, by its `core:arrived`: the carried plans, the next birthday, the
            # deaths.
            for member, sim in zip(members, sims):
                by = self.arrived_by[sim]
                carried = {}
                born = self.sims[sim][0]
                for name in ("conception_at", "birth_at", "separation_at", "leave_home_at"):
                    if member[name] is None:
                        continue
                    t = member[name] + delta
                    if t < arrival:
                        raise Invariant("pool.carried_before_arrival", f"{name} of {member['lineage_key']:016x} at {t}")
                    carried[name] = t
                    if name == "conception_at":
                        assert self.sims[sim][2] == FEMALE and self.partner_of(sim) is not None
                        self._schedule(t, GENERAL, (SIM_SUBJECT, sim), ATTEMPT_KIND, variant(3, varint(sim)), MONTH, by, arrival)
                    elif name == "birth_at":
                        assert sim in self.pregnancies
                        self._schedule(t, BIRTH, (SIM_SUBJECT, sim), BIRTH_KIND, variant(4, varint(sim)), None, by, arrival)
                    elif name == "separation_at":
                        assert partnership_number is not None and self.partnerships[partnership_number][0] == sim
                        self.separation_plans.append((partnership_number, None, None, t))
                        payload = variant(5, varint(partnership_number))
                        self._schedule(t, GENERAL, (PARTNERSHIP_SUBJECT, partnership_number), SEPARATION_KIND, payload, None, by, arrival)
                    else:
                        assert not self.sims[sim][10]
                        self._schedule(t, GENERAL, (SIM_SUBJECT, sim), LEAVE_HOME_KIND, variant(LEAVE_HOME_VARIANT, varint(sim)), None, by, arrival)
                self.carried[sim] = carried
                self._plan_birthday(sim, first_birthday(born, arrival), by, arrival)
                self._plan_deaths(sim, by, arrival)
            record["entries"] = list(range(before, self.next_event_id))
            self.arrivals.append(record)

    def liability_of(self, sim: int, disease: Disease) -> float:
        """`L` of a living or archived sim for a threshold disease with a trait of
        liability (design note of the threshold disease, section 2): the phenotype of the
        trait at the age of its creation (the weights do not depend on the age)."""
        key, _, person, _, _, age, _ = self.creation_of(sim)
        e_seed, c_seed, parents = self.env_of(sim)
        return phenotype(e_seed, disease.liability, person, key, parents, age, c_seed)

    def disease_due(self, sim: int, affliction: Affliction):
        """When the sim dies of the lethal affliction if nothing else happens first, or
        None (design note section 4)."""
        born, key = self.sims[sim][0], self.sims[sim][1]
        used, since = affliction.exposure
        if since is None:
            return None
        at_since = as_years(since - born)
        x0 = max(at_since, affliction.onset)
        remaining = max(budget_of(self.seed, key, affliction.disease.code, self.death_detail(sim)) - used, 0.0)
        # A budget the closed periods used up kills in the second the switch goes on
        # again, whatever hazard the age has then (spec 7.1; review of the design).
        if used > 0.0 and remaining <= 0.0:
            return since if abs(since) <= MAX_ABS_SECS else None
        age = disease_age(x0, remaining, affliction.disease.hazard)
        if age is None:
            return None
        seconds = from_years_ceil(age - at_since)
        if seconds is None:
            return None
        due = since + seconds
        return due if abs(due) <= MAX_ABS_SECS else None

    def cause_seconds(self, sim: int, code: str, nudge=(0, 0, 0)):
        """How many seconds after its origin the sim dies of senescence or accident if
        nothing else happens first, or None (never). `nudge` moves the budget, exp and ln
        by whole ulps, and with a fourth number the exp of `m` (a trait of longevity)."""
        born, key, sex, person = self.sims[sim][:4]
        origin = self.origin_of(sim)
        x0 = as_years(origin - born)
        budget = step_ulps(budget_of(self.seed, key, code, self.death_detail(sim)), nudge[0])
        if code == SENESCENCE:
            a, b = self.data.senescence[sex]
            m = self.data.multiplier(person, nudge[3] if len(nudge) > 3 else 0)
            x = senescence_age(x0, budget, a, b, m, nudge[1:3])
        else:
            assert code == ACCIDENT
            x = accident_age(x0, budget, self.data.accident[sex])
        return from_years_ceil(x - x0)

    def cause_due(self, sim: int, code: str):
        """When the sim dies of `core:senescence` or `core:accident` if nothing else
        happens first, or None: from its origin second (a founder from the start, a child
        from its birth), never beyond the clock."""
        # With a trait of longevity the exp of `m` is a fourth last bit to check.
        dims = 4 if code == SENESCENCE and self.data.longevity is not None else 3
        seconds = robust_seconds(lambda nudge: self.cause_seconds(sim, code, nudge), dims)
        if seconds is None:
            return None
        due = self.origin_of(sim) + seconds
        return due if abs(due) <= MAX_ABS_SECS else None

    def _plan_deaths(self, sim: int, by: int, now: int) -> None:
        for code in (SENESCENCE, ACCIDENT):
            due = self.cause_due(sim, code)
            if due is not None:
                payload = death_payload(sim, str_key(code))
                self._schedule(due, DEATH, (SIM_SUBJECT, sim), DEATH_KIND, payload, None, by, now)
        self._plan_disease_deaths(sim, by, now)

    def _plan_disease_deaths(self, sim: int, by: int, now: int) -> None:
        for affliction in self.sims[sim][4]:
            if affliction.exposure is None:
                continue
            due = self.disease_due(sim, affliction)
            if due is None:
                continue
            payload = death_payload(sim, affliction.disease.key)
            self._schedule(due, DEATH, (SIM_SUBJECT, sim), DEATH_KIND, payload, None, by, now)

    # ------------------------------------------------------------------ onsets

    def creation_second(self, sim: int) -> int:
        """`E` of a living or archived sim (ADR-0017 section 3, addendum): a founder 0 (the
        start), an arrival its arrival second (0 at a pool start), a child born in the town (a
        pool child too) its birth second; `origin_of` gives exactly these."""
        return self.origin_of(sim)

    def onset_second_of(self, sim: int, affliction: Affliction) -> int:
        """`B` of an affliction of a living or archived sim."""
        return onset_second(self.born_of(sim), affliction.onset)

    def made_by_start(self, sim: int) -> bool:
        """Whether `Start` created a living or archived sim: a founder, a person of a village,
        or a member of a household of the start's draw (occasion 0 and `n_start`); everyone
        else came by an immigration (also the one in second 0, after the inputs stamped
        before it), a birth or not at all. What a loader derives from the input log."""
        if not self.is_pool_person(sim):
            return self.born_of(sim) <= 0
        if not self.is_arrival(sim) or self.creation_second(sim) != 0:
            return False
        if self.village:
            return True
        index, _ = self.data.pool.members[self.key_of(sim)]
        return index in pool_draw(self.data.pool, self.seed, 0, self.pool_start["n_start"], [])

    def creation_onset_entries(self, sim: int) -> int:
        """How many entries of path 1 the creation of a living or archived sim wrote: its
        afflictions with `B <= E` if the switch its creation saw was on, else none: `Start`'s
        own value for what `Start` made (an override stamped before the start holds only
        after it, ADR-0017 section 3, addendum), else the switch in effect at `E` after the
        inputs of that second (the input log's history, `switch_at`). What a loader can count
        without the domain log."""
        e = self.creation_second(sim)
        afflictions = self.sims[sim][4] if sim in self.sims else self.archive[sim][6]
        seen = self.switch_history[0][1] if self.made_by_start(sim) else self.switch_at(max(e, 0))
        if not seen:
            return 0
        return sum(1 for affliction in afflictions if self.onset_second_of(sim, affliction) <= e)

    def _onset_step(self, sim: int, created: int, now: int) -> None:
        """The onset step of a sim (ADR-0017 section 3, addendum, paths 1 and 2), right after
        its creation entry `created` at `now` (`E`), before any other entry or plan of it: per
        affliction in data order with `B = born + ⌈onset · YEAR⌉`, `B <= E` writes the entry now
        if the switch (the one in effect now: the start's own value at the start, the switch
        then at a birth or an arrival) is on (path 1, caused by the creation entry); `B > E`
        plans the event `DiseaseOnset { sim, disease }` at `B` whatever the switch (path 2, by
        the creation entry, planned now, micro 0 since `B` is a later second; a `B` beyond the
        clock is never, as a death beyond it, ADR-0002). Path 3 (a ForkOverride that turns the
        switch on) is `_fork_onsets`."""
        assert now == self.creation_second(sim), (sim, now)
        for affliction in self.sims[sim][4]:
            b = self.onset_second_of(sim, affliction)
            if b <= now:
                if self.switch:
                    self._onset_entry(sim, affliction, now, [variant(0, varint(created))], 1)
                continue
            if abs(b) > MAX_ABS_SECS:
                continue
            self.onset_plans.append((now, sim, affliction.disease.code, b))
            self._schedule(b, GENERAL, (SIM_SUBJECT, sim), DISEASE_ONSET_KIND, onset_payload(sim, affliction.disease.key), None, created, now)

    def _onset_entry(self, sim: int, affliction: Affliction, now: int, causes: list, path: int) -> int:
        """The entry `core:disease_onset` of `sim` (subjects the sim) with `causes`. A check of
        this reference on its records (not a rule the paths read): one entry per sim and
        disease, never a second ("danach nie wieder")."""
        code = affliction.disease.code
        assert not [r for r in self.onsets if r[1] == sim and r[2] == code and r[4] is not None], (sim, code, now, path)
        b = self.onset_second_of(sim, affliction)
        entry = self._entry(now, [variant(0, varint(sim))], causes, onset_entry_payload(affliction.disease.code, b, affliction.onset))
        self.onsets.append((now, sim, affliction.disease.code, path, entry, causes))
        return entry

    def _disease_onset(self, event: Event, now: int, plan: bytes) -> None:
        """The event `DiseaseOnset` at `B` (path 2): the entry if the switch is on now (after
        the inputs of this second), caused by the plan; otherwise nothing."""
        sim, key = onset_of_payload(event.payload)
        assert event.subject == (SIM_SUBJECT, sim) and sim in self.sims, (event.subject, sim)
        (affliction,) = [affliction for affliction in self.sims[sim][4] if affliction.disease.key == key]
        assert self.onset_second_of(sim, affliction) == now, (sim, key, now)
        if self.switch:
            self._onset_entry(sim, affliction, now, [plan], 2)
        else:
            self.onsets.append((now, sim, affliction.disease.code, 2, None, [plan]))

    def onset_reported(self, sim: int, affliction: Affliction, history: list) -> bool:
        """Whether one of the reporting moments of an affliction of a sim of the active
        population had the switch on, before an input (ADR-0017 section 3, addendum, path 3):
        `history` is the switch history of the input log up to that input (`switch_history`
        without it: `Start`'s value from 0, then each override block from one second after its
        stamp, in sequence). The moments, with `B` the onset second and `E` the creation second:
        - the creation, if `B <= E`, with the switch it saw: `Start`'s own value for what
          `Start` made (`made_by_start`; an override stamped before the start acts only after
          it), otherwise the switch in effect at `E` after the inputs of `E`;
        - its own event at `B`, if `B > E`, with the switch in effect at `B` after the inputs
          of `B` (the event runs after them);
        - every override of `history` that set the switch on while the onset was reached and
          no own event was open: one acting after `E` if `B <= E` (or in `E` itself for what
          `Start` made, after `Start`), after `B` if `B > E`; inputs of one second count in
          their sequence.
        The caller asks only for an onset reached and without an open event, so the sim was in
        the active population at every one of these moments. Reads no log and no new state:
        `B`, `E`, the input log."""
        e = self.creation_second(sim)
        b = self.onset_second_of(sim, affliction)

        def at(r: int) -> bool:
            return [value for since, value in history if since <= r][-1]

        if b <= e and self.made_by_start(sim):
            seen, later = history[0][1], [value for since, value in history[1:] if since >= e]
        elif b <= e:
            seen, later = at(e), [value for since, value in history[1:] if since > e]
        else:
            seen, later = at(b), [value for since, value in history[1:] if since > b]
        return seen or any(later)

    def _fork_onsets(self, now: int, sequence: int) -> None:
        """Path 3 (ADR-0017 section 3, addendum, "Drei Wege" (3); Grok P1 and Q1): an override
        block whose change sets the switch on, acting in `now` (`T`, its stamp plus one) with
        the sequence number `sequence`, in its own input context after its `core:param_changed`
        and everything else it does, writes for every sim of the active population
        (`World.sims`) by SimId, and for each of its afflictions in data order with `B <= T` and
        no own open event (open: `B > E` and `B >= T`; an onset exactly in `T` with `B > E`
        keeps its event, which runs after the input and writes it by path 2), the entry
        `core:disease_onset` if none of its earlier reporting moments had the switch on
        (`onset_reported`, on the history before this input). The entry is the one of paths 1
        and 2, subjects the sim, caused by the input. No state, no event, no plan."""
        history = self.switch_history[:-1]
        assert self.switch_history[-1] == (now, True), self.switch_history[-1]
        cause = [variant(2, varint(sequence))]
        record = {"second": now, "input": sequence, "written": [], "reported": 0, "open": 0, "ahead": 0}
        for sim in sorted(self.sims):
            e = self.creation_second(sim)
            for affliction in self.sims[sim][4]:
                b = self.onset_second_of(sim, affliction)
                if b > now:
                    record["ahead"] += 1
                    continue
                if b > e and b >= now:
                    # Its own event at `B = T` is still in the queue (a check of this reference).
                    assert [event for event in self.queue if event.kind == DISEASE_ONSET_KIND and event.payload == onset_payload(sim, affliction.disease.key) and event.time == b]
                    record["open"] += 1
                    continue
                if self.onset_reported(sim, affliction, history):
                    record["reported"] += 1
                    continue
                entry = self._onset_entry(sim, affliction, now, cause, 3)
                record["written"].append((sim, affliction.disease.code, entry))
        self.fork_onsets.append(record)

    # ------------------------------------------------------------------ inputs

    def _partner(self, woman: int, man: int, causes: list, now: int, at_start: bool = False) -> int:
        """A partnership of the woman and the man at `now`, its entry `core:partnered` with
        `causes` (the input; or the plan of the round, the rule and the draw). Any pairing
        but a couple of `Start` (`at_start`, housed by `_house_founders`) then founds the
        couple's household (design note of the households, section 4)."""
        assert self.sims[woman][2] == FEMALE and self.sims[man][2] == MALE
        assert self.partner_of(woman) is None and self.open_partnership(man) is None
        # Partnerships never begin below the market age, never between close kin (design
        # note of the market, section 4; ADR-0018 section 5).
        assert all(now - self.sims[sim][0] >= self.data.market_age * YEAR for sim in (woman, man))
        assert self.kinship(woman, man) < Fraction(self.data.kinship_limit)
        number_ = self.next_partnership_id
        self.next_partnership_id += 1
        subjects = [variant(0, varint(woman)), variant(0, varint(man))]
        entry = self._entry(now, subjects, causes, variant(5, varint(number_)))
        self.partnerships[number_] = [woman, man, now, None]
        # Her counter (design note of the separation, section 3): `k` before, then one
        # more.
        k = self.sims[woman][8]
        self.sims[woman][8] += 1
        # The series after a partnership forms (design note of the fertility in the world,
        # section 4), then the separation, after her attempts.
        self._plan_series(woman, month_start_after(now), entry, now)
        self._plan_separation(number_, k, entry, now)
        if self.households_on and not at_start:
            self._house_couple(woman, man, entry, now)
        return entry

    # ------------------------------------------------------------------ households

    def dependent(self, sim: int, now: int) -> bool:
        """Living, under 18 (`now − born < 18 · YEAR`) and not moved out (design note of the
        households, section 2)."""
        if sim not in self.sims:
            return False
        her = self.sims[sim]
        return now - her[0] < 18 * YEAR and not her[10]

    def fit_to_care(self, sim: int, now: int) -> bool:
        """Living and not dependent."""
        return sim in self.sims and not self.dependent(sim, now)

    def members(self, household: int) -> list:
        """The living members of a household, by SimId."""
        return [sim for sim in sorted(self.sims) if self.sims[sim][9] == household]

    def leave_home_second(self, sim: int) -> int:
        """`born + age · YEAR` with the drawn age of leaving (design note of the households,
        section 1), for a living or archived sim."""
        age = leave_home_age(leave_home_unit(self.seed, self.key_of(sim)), self.data.leave_home_weights)
        return self.born_of(sim) + age * YEAR

    def _home_entry(self, now: int, subjects: list, causes: list, variant_: int, household: int) -> int:
        return self._entry(now, [variant(0, varint(sim)) for sim in subjects], causes, variant(variant_, varint(household)))

    def _found(self, movers: list, now: int, causes: list, why: str, moves: bool = True) -> int:
        """A new household at `now` with `movers`: its entry `core:household_formed`
        (subjects the members it starts with, by SimId), then, if `moves` (always but at the
        start), one `core:moved` per mover in the order given, each leaving its household
        (which ends if it was the last member there). Both kinds of entry have `causes`."""
        number_ = self.next_household_id
        self.next_household_id += 1
        self.households[number_] = [now, None]
        formed = self._home_entry(now, sorted(movers), causes, HOUSEHOLD_FORMED_VARIANT, number_)
        self.home_log.append((now, "formed", number_, sorted(movers), why, formed))
        for sim in movers:
            old = self.sims[sim][9]
            self.sims[sim][9] = number_
            if moves:
                moved = self._home_entry(now, [sim], causes, MOVED_VARIANT, number_)
                self.home_log.append((now, "moved", number_, [sim], why, moved))
            if old is not None:
                self._end_if_empty(old, now)
        return number_

    def _end_if_empty(self, household: int, now: int) -> None:
        """A household ends in the second its last member leaves it (death or move)."""
        if not self.members(household):
            assert self.households[household][1] is None
            self.households[household][1] = now
            self.home_log.append((now, "ended", household, [], "", None))

    def _set_carer(self, child: int, carer, now: int, why: str) -> None:
        """The carer of the child becomes `carer` (no entry); a change is logged for the
        stories."""
        old = self.sims[child][11]
        if old == carer:
            return
        self.sims[child][11] = carer
        self.carer_log.append((now, child, old, carer, why))

    def _join(self, child: int, household: int, now: int, causes: list, why: str) -> int:
        """A move into an existing household: one `core:moved` (subjects the one person,
        `causes`); the person leaves its household (none for a founder at the start), which
        ends if it was the last member there."""
        old = self.sims[child][9]
        assert old != household
        self.sims[child][9] = household
        moved = self._home_entry(now, [child], causes, MOVED_VARIANT, household)
        self.home_log.append((now, "moved", household, [child], why, moved))
        if old is not None:
            self._end_if_empty(old, now)
        return moved

    def _plan_leave_home(self, sim: int, by: int, now: int) -> None:
        """Its `leave_home` at the drawn second, planned by the entry `by`."""
        payload = variant(LEAVE_HOME_VARIANT, varint(sim))
        self._schedule(self.leave_home_second(sim), GENERAL, (SIM_SUBJECT, sim), LEAVE_HOME_KIND, payload, None, by, now)

    def _cancel_leave_home(self, sim: int) -> None:
        self.queue = [event for event in self.queue if not (event.kind == LEAVE_HOME_KIND and event.subject == (SIM_SUBJECT, sim))]

    def _house_founders(self, couples) -> None:
        """`Start`, after the founders and the couples (design note of the households,
        section 4; part 2, section 5): each couple of `couples` in list order founds a
        household with its two (both moved out); then each other founder of 18 or more in
        founder order one of his own (moved out); then each other founder (under 18) in
        founder order: a household of his own, moved out, if its drawn second of leaving is
        not after the start; else a group of one for the orphan rule (`_place`; founders
        are unrelated, so a foster household if a woman of a couple is 18 or more, which he
        joins with one `core:moved`, else a household of his own, not moved out, no carer,
        with its `core:household_formed` and no `core:moved`, as in part 1), with its
        `leave_home` planned by its creation entry after its housing. The entries of the
        start name the input and the rule."""
        now = 0
        causes = [variant(2, varint(1)), variant(3, string(HOUSEHOLD_RULE))]
        paired = []
        for woman, man in couples:
            pair = [woman + 1, man + 1]
            self._found(pair, now, causes, "start couple", moves=False)
            paired += pair
        founders = sorted(self.created_by)
        others = [sim for sim in founders if sim not in paired]
        adults = [sim for sim in others if now - self.sims[sim][0] >= 18 * YEAR]
        minors = [sim for sim in others if now - self.sims[sim][0] < 18 * YEAR]
        for sim in paired + adults:
            self.sims[sim][10] = True
        for sim in adults:
            self._found([sim], now, causes, "start adult", moves=False)
        for sim in minors:
            if self.leave_home_second(sim) <= now:
                self.sims[sim][10] = True
                self._found([sim], now, causes, "start minor past its age of leaving", moves=False)
            else:
                self._place([sim], now, causes, "start", at_start=True)
                self._plan_leave_home(sim, self.created_by[sim], now)

    def _house_couple(self, woman: int, man: int, entry: int, now: int) -> None:
        """A partnership (not of `Start`): the couple founds a new household; she moves,
        then he, then each dependent child either of them cares for (by SimId); both are
        moved out from now on (their `leave_home` cancelled, no carer)."""
        children = sorted(
            sim for sim in self.sims if sim not in (woman, man) and self.sims[sim][11] in (woman, man) and self.dependent(sim, now)
        )
        causes = [variant(0, varint(entry)), variant(3, string(HOUSEHOLD_RULE))]
        for sim in (woman, man):
            if self.sims[sim][11] is not None:
                self._set_carer(sim, None, now, "moved out by a partnership")
            self.sims[sim][10] = True
            self._cancel_leave_home(sim)
        self._found([woman, man] + children, now, causes, f"partnership {self.next_partnership_id - 1}")

    def _house_separation(self, woman: int, man: int, entry: int, now: int) -> None:
        """A separation: the man founds a new household and moves, each dependent child in
        the couple's household whose carer is he moves with him (by SimId); every dependent
        child there whose carer is neither stays and the woman becomes its carer (part 2,
        section 5: kept literally, whoever the carer is)."""
        home = self.sims[man][9]
        assert self.sims[woman][9] == home, "a couple lives together"
        dependent = [sim for sim in self.members(home) if self.dependent(sim, now)]
        children = [sim for sim in dependent if self.sims[sim][11] == man]
        # For the stories: the adults of the household who are not moved out (they stay, no
        # carer, no move) and the dependent children cared for by neither (with their carer).
        adults = [sim for sim in self.members(home) if sim not in (woman, man) and not self.dependent(sim, now) and not self.sims[sim][10]]
        neither = [(sim, self.sims[sim][11]) for sim in dependent if self.sims[sim][11] not in (woman, man)]
        causes = [variant(0, varint(entry)), variant(3, string(HOUSEHOLD_RULE))]
        self._found([man] + children, now, causes, "separation")
        for sim in dependent:
            if self.sims[sim][11] not in (woman, man):
                assert self.sims[sim][11] is not None, "an orphan household holds no couple"
                self._set_carer(sim, woman, now, "separation, the children stay with the mother")
        stayed = [sim for sim in dependent if sim not in children]
        self.separation_homes.append((now, woman, man, stayed, children))
        self.separation_details.append({"t": now, "woman": woman, "man": man, "home": home, "stayed": stayed, "moved": children, "adults_at_home": adults, "cared_by_neither": neither})

    def _leave_home(self, event: Event, now: int, plan: bytes) -> None:
        """`leave_home`: the person founds a household of his own and moves (causes the plan
        of the event and the rule); moved out from now on; carer None. Then (part 2,
        section 5) each dependent child he cared for (by SimId) gets the other biological
        parent if that one lives in the child's household and is fit to care (a partner he
        cannot have: he never formed a partnership); the others form the group of the
        orphan rule, with the same causes."""
        sim = event.subject[1]
        assert sim in self.sims and not self.sims[sim][10]
        assert self.open_partnership(sim) is None and not any(sim in value[:2] for value in self.partnerships.values())
        self.sims[sim][10] = True
        if self.sims[sim][11] is not None:
            self._set_carer(sim, None, now, "left home")
        causes = [plan, variant(3, string(HOUSEHOLD_RULE))]
        self._found([sim], now, causes, "leave_home")
        cared = sorted(child for child in self.sims if self.sims[child][11] == sim and self.dependent(child, now))
        orphans = []
        for child in cared:
            heir = self._other_parent(child, sim, now)
            if heir is None:
                orphans.append(child)
            else:
                self._set_carer(child, heir, now, "succession: the other parent")
        if orphans:
            self._place(orphans, now, causes, "carer left home")

    def _other_parent(self, child: int, gone: int, now: int):
        """The first of the child's mother and father who is not `gone`, lives in the
        child's household and is fit to care, or None (succession, part 1 section 5). For a
        pool person the mother and the father are the members of the child's household whose
        lineage keys are its PoolOrigin keys (design note of the arrival, section 7)."""
        if self.is_pool_person(child):
            home = self.sims[child][9]
            members = {self.key_of(sim): sim for sim in self.members(home)}
            parents = [members.get(key) for key in self.biological_keys(child) if key is not None]
            parents = [parent for parent in parents if parent is not None]
        else:
            parents = self.sims[child][7]
            parents = parents[:2] if parents is not None else ()
        for parent in parents:
            if parent != gone and parent in self.sims and self.sims[parent][9] == self.sims[child][9] and self.fit_to_care(parent, now):
                return parent
        return None

    def _succession(self, dead: int, partner, entry: int, now: int) -> None:
        """The succession of the carer (design note of the households, section 5): each
        dependent child the dead person cared for (by SimId) gets at once the other
        biological parent if he lives in the child's household and is fit to care, else
        the dead carer's partner in the partnership his death ended (`partner`) if he lives
        there and is fit to care (a step-parent: no social parent edge), else the orphan
        rule: those children form one group (part 2, sections 3 and 5), the causes the
        `core:death` and the rule."""
        cared = sorted(sim for sim in self.sims if self.sims[sim][11] == dead and self.dependent(sim, now))
        orphans = []
        for child in cared:
            home = self.sims[child][9]
            heir, why = self._other_parent(child, dead, now), "succession: the other parent"
            if heir is None and partner is not None and partner in self.sims and self.sims[partner][9] == home and self.fit_to_care(partner, now):
                heir, why = partner, "succession: the partner (step-parent)"
            if heir is None:
                orphans.append(child)
            else:
                self._set_carer(child, heir, now, why)
        if orphans:
            causes = [variant(0, varint(entry)), variant(3, string(HOUSEHOLD_RULE))]
            self._place(orphans, now, causes, "death of the carer")

    # ------------------------------------------------------------------ the orphan rule

    def adult_at(self, sim: int, now: int) -> bool:
        """18 or more at `now` (`now − born ≥ 18 · YEAR`)."""
        return now - self.sims[sim][0] >= 18 * YEAR

    def relatives(self, representative: int, now: int) -> list:
        """Stage 1 of the orphan rule (part 2, section 3): the living people of 18 or more at
        `now` with `φ(representative, person) > 0`, as (φ, sim), best first: the greatest
        `φ`, a tie the smaller lineage key. `φ` exact, over every sim living or archived."""
        found = [(self.kinship(representative, sim), sim) for sim in sorted(self.sims) if self.adult_at(sim, now)]
        return sorted(((phi, sim) for phi, sim in found if phi > 0), key=lambda row: (-row[0], self.key_of(row[1])))

    def foster_women(self, now: int) -> list:
        """Stage 2 (part 2, sections 3 and 4): the living women of 18 or more at `now` who
        are the woman of a partnership with `ended` None, by SimId."""
        return [sim for sim in sorted(self.sims) if self.sims[sim][2] == FEMALE and self.adult_at(sim, now) and self.partner_of(sim) is not None]

    def foster_choice(self, representative: int, women: list, now: int):
        """The woman FOSTER_RANK chooses (part 2, section 1): `k_smallest` with `k` = 1 at
        (FOSTER_RANK, the representative's lineage key, `floor(now / MONTH)`, the women's
        lineage keys); the smallest `(rank, lineage key)`. Returns (the woman, the ranks as
        (woman, key, rank))."""
        n = now // MONTH
        subject = self.key_of(representative)
        keys = {self.key_of(woman): woman for woman in women}
        assert len(keys) == len(women)
        (key,) = k_smallest(self.seed, FOSTER_RANK, subject, n, list(keys), 1)
        ranks = [(woman, self.key_of(woman), rank(self.seed, FOSTER_RANK, subject, n, self.key_of(woman))) for woman in women]
        assert min(ranks, key=lambda row: (row[2], row[1]))[0] == keys[key]
        return keys[key], ranks

    def _place(self, children: list, now: int, causes: list, trigger: str, at_start: bool = False) -> None:
        """The orphan rule (part 2, section 3) for one group: the children (dependent, of one
        household, or founders at the start) sorted by age, the eldest first (born, then
        lineage key); the representative is the first. In this order: the nearest relative
        (`relatives`), else the foster household (`foster_women`, `foster_choice`), else a
        household of their own. In the first two the chosen person becomes the carer of each
        child, and the relative (unless its biological parent), or the foster woman and then
        the man of her open partnership (each unless its biological parent), its social
        parents (each at most once, appended); the children move into the carer's household
        (`core:moved` per child, the eldest first, `causes`) unless they live there already
        (no entry). In the last the group founds a household (`core:household_formed`,
        subjects by SimId, then `core:moved` per child, eldest first; at the start no
        `core:moved`, as in part 1), no carer, still dependent."""
        group = sorted(children, key=lambda child: (self.sims[child][0], self.sims[child][1]))
        homes = {self.sims[child][9] for child in group}
        assert len(homes) == 1, homes
        (home,) = homes
        assert all(self.dependent(child, now) for child in group)
        representative = group[0]
        record = {"t": now, "trigger": trigger, "group": group, "representative": representative, "home": home, "causes": causes}
        found = self.relatives(representative, now)
        record["relatives"] = [(sim, phi, self.key_of(sim)) for phi, sim in found]
        carer, social = None, []
        if found:
            carer = found[0][1]
            social = [carer]
            record["stage"] = "relative"
        else:
            women = self.foster_women(now)
            record["women"] = women
            if women:
                carer, ranks = self.foster_choice(representative, women, now)
                record["ranks"] = ranks
                social = [carer, self.partner_of(carer)]
                record["stage"] = "foster"
            else:
                record["stage"] = "own household"
        record["carer"] = carer
        record["entries"] = []
        before = self.next_event_id
        if carer is None:
            for child in group:
                self._set_carer(child, None, now, "orphan rule: a household of its own")
            record["household"] = self._found(group, now, causes, f"orphan rule ({trigger}): own household", moves=not at_start)
            record["added"] = {child: [] for child in group}
        else:
            record["added"] = {}
            for child in group:
                self._set_carer(child, carer, now, f"orphan rule: {record['stage']}")
                added = []
                for person in social:
                    if not self.is_biological_parent(child, person) and person not in self.sims[child][12]:
                        self.sims[child][12].append(person)
                        added.append(person)
                record["added"][child] = added
            target = self.sims[carer][9]
            record["household"] = target
            if home != target:
                for child in group:
                    self._join(child, target, now, causes, f"orphan rule ({trigger}): {record['stage']}")
        record["entries"] = list(range(before, self.next_event_id))
        self.placements.append(record)

    # ------------------------------------------------------------------ separation

    def separation_budget(self, woman_key: int, k: int, man_key: int) -> float:
        """`B = exponential(SEPARATION_BUDGET, 1)` at (SEPARATION_BUDGET, her lineage key,
        `k`, his lineage key) (design note of the separation, section 1)."""
        return exponential_reference(words(chacha_key(self.seed, SEPARATION_BUDGET, woman_key, k, man_key))[0], 1.0)[1]

    def separation_seconds(self, budget: float):
        """`max(from_years_ceil(B / λ), 1 s)` (design note section 3), or None: `λ` = 0 or
        never. Each due second is checked not to hang on the last bit of the budget (the
        only libm in it, `ln`), so the pinned libm of the core gives the same second."""
        rate = self.data.separation_rate
        if rate == 0.0:
            return None

        def seconds(ulps: int):
            years = step_ulps(budget, ulps) / rate
            whole = from_years_ceil(years)
            return None if whole is None else max(whole, 1)

        nominal = seconds(0)
        assert all(seconds(ulps) == nominal for ulps in (-1, 1)), f"a separation hangs on the last bit of {budget!r}"
        return nominal

    def separation_k(self, number_: int) -> int:
        """`k` of the partnership (design note of the separation, section 6): her
        `partnerships` less the number of her partnerships with this PartnershipId or a
        larger one. For a town woman that is the number of her partnerships with a smaller
        PartnershipId, as before; for an arrival her count carries the pool's (design note
        of the arrival, section 5: the arrival's own partnership is the pool's last, `k` =
        her pool count less one; decision of NOTES.md)."""
        woman = self.partnerships[number_][0]
        counter = self.sims[woman][8] if woman in self.sims else self.archive[woman][10]
        later = sum(1 for other, value in self.partnerships.items() if other >= number_ and value[0] == woman)
        return counter - later

    def arrival_partnership(self, number_: int):
        """The carried separation `(due or None,)` of a partnership an arrival brought
        (design note of the arrival, section 9: formed at `formed_pool + Δ`, both partners
        arrivals of the same household of the pool), else None: its due is not recomputed,
        it is the woman's `separation_at + Δ` of the pool."""
        woman, man, formed, _ = self.partnerships[number_]
        if not (self.is_arrival(woman) and self.is_arrival(man)) or self.village:
            # The village brings no partnership (design note of the village, section 6).
            return None
        pool = self.data.pool
        index, member = pool.members[self.key_of(woman)]
        household = pool.households[index]
        if household["partnership"] is None or pool.members[self.key_of(man)][0] != index:
            return None
        woman_key, man_key, formed_pool = household["partnership"]
        delta = self.origins[woman]["arrived"] - pool.frozen_at
        if (woman_key, man_key, formed_pool + delta) != (self.key_of(woman), self.key_of(man), formed) or self.origins[man]["arrived"] != self.origins[woman]["arrived"]:
            return None
        due = None if member["separation_at"] is None else member["separation_at"] + delta
        return (due,)

    def separation_due(self, number_: int):
        """When the partnership separates if nothing else happens first, or None (design
        note section 3; section 6: `k` = the partnerships of the same woman with a smaller
        PartnershipId, `separation_k`). A partnership an arrival brought has its carried
        separation (`arrival_partnership`), not recomputed."""
        carried = self.arrival_partnership(number_) if self.origins else None
        if carried is not None:
            return carried[0]
        woman, man, formed, _ = self.partnerships[number_]
        k = self.separation_k(number_)
        seconds = self.separation_seconds(self.separation_budget(self.key_of(woman), k, self.key_of(man)))
        if seconds is None:
            return None
        due = formed + seconds
        return due if abs(due) <= MAX_ABS_SECS else None

    def _plan_separation(self, number_: int, k: int, by: int, now: int) -> None:
        """The event `separation { partnership }` (variant 5, class general, subject the
        partnership, kind 6) at its due, planned by the entry `by` (design note section
        3)."""
        woman, man = self.partnerships[number_][:2]
        budget = self.separation_budget(self.key_of(woman), k, self.key_of(man))
        due = self.separation_due(number_)
        self.separation_plans.append((number_, k, budget, due))
        if due is None:
            return
        payload = variant(5, varint(number_))
        self._schedule(due, GENERAL, (PARTNERSHIP_SUBJECT, number_), SEPARATION_KIND, payload, None, by, now)

    def open_partnership(self, sim: int):
        """The number of the open partnership of the sim (woman or man), or None."""
        open_ = [n for n, p in self.partnerships.items() if sim in p[:2] and p[3] is None]
        assert len(open_) <= 1
        return open_[0] if open_ else None

    def pending_separation(self, number_: int):
        """The time of the pending separation of the partnership, or None."""
        times = [e.time for e in self.queue if e.kind == SEPARATION_KIND and e.subject == (PARTNERSHIP_SUBJECT, number_)]
        assert len(times) <= 1
        return times[0] if times else None

    def _separate(self, event: Event, now: int, plan: bytes) -> None:
        """A separation at `now` (design note section 4): the partnership ends by
        separation, her series ends (a running pregnancy stays), one entry
        `core:separated { partnership }` (variant 9) with the subjects her and him and the
        causes the plan and the draw."""
        number_ = event.subject[1]
        couple_ = self.partnerships[number_]
        woman, man, _, ended = couple_
        assert ended is None
        couple_[3] = (now, ENDED_BY_SEPARATION)
        attempts = variant(3, varint(woman))
        self.queue = [other for other in self.queue if other.payload != attempts]
        k = self.separation_k(number_)
        key = (SEPARATION_BUDGET, self.key_of(woman), k, self.key_of(man))
        draw = variant(4, *(varint(word) for word in key))
        subjects = [variant(0, varint(woman)), variant(0, varint(man))]
        entry = self._entry(now, subjects, [plan, draw], variant(9, varint(number_)))
        self.separations.append((now, number_, entry, event.plan, key))
        if self.households_on:
            self._house_separation(woman, man, entry, now)

    # ------------------------------------------------------------------ market

    def kinship(self, x: int, y: int) -> Fraction:
        """`φ(x, y)` of the genetics specification 2.7 over the ancestors of `x` and `y` up
        to their great-grandparents, living or archived (Grok, H2; ADR-0020 section 4;
        design note of the kinship depth, section 1): the recursion of
        `spec_reference.Pedigree.query` on the SimIds, the later SimId the younger (SimIds
        go by birth, a tie in one second the larger SimId; between two founders, who have no
        parents, the order changes nothing). Missing parents and parents outside the set of
        the query count 0. Exact. The cache holds only the final `φ` of a pair (section 1
        of the note): the set of a pair never changes, since parents are fixed at birth."""
        # Over lineage keys (design note of the arrival, section 7): the world looks a person
        # up by SimId, takes its lineage key and queries `kinship_by_keys` with `kin_of`; the
        # values are those of the SimId pedigree for a town without a pool (the recursion's
        # order changes nothing exact). The cache is keyed by the pair of lineage keys.
        kx, ky = self.key_of(x), self.key_of(y)
        pair = (min(kx, ky), max(kx, ky))
        if pair not in self.kinships:
            self.kinships[pair] = kinship_by_keys(kx, ky, self.kin_of)[0]
        return self.kinships[pair]

    def kin_of(self, key: int) -> tuple:
        """`(order, mother, father)` of a lineage key (design note of the arrival, section 7):
        an arrival (living or archived) `(0, born_pool)` with its PoolOrigin keys; a pool child
        `(1, born)` with its PoolOrigin keys; a town person `(1, born)` with its parents'
        lineage keys (from their SimIds) or none; a record of the data's pool that is no
        person of the town `(0, born)` of the record with the record's parents. Nothing else is
        reachable (asserted)."""
        sim = self.by_key.get(key)
        if sim is not None:
            origin = self.origins.get(sim)
            if origin is not None:
                order = (0, origin["born_pool"]) if origin["born_pool"] is not None else (1, self.born_of(sim))
                return order, origin["mother"], origin["father"]
            parents = self.parent_keys(sim)
            return ((1, self.born_of(sim)),) + ((None, None) if parents is None else parents)
        pool = self.data.pool
        assert pool is not None and key in pool.records, f"no person and no record for the key {key:016x}"
        record = pool.records[key]
        return (0, record["born"]), record["mother"], record["father"]

    def kinship_query(self, x: int, y: int, depth=KINSHIP_DEPTH) -> tuple:
        """`(φ, S, visited)` of one query (`spec_reference.Pedigree.query`), for the known
        answers of the depth (design note of the kinship depth, section 4): the value, the
        set `S` of the query and the pairs its recursion evaluated. `depth=None` is the rule
        before H2 (every known ancestor), only to show what the depth changes."""
        return self.pedigree.query(x, y, depth)

    def phi_float(self, x: int, y: int) -> float:
        """φ as f64: a sum of powers of two, exact."""
        phi = self.kinship(x, y)
        value = float(phi)
        assert Fraction(value) == phi
        return value

    def kinship_of(self, a: int, b: int):
        """The kinship query (design note of the pedigree, section 4,
        `WorldState::kinship(a, b) -> Option<f64>`): `φ` of two persons, living or archived,
        as f64 (exact, a dyadic fraction), the same `φ` the market uses (`kinship`); None if
        either SimId is unknown (never given, or 0). `kinship_of(a, a)` is `(1 + F_a) / 2`
        (spec 2.7): 1/2 for a founder."""
        if not all(sim in self.sims or sim in self.archive for sim in (a, b)):
            return None
        return self.phi_float(a, b)

    def born_of(self, sim: int) -> int:
        return self.sims[sim][0] if sim in self.sims else self.archive[sim][0]

    def parent_keys(self, sim: int):
        """(the mother's lineage key, the father's) of a sim born in the run, or None (a
        founder)."""
        parents = self.sims[sim][7] if sim in self.sims else self.archive[sim][9]
        if parents is None:
            return None
        return self.key_of(parents[0]), self.key_of(parents[1])

    def phenotype_parts(self, sim: int, trait: PhenotypeTrait, now: int) -> dict:
        """The phenotype of the trait of a living sim at `now` and its parts
        (`phenotype_parts`): its genome, its lineage key, its parents' keys, its age `now −
        born` in seconds."""
        born, key, _, person = self.sims[sim][:4]
        e_seed, c_seed, parents = self.env_of(sim)
        return phenotype_parts(e_seed, trait, person, key, parents, now - born, c_seed)

    def phenotypes(self, sim: int, now: int) -> list:
        """(weight, z) of each present similarity term (cognition, then height) for a living
        sim at `now`."""
        return [(weight, self.phenotype_parts(sim, trait, now)["z"]) for _, trait, weight in self.data.similarity_traits()]

    def sex_of(self, sim: int) -> bytes:
        return self.sims[sim][2] if sim in self.sims else self.archive[sim][4]

    def measure(self):
        """`WorldState::measure` (design note of the measurement, section 1): `measure_of`
        over the living (the active population, `self.sims`) at the clock, None without
        one. A read-only view: no draw, no event, no entry, no state."""
        return measure_of([(value[1], value[3]) for value in self.sims.values()], self.data)

    def measure_json(self):
        """The line of `simagent measure` for this world (its clock the last closed second),
        or None (the CLI's error `no_living`)."""
        measurement = self.measure()
        return None if measurement is None else measurement_json(self.closed, measurement)

    def _market_round(self, now: int, plan: bytes) -> None:
        """A round at `now` (design note of the market, section 3), `n = now / MONTH`: the
        single men in the market age, held for the round; the single women in it who
        search (MARKET_SEARCH); their order (MARKET_ORDER); per searcher in turn her
        candidates (PAIRING_RANK, skipping `φ ≥ 1/8`, up to `k`), the utility and the
        choice; a pairing if the chosen man has no open partnership now."""
        data = self.data
        n = now // MONTH
        assert now == n * MONTH and n >= 1
        adult = data.market_age * YEAR

        def single_in_market(sim: int, sex: bytes) -> bool:
            born, _, sim_sex = self.sims[sim][:3]
            return sim_sex == sex and self.open_partnership(sim) is None and now - born >= adult

        men = [sim for sim in sorted(self.sims) if single_in_market(sim, MALE)]
        women = [sim for sim in sorted(self.sims) if single_in_market(sim, FEMALE)]
        if not women:
            return
        considered, searchers = [], []
        for woman in women:
            age = now - self.sims[woman][0]
            p = search_chance(age, data)
            word = words(chacha_key(self.seed, MARKET_SEARCH, self.key_of(woman), n, 0))[0]
            u = unit(word)
            # A chance from exp keeps the margin; 0 and 1 cannot be moved by libm.
            assert p in (0.0, 1.0) or abs(u - p) > DRAW_MARGIN, f"a search hangs on the last bits of {p!r}"
            searched = bernoulli(word, p)
            considered.append((woman, age, p, u, searched, age < 18 * YEAR))
            if searched:
                searchers.append(woman)
        order = ranked(self.seed, MARKET_ORDER, WORLD_KEY, n, [self.key_of(woman) for woman in searchers])
        by_key = {self.key_of(woman): woman for woman in searchers}
        boys = [sim for sim in sorted(self.sims) if self.sims[sim][2] == MALE and now - self.sims[sim][0] < adult]
        record = {"t": now, "n": n, "men": men, "considered": considered, "boys": boys, "choices": []}
        self.rounds.append(record)
        for key in order:
            record["choices"].append(self._choose(by_key[key], men, now, n, plan))

    def _choose(self, woman: int, men: list, now: int, n: int, plan: bytes) -> dict:
        """Her candidates, the utilities and her choice (design note of the market, section
        3, steps 4 to 6); the record of it."""
        data = self.data
        her_key, her_born = self.key_of(woman), self.sims[woman][0]
        by_key = {self.key_of(man): man for man in men}
        walked, kept = [], []
        for key in ranked(self.seed, PAIRING_RANK, her_key, n, list(by_key)):
            if len(kept) == data.market_candidates:
                break
            man = by_key[key]
            phi = self.kinship(woman, man)
            skipped = phi >= Fraction(data.kinship_limit)
            walked.append((man, phi, skipped))
            if not skipped:
                kept.append(man)
        record = {"woman": woman, "walked": walked, "candidates": [], "chosen": None, "outcome": "no candidate", "terms": []}
        if not kept:
            return record
        # Her phenotypes once (design note of the similarity, section 2), each candidate's
        # once per candidate; the similarity terms only for the named traits.
        hers = self.phenotypes(woman, now)
        record["phenotypes"] = {woman: [z for _, z in hers]}
        best = None
        for index, man in enumerate(kept):
            gap = as_years(now - self.sims[man][0]) - as_years(now - her_born)
            phi = self.phi_float(woman, man)
            g = noise_of(self.seed, her_key, n, self.key_of(man))
            his = self.phenotypes(man, now)
            record["phenotypes"][man] = [z for _, z in his]
            similarity = [(weight, z_her - z_his) for (weight, z_her), (_, z_his) in zip(hers, his)]
            terms = penalties(gap, phi, data, similarity)
            u = utility_of(terms, g)
            assert u == utility(gap, phi, g, data, similarity)
            assert math.isfinite(u), "market.utility"
            record["candidates"].append((man, gap, phi, g, u))
            record["terms"].append(terms)
            # The largest; on a tie the earlier in the list (the smaller rank).
            if best is None or u > record["candidates"][best][4]:
                best = index
        top = record["candidates"][best][4]
        assert all(abs(top - u) > UTILITY_MARGIN for i, (*_, u) in enumerate(record["candidates"]) if i != best), (
            "a choice hangs on the last bits of the utility"
        )
        man = kept[best]
        record["chosen"] = man
        if self.open_partnership(man) is not None:
            # Chosen earlier in this round: no second try (11.2).
            record["outcome"] = "taken"
            return record
        draw = variant(4, varint(PAIRING_NOISE), varint(her_key), varint(n), varint(self.key_of(man)))
        record["outcome"] = "paired"
        record["partnership"] = self.next_partnership_id
        record["entry"] = self._partner(woman, man, [plan, variant(3, string(PARTNER_MARKET_RULE)), draw], now)
        return record

    def arrange_rejection(self, woman: int, man: int):
        """The reject code of `ArrangePartnership { woman, man }` submitted now, or None
        (ADR-0004 section 4; design note of the market, section 4): the existing checks in
        their order, each for both (unknown, not alive, the sexes, already partnered), then
        `PartnerTooYoung` (either under the market age in the second the input acts, the
        stamp plus one), then `PartnersTooCloselyRelated` (`φ ≥ 1/8`)."""
        acts = self.closed + 1
        pair = (woman, man)
        if any(sim not in self.sims and sim not in self.archive for sim in pair):
            return UNKNOWN_SIM
        if any(sim not in self.sims for sim in pair):
            return SIM_NOT_ALIVE
        if self.sims[woman][2] != FEMALE or self.sims[man][2] != MALE:
            return PARTNERS_OF_WRONG_SEX
        if any(self.open_partnership(sim) is not None for sim in pair):
            return ALREADY_PARTNERED
        if any(acts - self.sims[sim][0] < self.data.market_age * YEAR for sim in pair):
            return PARTNER_TOO_YOUNG
        if self.kinship(woman, man) >= Fraction(self.data.kinship_limit):
            return PARTNERS_TOO_CLOSELY_RELATED
        return None

    def try_arrange(self, woman: int, man: int):
        """`ArrangePartnership` submitted now: its reject code (nothing changes, no sequence
        number, ADR-0004 section 4), or None when it is accepted and applied."""
        code = self.arrange_rejection(woman, man)
        if code is None:
            self.arrange(woman, man)
        return code

    # ------------------------------------------------------------------ fertility

    def key_of(self, sim: int) -> int:
        """The lineage key of a living or archived sim."""
        return self.sims[sim][1] if sim in self.sims else self.archive[sim][3]

    def wish_of(self, woman: int) -> int:
        """Her wish (genetics specification 11.3, design note section 3): `wish(u, shape,
        tilt)` with `u` the draw FERTILITY_WISH at her lineage key; pure, not held. The
        draw keeps `DRAW_MARGIN` from every threshold."""
        data = self.data
        u = wish_unit(self.own_seed(woman), self.key_of(woman))
        assert all(abs(u - acc) > DRAW_MARGIN for acc in wish_thresholds(data.wish_shape, data.wish_tilt)), u
        return wish(u, data.wish_shape, data.wish_tilt)

    def theta_of(self, sim: int, now=None) -> float:
        """`θ_i` of a sim: the curve of its sex with `z_p` = 0 without a trait of the puberty
        timing (11.1), else with the phenotype `z_p` of that trait at its age `now − born`
        (0 if `now` is None; the weights do not depend on the age, so neither does `z_p`,
        ADR-0012 section 3)."""
        if sim in self.sims:
            born, key, sex, person = self.sims[sim][:4]
        else:
            born, _, _, key, sex, person = self.archive[sim][:6]
        if self.data.puberty is None:
            return theta_i(self.data.puberty_curve[sex], 0.0)
        age = 0 if now is None else now - born
        e_seed, c_seed, parents = self.env_of(sim)
        z_p = phenotype(e_seed, self.data.puberty, person, key, parents, age, c_seed)
        return theta_i(self.data.puberty_curve[sex], z_p)

    def menarche_of(self, woman: int, now=None) -> float:
        """Her menarche (5.6, design note section 3): `menarche(θ_i, δ)` with `δ` the draw
        FERTILITY_MENARCHE at her lineage key; the form `puberty` only, pure, not held."""
        form = self.data.fertility
        assert form.kind == "puberty"
        return menarche(self.theta_of(woman, now), menarche_delta(self.own_seed(woman), self.key_of(woman), form), form)

    def partner_of(self, woman: int):
        """Her partner in the open partnership, or None."""
        open_ = [p[1] for p in self.partnerships.values() if p[0] == woman and p[3] is None]
        assert len(open_) <= 1
        return open_[0] if open_ else None

    def factors(self, woman: int, man: int, t: int) -> tuple:
        """`(F_w, F_m)` at `t` (design note section 5): her factor at her age from her
        menarche (the window does not read it), his at his age from his `θ_i`."""
        form = self.data.fertility
        her_age = as_years(t - self.sims[woman][0])
        his_age = as_years(t - self.sims[man][0])
        her_menarche = self.menarche_of(woman, t) if form.kind == "puberty" else 0.0
        return female_factor(her_age, her_menarche, form), male_factor(his_age, self.theta_of(man, t), form)

    def attempt_chance(self, woman: int, man: int, t: int) -> float:
        """The chance of her attempt at `t`: `chance(adult_rate, F_w, F_m)`."""
        f_w, f_m = self.factors(woman, man, t)
        return chance(self.data.adult_rate, f_w, f_m)

    def series_allowed(self, woman: int, s: int) -> bool:
        """Whether her series may be planned at `s` now (design note section 4): an open
        partnership, not pregnant, `births < wish`, `age(w, s) < E`."""
        her = self.sims[woman]
        return (
            self.partner_of(woman) is not None
            and woman not in self.pregnancies
            and her[6] < self.wish_of(woman)
            and as_years(s - her[0]) < self.data.fertility_end
        )

    def _plan_series(self, woman: int, s: int, by: int, now: int) -> None:
        """Her monthly series from `s` on, planned by the entry `by`, if the rule allows it
        at this moment."""
        if not self.series_allowed(woman, s):
            return
        payload = variant(3, varint(woman))
        self._schedule(s, GENERAL, (SIM_SUBJECT, woman), ATTEMPT_KIND, payload, MONTH, by, now)

    def pending_series(self, woman: int):
        """The time of her pending series, or None."""
        times = [event.time for event in self.queue if event.kind == ATTEMPT_KIND and event.subject == (SIM_SUBJECT, woman)]
        assert len(times) <= 1
        return times[0] if times else None

    def town_births(self, woman: int) -> list:
        """The birth seconds of her children born in the town, living or archived: town
        children (`parents` with her as mother) and pool children (PoolOrigin mother her
        key)."""
        key = self.key_of(woman)
        seconds = []
        for sim in set(self.sims) | set(self.archive):
            parents = self.sims[sim][7] if sim in self.sims else self.archive[sim][9]
            if parents is not None and parents[0] == woman:
                seconds.append(self.born_of(sim))
            elif sim in self.origins and self.origins[sim]["born_pool"] is None and self.origins[sim]["mother"] == key:
                seconds.append(self.born_of(sim))
        return seconds

    def series_exempt(self, woman: int, number_: int) -> bool:
        """Whether her series is still the one she brought (review of the reference of the
        arrival, NOTES2.md): she is an arrival, her open partnership `number_` is the one her
        household brought (`arrival_partnership`), and no child of hers was born in the
        town. Only then is her series the carried one, exempt from the town's rule."""
        return self.is_arrival(woman) and self.arrival_partnership(number_) is not None and not self.town_births(woman)

    def series_due(self, sim: int):
        """The second of the pending series of attempts the rules give a living sim, or None
        (ADR-0017 sections 4 and 5: the series is pending exactly when the rule asks for it;
        design note of the arrival, sections 5 and 9, for the series an arrival brought):

        - none for a man, a pregnant woman, a woman without an open partnership;
        - exempt (`series_exempt`): the carried series, from `conception_at + Δ` of the pool
          every MONTH, not recomputed; none if the pool carried none;
        - else the town's rule: from the last trigger in her open partnership, its forming at
          `f` (series from the first month start after `f`) or her last birth in the town at
          `b` (from the first month start at or after `b + max(pause, 1 s)`), the birth the
          later one if `b > f`, or `b = f` with the partnership of an input (the input acts
          before the events of its second; a round runs after the births of its second);
          none if `births >= wish`;
        - in both the pending one is the first second of that monthly grid after the clock,
          and it is there if it is the grid's first or `age < E` at it (an attempt that fails
          at `t` ends the series when `age(t + MONTH) >= E`; the town's first needs `age < E`
          at it as well)."""
        her = self.sims[sim]
        if her[2] != FEMALE or sim in self.pregnancies:
            return None
        number_ = self.open_partnership(sim)
        if number_ is None:
            return None
        _, man, formed, _ = self.partnerships[number_]

        def pending(first: int):
            return first if first > self.closed else first + ((self.closed - first) // MONTH + 1) * MONTH

        if self.series_exempt(sim, number_):
            carried = self.pool_member(sim)["conception_at"]
            if carried is None:
                return None
            first = carried + self.origins[sim]["arrived"] - self.data.pool.frozen_at
            t = pending(first)
            return t if t == first or as_years(t - her[0]) < self.data.fertility_end else None
        births = self.town_births(sim)
        last = max(births) if births else None
        if last is not None and (last > formed or (last == formed and (formed, sim, man) in self.input_pairings)):
            first = month_start_at_or_after(last + max(self.data.pause_months * MONTH, 1))
        else:
            first = month_start_after(formed)
        if her[6] >= self.wish_of(sim):
            return None
        t = pending(first)
        return t if as_years(t - her[0]) < self.data.fertility_end else None

    def _attempt(self, event: Event, now: int, plan: bytes) -> None:
        woman = event.subject[1]
        her = self.sims[woman]
        man = self.partner_of(woman)
        him = self.sims[man]
        p = self.attempt_chance(woman, man, now)
        draw = (CONCEPTION_ATTEMPT, her[1], now // MONTH, him[1])
        word = words(chacha_key(self.seed, *draw))[0]
        assert abs(unit(word) - p) > DRAW_MARGIN, f"an attempt hangs on the last bits of its chance {p!r}"
        self.attempts.append((now, woman, man, p, bernoulli(word, p)))
        if not bernoulli(word, p):
            # Her last attempt: the next one would be at or after the end of her fertility.
            if as_years(now + MONTH - her[0]) >= self.data.fertility_end:
                self.queue.remove(event)
            return
        self.queue.remove(event)
        conception_number = her[5]
        her[5] += 1
        # The twins (design note of the twins, sections 1 and 2): the draw at the key of
        # the first child decides one child, identical or fraternal twins.
        first = child_key(her[1], him[1], conception_number, 0)
        u = multiple_unit(self.seed, first)
        kind = multiple_kind(u, self.data.twins)
        keys = [first] if kind == SINGLE else [first, child_key(her[1], him[1], conception_number, 1)]
        random = lambda key: variant(4, *(varint(word) for word in key))
        causes = [plan, random(draw), random((MULTIPLE_BIRTH, first, 0, 0))]
        subjects = [variant(0, varint(woman)), variant(0, varint(man))]
        children, entries = [], []
        for index, key in enumerate(keys):
            # Per child in index order its conception entry (section 3), the same causes
            # for both.
            entries.append(self._entry(now, subjects, causes, variant(6, varint(key))))
            if kind == IDENTICAL and index == 1:
                # Identical twins share the zygote of child 0 and its two meioses.
                _, sex, person = children[0]
                children.append((key, sex, copy_genome(person)))
                continue
            sex, person = self._zygote(key, woman, man, entries[-1], now)
            children.append((key, sex, person))
        self.multiples.append((now, woman, man, conception_number, u, kind, [(key, sex) for key, sex, _ in children]))
        self.pregnancies[woman] = (man, conception_number, children, now)
        months = self.data.pregnancy_months
        payload = variant(4, varint(woman))
        # One birth, planned by the conception entry of child 0.
        self._schedule(now + months * MONTH, BIRTH, (SIM_SUBJECT, woman), BIRTH_KIND, payload, None, entries[0], now)

    def _zygote(self, genome_key: int, woman: int, man: int, conception: int, now: int) -> tuple:
        """(sex, genome) of the zygote under `genome_key`, with its two entries
        `core:meiosis` (her side, then his), caused by the conception entry
        `conception` and the rule."""
        # The meiosis of the independent reference (tools/meiosis_reference.py, written
        # from the genetics specification 2.3 by another model).
        data = self.data
        person, records = meiosis_reference.child_with_mutation(
            self.seed, genome_key, self.sims[woman][3], self.sims[man][3], data.map_lengths, data.sex_ratio, data.genome_map, data.rates
        )
        for side, parent in ((0, woman), (1, man)):
            record = records["mother" if side == 0 else "father"]
            segments = seq(
                [
                    segment_meiosis(s["segment"], [MATERNAL, PATERNAL][s["start_strand"]], s["crossover_positions"])
                    for s in record["segments"]
                ]
            )
            chromosome = Y_CHROMOSOME if record.get("sex_chromosome") == "y" else X
            mutations = seq([mutation(m["locus"], m["old"], m["new"]) for m in record["mutations"]])
            # ADR-0004 section 7: the conception and the rule; the draws follow from the
            # genome key and the side.
            causes = [variant(0, varint(conception)), variant(3, string("core:meiosis"))]
            payload = variant(7, varint(genome_key), segments, chromosome, mutations)
            self._entry(now, [variant(0, varint(parent))], causes, payload)
        sex = MALE if "y" in person["paternal_sex"] else FEMALE
        return sex, person

    def _birth(self, event: Event, now: int, plan: bytes) -> None:
        mother = event.subject[1]
        father, conception_number, children, _ = self.pregnancies.pop(mother)
        # A pregnancy of the pool (design note of the arrival, section 6.3): its children are
        # pool children; the father may have no SimId.
        pool_father = self.pool_fathers.pop(mother, None)
        # Per child in index order: the next SimId and its creation entry, one after the
        # other (design note of the twins, sections 3 and 4).
        born = []
        home = self.sims[mother][9]
        for key, sex, person in children:
            child = self.next_sim_id
            self.next_sim_id += 1
            subjects = [variant(0, varint(child)), variant(0, varint(mother))] + ([] if father is None else [variant(0, varint(father))])
            entry = self._entry(now, subjects, [plan, variant(3, string("core:gestation"))], variant(8, varint(key)))
            if pool_father is None:
                c_seed = self.c_seed_of_child(mother, father)
                afflictions = self.afflictions_of(key, sex, person, now, (self.key_of(mother), self.key_of(father)), 0, c_seed)
                self.sims[child] = [now, key, sex, person, afflictions, 0, 0, (mother, father, conception_number), 0, None, False, None, []]
                self.pedigree.add(child, mother, father)
            else:
                # A pool child: `parents` None, its PoolOrigin, the pool's afflictions of the
                # unborn with the exposure from the birth if the switch is on.
                _, _, unborn = self.data.pool.unborn[key]
                assert unborn["sex"] == (0 if sex == FEMALE else 1) and genome_of(person) == genome_of(genome_from_raw(unborn["genome"]))
                afflictions = self.pool_afflictions(unborn["afflictions"], now)
                self.sims[child] = [now, key, sex, person, afflictions, 0, 0, None, 0, None, False, None, []]
                self.origins[child] = {
                    "genome_key": unborn["genome_key"],
                    "born_pool": None,
                    "arrived": now,
                    "arrival_year": now // YEAR,
                    "mother": self.key_of(mother),
                    "father": pool_father,
                    "social_parents": [],
                }
                self.pedigree.add(child)
            self.created_by[child] = entry
            self.by_key[key] = child
            born.append((child, entry))
            # Its onsets right after its `core:birth` (paths 1 and 2), before its `core:moved`
            # and the next child's `core:birth` (twins: birth A, onset A, birth B, onset B).
            self._onset_step(child, entry, now)
            if self.households_on:
                # The child joins the mother's household, right after its `core:birth`
                # (causes the plan of the birth and the rule), carer the mother, not moved
                # out (design note of the households, section 4).
                assert self.fit_to_care(mother, now)
                self.sims[child][9] = home
                self._set_carer(child, mother, now, "birth")
                moved = self._home_entry(now, [child], [plan, variant(3, string(HOUSEHOLD_RULE))], MOVED_VARIANT, home)
                self.home_log.append((now, "moved", home, [child], "birth", moved))
        # Then per child in index order its first birthday, its deaths and its `leave_home`,
        # by its own creation entry.
        for child, entry in born:
            self._plan_birthday(child, now + YEAR, entry, now)
            self._plan_deaths(child, entry, now)
            if self.households_on:
                self._plan_leave_home(child, entry, now)
        # She bore them alive; then the series after a birth (design note of the fertility
        # in the world, sections 4 and 6), if her partnership is open, planned by the
        # creation entry of child 0.
        self.sims[mother][6] += len(children)
        pause = max(self.data.pause_months * MONTH, 1)
        self._plan_series(mother, month_start_at_or_after(now + pause), born[0][1], now)

    def arrange(self, woman: int, man: int) -> None:
        """`ArrangePartnership` of two living sims, submitted right now; it must be
        accepted (`try_arrange` for one that may be rejected)."""
        code = self.arrange_rejection(woman, man)
        assert code is None, f"ArrangePartnership is rejected: {REJECT_NAMES[code]}"
        self.script.append(("arrange", woman, man))
        sequence = len(self.inputs) + 1
        self.inputs.append(signed(self.closed) + varint(sequence) + variant(2, varint(woman), varint(man)))
        self.input_pairings.append((self.closed + 1, woman, man))
        self._partner(woman, man, [variant(2, varint(sequence))], self.closed + 1)

    def _start(self, founders: list) -> None:
        now = 0
        by_input = variant(2, varint(1))
        started = self._entry(now, [], [by_input], variant(0, varint(len(founders))))
        self._schedule(YEAR - 1, OBSERVER, (WORLD_SUBJECT, 0), YEAR_END_KIND, variant(0), YEAR, started, now)
        # The rounds of the market, right after the year end series (seq 2): the first at the
        # first month start after the start, then every month (design note of the market,
        # section 2).
        if self.market:
            self._schedule(month_start_after(now), GENERAL, (WORLD_SUBJECT, 0), MARKET_ROUND_KIND, MARKET_ROUND_PAYLOAD, MONTH, started, now)
        # With a capacity the emigration, right after the round series and before anything of
        # the founders (insertion number 3): one periodic event, first at the last second of
        # year 0, period YEAR, the same second and period as the year end; not planned again
        # by its handler (design note of the emigration, section 2, as the coordinator fixed
        # it).
        if self.capacity is not None:
            payload = variant(EMIGRATION_VARIANT, varint(self.capacity))
            self._schedule(YEAR - 1, EMIGRATION_CLASS, (WORLD_SUBJECT, 0), EMIGRATION_KIND, payload, YEAR, started, now)
        # With a pool whose immigration is on and `q > 0` the immigration, right after the other
        # series (design note of the arrival, section 4; decision of NOTES.md): one periodic
        # event `Immigration { persons: q }`, first at second 0, period YEAR, planned by
        # `core:started`.
        if self.pool_start is not None and self.pool_start["immigration"] and immigration_persons(self.pool_start["n_start"]) > 0:
            payload = variant(IMMIGRATION_VARIANT, varint(immigration_persons(self.pool_start["n_start"])))
            self._schedule(0, IMMIGRATION_CLASS, (WORLD_SUBJECT, 0), IMMIGRATION_KIND, payload, YEAR, started, now)
        for index, (born, sex, genes) in enumerate(founders):
            sim = self.next_sim_id
            self.next_sim_id += 1
            key = founder_key(self.tag, index)
            subject = variant(0, varint(sim))
            added = self._entry(now, [subject], [by_input], variant(1, signed(born), lineage_key(key)))
            # Its genome under F1 (S2b): the loci it names, every other locus drawn with
            # FOUNDER_ALLELE where it has a `q` (`founder_alleles_drawn`).
            named, drawn, draws = founder_alleles_drawn(self.data, self.seed, key, sex, genes)
            person = founder_genome(sex, named + drawn, self.data.loci)
            self.founder_draws[sim] = draws
            # A founder's origin is the start (B19); its age at the creation `now − born`.
            afflictions = self.afflictions_of(key, sex, person, now, None, now - born)
            # Household, moved out, carer and social parents are set after the founders and
            # the couples (`_house_founders`).
            self.sims[sim] = [born, key, sex, person, afflictions, 0, 0, None, 0, None, False, None, []]
            self.created_by[sim] = added
            self.pedigree.add(sim)
            self.by_key[key] = sim
            # Its onsets right after its `core:founder_added` (paths 1 and 2), before its plans.
            self._onset_step(sim, added, now)
            self._plan_birthday(sim, first_birthday(born, now), added, now)
            self._plan_deaths(sim, added, now)

    def override(self, switch: bool) -> None:
        """An override block with the one change, submitted right now. A change that sets the
        switch on then writes the onsets of path 3 in the input's context (`_fork_onsets`),
        after its `core:param_changed` and the deaths it plans again."""
        self.script.append(("override", switch))
        sequence = len(self.inputs) + 1
        self.inputs.append(signed(self.closed) + varint(sequence) + variant(1, seq([param_change(switch)])))
        now = self.closed + 1
        self.switch_history.append((now, switch))
        flips = switch != self.switch
        self.switch = switch
        changed = self._entry(now, [], [variant(2, varint(sequence))], variant(3, param_change(switch)))
        if flips:
            self._switch_exposures(switch, changed, now)
        if switch:
            self._fork_onsets(now, sequence)
            # An "on" while the switch is on writes nothing: an earlier moment had it on (a
            # check of this reference, not a rule the path reads).
            assert flips or not self.fork_onsets[-1]["written"], self.fork_onsets[-1]

    def _switch_exposures(self, switch: bool, changed: int, now: int) -> None:
        """The switch flips at `now` by the entry `changed`: the exposures of every living sim's
        lethal afflictions open or close, and its deaths of a disease are planned again."""
        for sim in sorted(self.sims):
            born = self.sims[sim][0]
            lethal = [affliction for affliction in self.sims[sim][4] if affliction.exposure is not None]
            if not lethal:
                continue
            for affliction in lethal:
                if switch:
                    affliction.exposure[1] = now
                    continue
                used, since = affliction.exposure
                from_ = max(as_years(since - born), affliction.onset)
                to = as_years(now - born)
                if to > from_:
                    affliction.exposure[0] = used + disease_hazard_used(affliction.disease.hazard, from_, to)
                affliction.exposure[1] = None
            pending = {death_payload(sim, affliction.disease.key) for affliction in lethal}
            self.queue = [event for event in self.queue if event.payload not in pending]
            self._plan_disease_deaths(sim, changed, now)

    def override_nutrition(self, year: int, index: float) -> None:
        """An override block with the one change `ParamChange::NutritionYear { year, index }`
        (ADR-0013 section 2), submitted right now: stamped with the last closed second, it
        acts in the next one (`now`), whose calendar year is the fork year `X`; a year before
        `X` is `nutrition_year_past`, a year or index outside the range of `Start`
        `nutrition_out_of_range` (both rejections, asserted here). The year's entry is
        replaced or inserted, the list stays ascending; one entry `core:param_changed` (no
        subject, caused by the input). Nothing reads the list but the body mapping, so no
        event is planned or cancelled."""
        index = float(index)
        assert 0.0 <= index <= 1.0 and -1_000_000 <= year <= 999_999, "nutrition_out_of_range"
        now = self.closed + 1
        assert year >= now // YEAR, "nutrition_year_past"
        self.script.append(("override_nutrition", year, index))
        sequence = len(self.inputs) + 1
        change = param_change_nutrition(year, index)
        self.inputs.append(signed(self.closed) + varint(sequence) + variant(1, seq([change])))
        self.nutrition = sorted([entry for entry in self.nutrition if entry[0] != year] + [(int(year), index)])
        self._entry(now, [], [variant(2, varint(sequence))], variant(3, change))

    # ------------------------------------------------------------------ events

    def _entry(self, time: int, subjects: list, causes: list, payload: bytes) -> int:
        event_id = self.next_event_id
        self.next_event_id += 1
        assert payload[0] < 0x80
        self.entry_kinds.append(payload[0])
        self.log.append(varint(event_id) + signed(time) + seq(subjects) + seq(causes) + payload)
        return event_id

    def _schedule(self, time, cls, subject, kind, payload, period, by, planned_at) -> None:
        """Plans an event (ADR-0002, the micro steps): in the handler of the event `current`
        at `(t, p, m)` a time before `t`, or `t` in a lower phase, is an error, `t` in the
        same phase is micro step `m + 1` (at most MAX_MICRO_STEP), anything else micro 0; in
        an input (`current` None) every time is micro 0 (the input acts in an open second)."""
        insertion = self.next_insertion
        self.next_insertion += 1
        event = Event(time, cls, subject, kind, insertion, payload, period, (by, planned_at))
        current = self.current
        if current is not None:
            assert time > current.time or (time == current.time and event.phase() >= current.phase()), "scheduler.schedule_in_past"
            if time == current.time and event.phase() == current.phase():
                event.micro = current.micro + 1
                assert event.micro <= MAX_MICRO_STEP, "scheduler.cascade_limit"
        self.queue.append(event)

    def _plan_birthday(self, sim: int, time: int, by: int, now: int) -> None:
        self._schedule(time, GENERAL, (SIM_SUBJECT, sim), BIRTHDAY_KIND, variant(1, varint(sim)), None, by, now)

    def advance(self, limit: int) -> None:
        """Runs every event up to and including `limit`."""
        self.script.append(("advance", limit))
        while True:
            due = [event for event in self.queue if event.time <= limit]
            if not due:
                break
            event = min(due, key=Event.key)
            self.queue.remove(event)
            now = event.time
            plan = variant(1, varint(event.plan[0]), signed(event.plan[1]))
            self.ran.append((now, event.kind))
            # The key of every event that ran with the first entry it wrote (for the order of
            # the micro steps in the known answers); not state.
            self.fired.append((event.key(), self.next_event_id))
            # The handler's context (the micro steps); a periodic event's next occurrence is a
            # later second, micro 0 (it is in the queue before the handler runs).
            self.current = copy.copy(event)
            if event.period is not None:
                event.micro = 0
            try:
                self._run(event, now, plan)
            finally:
                self.current = None
        self.closed = limit

    def _run(self, event: Event, now: int, plan: bytes) -> None:
        """The handler of one event (`advance`)."""
        if event.kind == YEAR_END_KIND:
            # The next occurrence is in the queue before the observer looks.
            event.time += event.period
            self.queue.append(event)
            self.year_hashes.append(hash64(self.projection(now - 1)))
            self.census_hashes.append(hash64(self.census(now - 1)))
        elif event.kind == MARKET_ROUND_KIND:
            # Periodic: the next round is in the queue before this one runs.
            event.time += event.period
            self.queue.append(event)
            self._market_round(now, plan)
        elif event.kind == EMIGRATION_KIND:
            # Periodic as well: the next one (the same `K`) is in the queue first.
            assert event.payload == variant(EMIGRATION_VARIANT, varint(self.capacity))
            event.time += event.period
            self.queue.append(event)
            self._emigration(now, plan)
        elif event.kind == IMMIGRATION_KIND:
            # Periodic: the next year's is in the queue before this one runs (design note
            # of the arrival, section 4).
            q = immigration_persons(self.pool_start["n_start"])
            assert event.payload == variant(IMMIGRATION_VARIANT, varint(q)) and now % YEAR == 0
            event.time += event.period
            self.queue.append(event)
            self._immigration(now, plan, q)
        elif event.kind == ATTEMPT_KIND:
            event.time += event.period
            self.queue.append(event)
            self._attempt(event, now, plan)
        elif event.kind == BIRTH_KIND:
            self._birth(event, now, plan)
        elif event.kind == SEPARATION_KIND:
            self._separate(event, now, plan)
        elif event.kind == LEAVE_HOME_KIND:
            self._leave_home(event, now, plan)
        elif event.kind == DISEASE_ONSET_KIND:
            self._disease_onset(event, now, plan)
        elif event.kind == BIRTHDAY_KIND:
            sim = event.subject[1]
            age = (now - self.sims[sim][0]) // YEAR
            subject = variant(0, varint(sim))
            entry = self._entry(now, [subject], [plan], variant(2, varint(age)))
            self._plan_birthday(sim, now + YEAR, entry, now)
            # The 18th birthday: no longer dependent, no carer (no entry, no move).
            if self.sims[sim][11] is not None and not self.dependent(sim, now):
                assert age == 18, (sim, age)
                self._set_carer(sim, None, now, "18th birthday")
        else:
            self._die(event, now, plan)

    def _die(self, event: Event, now: int, plan: bytes) -> None:
        sim = event.subject[1]
        born, key, sex, person, afflictions, conceptions, births, parents, partnerships, household, moved_out, _, social = self.sims.pop(sim)
        codes = [SENESCENCE, ACCIDENT] + [
            affliction.disease.code for affliction in afflictions if affliction.exposure is not None
        ]
        code = next(code for code in codes if event.payload == death_payload(sim, str_key(code)))
        draw = variant(4, varint(DEATH_BUDGET), varint(key), varint(str_key(code)), varint(self.death_detail(sim)))
        death_entry = self._entry(now, [variant(0, varint(sim))], [plan, draw], variant(4, string(code)))
        # Every pending event of the sim goes, its `leave_home` too.
        self.queue = [other for other in self.queue if other.subject != (SIM_SUBJECT, sim)]
        # The partner in the partnership this death ends (the step-parent of section 5).
        partner = None
        for number_, couple_ in self.partnerships.items():
            if couple_[3] is None and sim in couple_[:2]:
                partner = couple_[1] if couple_[0] == sim else couple_[0]
                couple_[3] = (now, ENDED_BY_DEATH)
                # The partnership ends, and with it her series of attempts and its pending
                # separation (design note of the separation, section 5).
                attempts = variant(3, varint(couple_[0]))
                separation = (PARTNERSHIP_SUBJECT, number_)
                self.queue = [
                    other
                    for other in self.queue
                    if other.payload != attempts and not (other.kind == SEPARATION_KIND and other.subject == separation)
                ]
        # Her pregnancy ends with her.
        self.pregnancies.pop(sim, None)
        self.pool_fathers.pop(sim, None)
        # The afflictions stay as they were at the death; the household is the last one; the
        # social parents stay (design note of the households, part 2, section 2).
        self.archive[sim] = (
            born, now, str_key(code), key, sex, person, afflictions, conceptions, births, parents, partnerships,
            household, moved_out, tuple(social),
        )
        if self.households_on:
            # He leaves his household (it ends if he was its last member); then the
            # succession for each dependent child he cared for (design note of the
            # households, sections 4 and 5).
            self.home_log.append((now, "left by death", household, [sim], "death", death_entry))
            self._end_if_empty(household, now)
            self._succession(sim, partner, death_entry, now)

    # ------------------------------------------------------------------ emigration

    def _emigration(self, now: int, plan: bytes) -> None:
        """The event `Emigration` at `now`, the last second of a year (design note of the
        emigration, sections 2 and 3): `P` the living sims; nothing, and no draw, if `P ≤ K`;
        else `e = P / K − 1` (f64, `P` and `K` exact), 0.5 if that is larger, and each
        household that has not ended and has a member of 18 or more draws once at (EMIGRATION,
        the smallest lineage key of those members, the year of `now`, 0); all see the same
        `P`, no draw depends on another. Then in ascending HouseholdId each household that
        drew yes leaves whole (`_emigrate`)."""
        assert now % YEAR == YEAR - 1, now
        k = self.capacity
        p = len(self.sims)
        year = now // YEAR
        record = {"t": now, "year": year, "P": p, "K": k, "e": None, "draws": [], "left": [], "details": []}
        self.emigrations.append(record)
        if p <= k:
            return
        e = emigration_rate(p, k)
        record["e"] = e
        going = []
        for household in sorted(self.households):
            if self.households[household][1] is not None:
                continue
            members = self.members(household)
            adults = [sim for sim in members if self.sims[sim][0] + 18 * YEAR <= now]
            if not adults:
                record["draws"].append({"household": household, "members": members, "adults": [], "draw": None})
                continue
            subject = min(self.key_of(sim) for sim in adults)
            word = words(chacha_key(self.seed, EMIGRATION, subject, year, 0))[0]
            yes = bernoulli(word, e)
            record["draws"].append({"household": household, "members": members, "adults": adults, "subject": subject, "u": unit(word), "draw": yes})
            if yes:
                going.append((household, subject))
        for household, subject in going:
            draw = variant(4, varint(EMIGRATION), varint(subject), varint(year), varint(0))
            members = self.members(household)
            # For the stories (not state): who leaves, their ages and carers, the open
            # partnerships, the pregnancies and the planned events that go with them.
            detail = {
                "household": household,
                "members": [(sim, self.sims[sim][0], now - self.sims[sim][0] >= 18 * YEAR, self.sims[sim][10], self.sims[sim][11]) for sim in members],
                "partnerships": sorted(n for n, c in self.partnerships.items() if c[3] is None and set(c[:2]) <= set(members)),
                "pregnancies": {sim: (self.pregnancies[sim][0], [key for key, _, _ in self.pregnancies[sim][2]], self.pregnancies[sim][3]) for sim in members if sim in self.pregnancies},
                "cancelled": sorted(
                    (event.time, event.kind, event.subject)
                    for event in self.queue
                    if event.subject in [(SIM_SUBJECT, sim) for sim in members]
                    or (event.kind == SEPARATION_KIND and event.subject[1] in [n for n, c in self.partnerships.items() if c[3] is None and set(c[:2]) <= set(members)])
                ),
            }
            record["details"].append(detail)
            record["left"].append((household, self._emigrate(household, now, [plan, variant(3, string(CAPACITY_RULE)), draw])))

    def _emigrate(self, household: int, now: int, causes: list) -> list:
        """A household that leaves at `now` (design note of the emigration, section 3): per
        member in ascending SimId one entry `core:emigrated { household }` (subject the sim,
        `causes`), then the member goes to the archive with `LeftBy::Emigration` at `now` (its
        afflictions as they are, its household this one), every event planned for it goes,
        its open partnership (whose partner lives here too) ends with `EndedBy::Emigration`
        and loses its separation, her pregnancy goes with her. The household ends at `now`.
        Nobody left behind changes household, carer or social parents. Returns the members
        with their entries."""
        members = self.members(household)
        assert members
        # Nobody outside is cared for by a member, and every open partnership of a member has
        # both partners here (ADR-0018).
        assert not [sim for sim in self.sims if sim not in members and self.sims[sim][11] in members]
        for number_, couple_ in self.partnerships.items():
            if couple_[3] is None and set(couple_[:2]) & set(members):
                assert set(couple_[:2]) <= set(members), (number_, couple_, members)
        left = []
        for sim in members:
            entry = self._entry(now, [variant(0, varint(sim))], causes, variant(EMIGRATED_VARIANT, varint(household)))
            born, key, sex, person, afflictions, conceptions, births, parents, partnerships, home, moved_out, _, social = self.sims.pop(sim)
            assert home == household
            self.queue = [other for other in self.queue if other.subject != (SIM_SUBJECT, sim)]
            for number_, couple_ in self.partnerships.items():
                if couple_[3] is None and sim in couple_[:2]:
                    couple_[3] = (now, ENDED_BY_EMIGRATION)
                    self.queue = [other for other in self.queue if not (other.kind == SEPARATION_KIND and other.subject == (PARTNERSHIP_SUBJECT, number_))]
            # Her pregnancy goes with her, its children are never born (her conception stays
            # counted), as at her death.
            self.pregnancies.pop(sim, None)
            self.pool_fathers.pop(sim, None)
            self.archive[sim] = (
                born, now, None, key, sex, person, afflictions, conceptions, births, parents, partnerships,
                household, moved_out, tuple(social),
            )
            self.home_log.append((now, "left by emigration", household, [sim], "emigration", entry))
            left.append((sim, entry))
        self._end_if_empty(household, now)
        assert self.households[household][1] == now
        return left

    def left_by(self, sim: int) -> str:
        """"death" or "emigration" for an archived sim."""
        return "emigration" if self.archive[sim][2] is None else "death"

    # ------------------------------------------------------------------ views

    def ordered_queue(self) -> list:
        return sorted(self.queue, key=Event.key)

    def projection(self, closed=None) -> bytes:
        closed = self.closed if closed is None else closed
        # Without households (the switch of this reference) the fields hold 0, false, none.
        sims = [
            varint(sim)
            + signed(born)
            + lineage_key(key)
            + sex
            + genome_of(person)
            + afflictions_bytes([affliction.triple() for affliction in afflictions])
            + varint(conceptions)
            + varint(births)
            + varint(partnerships)
            + varint(household or 0)
            + flag(moved_out)
            + (NONE if carer is None else some(varint(carer)))
            + parents_bytes(parents)
            + social_parents_bytes(social)
            + pool_origin_bytes(self.origins.get(sim))
            for sim, (born, key, sex, person, afflictions, conceptions, births, parents, partnerships, household, moved_out, carer, social) in sorted(
                self.sims.items()
            )
        ]
        archive = [
            varint(sim)
            + signed(born)
            + signed(left)
            + (variant(LEFT_BY_EMIGRATION) if cause is None else variant(LEFT_BY_DEATH, varint(cause)))
            + lineage_key(key)
            + sex
            + genome_of(person)
            + afflictions_bytes([affliction.triple() for affliction in afflictions])
            + varint(conceptions)
            + varint(births)
            + varint(partnerships)
            + varint(household or 0)
            + flag(moved_out)
            + parents_bytes(parents)
            + social_parents_bytes(social)
            + pool_origin_bytes(self.origins.get(sim))
            for sim, (born, left, cause, key, sex, person, afflictions, conceptions, births, parents, partnerships, household, moved_out, social) in sorted(
                self.archive.items()
            )
        ]
        households = [
            varint(number_) + household_bytes(formed, ended) for number_, (formed, ended) in sorted(self.households.items())
        ]
        pregnancies = [
            varint(mother) + pregnancy_bytes(*value, pool_father=self.pool_fathers.get(mother)) for mother, value in sorted(self.pregnancies.items())
        ]
        return (
            varint(PROJECTION_VERSION)
            + some(varint(self.seed))
            + some(world_params(self.switch, self.nutrition))
            + signed(closed)
            + varint(self.next_sim_id)
            + varint(self.next_partnership_id)
            + varint(self.next_household_id)
            + seq([event.queue_bytes() for event in self.ordered_queue()])
            + varint(self.next_insertion)
            + seq(sims)
            + seq([varint(n) + partnership(*value) for n, value in sorted(self.partnerships.items())])
            + seq(households)
            + seq(pregnancies)
            + seq(archive)
            + used_households_bytes(self.used)
        )

    def census(self, closed=None) -> bytes:
        """The census of the null twin (test rule 8; design note of the market, section 7):
        the projection without the queue and its next insertion number, so the domain
        state only (seed, switch, clock, the counters of SimId, PartnershipId and
        HouseholdId, sims, partnerships, households, pregnancies, archive). A round that
        changes nothing changes none of it."""
        projection = self.projection(closed)
        queue = seq([event.queue_bytes() for event in self.ordered_queue()]) + varint(self.next_insertion)
        assert projection.count(queue) == 1
        return projection.replace(queue, b"")

    def state_hash(self) -> int:
        return hash64(self.projection())

    def log_hash(self) -> int:
        return hash64(b"".join(self.log))

    def save(self) -> bytes:
        """Header, projection, exceptions (ADR-0006 section 2)."""
        plans = [varint(event.plan[0]) + signed(event.plan[1]) for event in self.ordered_queue()]
        years = [signed(year) + varint(value) for year, value in enumerate(self.year_hashes)]
        return (
            header(self.data.hash)
            + self.projection()
            + seq(plans)
            + varint(self.next_event_id)
            + seq(self.inputs)
            + seq(years)
        )

    # ------------------------------------------------------------------ the pedigree

    def switch_at(self, r: int) -> bool:
        """The switch of the hereditary diseases at the second `r >= 0` by the input log
        (design note of the pedigree, section 1): the value of `Start` from 0, each override
        block from one second after its stamp (an override stamped at the clock is in the
        state already, but holds only from the next second)."""
        assert r >= 0
        return [value for since, value in self.switch_history if since <= r][-1]

    def cause_code(self, key: int) -> str:
        """The data ID of a cause of death from its `str_key` (ADR-0004 section 2: the export
        writes the string): `core:senescence`, `core:accident` or a disease of the data."""
        codes = {str_key(code): code for code in [SENESCENCE, ACCIDENT] + [disease.code for disease in self.data.diseases]}
        return codes[key]

    def pedigree_view(self) -> dict:
        """The pedigree of the state (design note of the pedigree, sections 1 to 3 and 5), as
        the object `pedigree.json` holds, keys in the order of the note: `format_version` 4
        (2 before the emigration, 3 before the arrival; 4 adds `pool` as the last key of each
        person, `pool_view`),
        then `persons` by SimId (living and archived together), `partnerships` by
        PartnershipId, `households` by HouseholdId.

        A person: `sim`, `lineage_key` (16 lowercase hex digits, ADR-0004 section 3), `sex`
        ("female", "male"), `alleles` (the founders' form: per locus with an allele other
        than the reference, `{locus, copies}` with the copies maternal then paternal, the
        loci in the order of the genome map's file), `born`, `died` (or null), `cause_of_death`
        (its data ID, or null), `emigrated` (the second it emigrated, or null; design note of
        the emigration, section 4: an emigrant has `died` and `cause_of_death` null, its last
        household, carer null and its diseases at the second it left), `mother`, `father`
        (or null for a founder), then
        `social_parents` (the list of the state, in its order), `household` (the last one
        for the dead), `moved_out`, `carer` (null for the dead and anyone without one),
        `diseases` (per affliction in data order `{disease, onset, affected}`: `affected`
        iff at `r` the age `(r − born) / 31 449 600` (f64) is at least the onset and the
        switch is on at `r`; `r` is the later of the clock and 0 for the living, the second
        of the death for the dead). A partnership: `partnership`, `woman`, `man`, `formed`,
        `ended` (or null), `ended_by` ("death", "separation", "emigration" or null), `kinship` (`φ` of
        the two, f64). A household: `household`, `formed`, `ended` (or null)."""
        assert self.households_on, "the pedigree of a world with households"
        # The loci in the order of the genome map's file (as the export walked them since
        # step 5; ADR-0007 section 3: the state names definitions by their data IDs).
        loci = list(self.data.loci)
        persons = []
        for sim in sorted(set(self.sims) | set(self.archive)):
            if sim in self.sims:
                born, key, sex, person, afflictions, _, _, parents, _, household, moved_out, carer, social = self.sims[sim]
                died, cause, r = None, None, max(self.closed, 0)
            else:
                born, died, cause_key, key, sex, person, afflictions, _, _, parents, _, household, moved_out, social = self.archive[sim]
                cause, carer, r = (None, None, died) if cause_key is None else (self.cause_code(cause_key), None, died)
            assert isinstance(moved_out, bool) and household is not None, (sim, moved_out, household)
            alleles = []
            for locus in loci:
                copies = copies_at(person, self.data.loci[locus])
                if any(copies):
                    alleles.append({"locus": locus, "copies": list(copies)})
            # An emigrant (format 3): `died` and `cause_of_death` null, `emigrated` the second
            # it left, its diseases as at that second.
            emigrated = None
            if sim in self.archive and cause_key is None:
                emigrated, died = died, None
            switch = self.switch_at(r)
            diseases = [
                {
                    "disease": affliction.disease.code,
                    "onset": affliction.onset,
                    "affected": as_years(r - born) >= affliction.onset and switch,
                }
                for affliction in afflictions
            ]
            persons.append(
                {
                    "sim": sim,
                    "lineage_key": f"{key:016x}",
                    "sex": "female" if sex == FEMALE else "male",
                    "alleles": alleles,
                    "born": born,
                    "died": died,
                    "cause_of_death": cause,
                    "emigrated": emigrated,
                    "mother": None if parents is None else parents[0],
                    "father": None if parents is None else parents[1],
                    "social_parents": list(social),
                    "household": household,
                    "moved_out": moved_out,
                    "carer": carer,
                    "diseases": diseases,
                    "pool": self.pool_view(sim),
                }
            )
        partnerships = [
            {
                "partnership": number_,
                "woman": woman,
                "man": man,
                "formed": formed,
                "ended": None if ended is None else ended[0],
                "ended_by": None if ended is None else ENDED_BY_NAMES[ended[1]],
                "kinship": self.phi_float(woman, man),
            }
            for number_, (woman, man, formed, ended) in sorted(self.partnerships.items())
        ]
        households = [
            {"household": number_, "formed": formed, "ended": ended}
            for number_, (formed, ended) in sorted(self.households.items())
        ]
        return {"format_version": PEDIGREE_FORMAT_VERSION, "persons": persons, "partnerships": partnerships, "households": households}

    def pool_view(self, sim: int):
        """The `pool` of a person in `pedigree.json` format 4 (design note of the arrival,
        section 8): null for a town person, else `{genome_key, born_pool, arrived,
        arrival_year, mother_key, father_key, social_parent_keys}` with the keys as 16
        lowercase hex digits (null where None)."""
        origin = self.origins.get(sim)
        if origin is None:
            return None
        hexed = lambda key: None if key is None else f"{key:016x}"
        return {
            "genome_key": f"{origin['genome_key']:016x}",
            "born_pool": origin["born_pool"],
            "arrived": origin["arrived"],
            "arrival_year": origin["arrival_year"],
            "mother_key": hexed(origin["mother"]),
            "father_key": hexed(origin["father"]),
            "social_parent_keys": [hexed(key) for key in origin["social_parents"]],
        }

    def pedigree_json(self) -> str:
        """The text of `pedigree.json` as serde_json writes it (`json_text`)."""
        return json_text(self.pedigree_view())


def json_number(value: float) -> str:
    """An f64 as serde_json 1.0.151 writes it (its formatter `zmij` 1.0.23, the same digits
    and layout as `ryu` except a `+` in a positive exponent): the shortest digits that read
    back to the same f64 (Python's repr gives them), then with the decimal exponent `e` of
    the first digit (`d.ddd · 10^e`): for `−5 ≤ e ≤ 15` plain notation, `1234.0`,
    `12.34` or `0.001234` (a whole number gets `.0`); else `1.234e-7`, `1e-7` (one digit,
    no point), `1.5e+16` (sign `+` for `e ≥ 0`, no zeros before the exponent's digits).
    `0.0`, `-0.0`; never NaN or an infinity here."""
    assert math.isfinite(value), value
    if value == 0.0:
        return "-0.0" if math.copysign(1.0, value) < 0.0 else "0.0"
    sign = "-" if value < 0.0 else ""
    mantissa, _, exponent = repr(abs(value)).partition("e")
    whole, _, fraction = mantissa.partition(".")
    digits = (whole + fraction).lstrip("0")
    k = (int(exponent) if exponent else 0) - len(fraction)
    stripped = digits.rstrip("0")
    k += len(digits) - len(stripped)
    digits = stripped
    assert digits and float(f"{digits}e{k}") == abs(value), (value, digits, k)
    length = len(digits)
    e = length + k - 1
    if -5 <= e <= 15:
        if k >= 0:
            text = digits + "0" * k + ".0"
        elif e >= 0:
            text = digits[: e + 1] + "." + digits[e + 1 :]
        else:
            text = "0." + "0" * (-e - 1) + digits
    else:
        head = digits[0] + ("." + digits[1:] if length > 1 else "")
        text = head + "e" + ("+" if e >= 0 else "-") + str(abs(e))
    assert float(text) == abs(value), (value, text)
    return sign + text


# The writer of the exponent and its thresholds (zmij 1.0.23 `FIXED_DEC_EXP` -5..=15, its tests
# and its comparison with ryu), and dyadic fractions of the kinship.
for _value, _text in [
    (1.0, "1.0"), (0.0, "0.0"), (-0.0, "-0.0"), (0.25, "0.25"), (0.0625, "0.0625"), (0.53125, "0.53125"),
    (2.0**-14, "0.00006103515625"), (1e-5, "0.00001"), (2.0**-17, "7.62939453125e-6"), (1e-6, "1e-6"),
    (2.9802322387695312e-08, "2.9802322387695312e-8"), (5.960464477539062e-07, "5.960464477539062e-7"),
    (6.62607015e-34, "6.62607015e-34"), (5.444310685350916e14, "544431068535091.6"), (43210.0, "43210.0"),
    (10000.0, "10000.0"), (-5942736479622170.0, "-5942736479622170.0"), (1e16, "1e+16"),
    (3.439070283483335e35, "3.439070283483335e+35"), (48.02016537979952, "48.02016537979952"), (5e-324, "5e-324"),
]:
    assert json_number(_value) == _text, (_value, json_number(_value), _text)


def struct_bits(value: float) -> str:
    """The 16 hex digits of the f64 bits."""
    import struct

    return struct.pack(">d", value).hex()


def json_text(value) -> str:
    """Compact JSON as serde_json's `to_string` writes it: no spaces, the keys of an object
    in the order given, integers as integers, f64 by `json_number`, `null`, `true`,
    `false`; strings here are data IDs, hex digits and names (printable ASCII or beyond
    ASCII, as serde_json writes them unescaped; no quote, backslash or control character,
    so no escapes)."""
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, int):
        assert abs(value) <= 2**53 - 1, value
        return str(value)
    if isinstance(value, float):
        return json_number(value)
    if isinstance(value, str):
        assert all((" " <= char <= "~" or ord(char) > 0x9F) and char not in '"\\' for char in value), value
        return f'"{value}"'
    if isinstance(value, list):
        return "[" + ",".join(json_text(item) for item in value) + "]"
    assert isinstance(value, dict), value
    return "{" + ",".join(f"{json_text(name)}:{json_text(item)}" for name, item in value.items()) + "}"


def emigration_rate(p: int, k: int) -> float:
    """`e` of the emigration (design note of the emigration, section 2) for `P > K`: `P / K
    − 1` with `P` and `K` converted exactly to f64 (one division, one subtraction, both
    correctly rounded), and 0.5 if that is larger (a comparison, no `min`)."""
    assert p > k >= 1 and p <= 2**53 and k <= 2**53
    e = float(p) / float(k) - 1.0
    if e > EMIGRATION_MAX_RATE:
        e = EMIGRATION_MAX_RATE
    return e


# The unit values of the design note (section 6), by hand: at `P = K + 1` the rate is `1 / K`
# up to the rounding of `(K + 1) / K`; at `P = 1.5 K` exactly 0.5; at `P = 3 K` 2, held at 0.5.
assert emigration_rate(11, 10) == 11.0 / 10.0 - 1.0 and abs(emigration_rate(11, 10) - 0.1) < 1e-15
assert emigration_rate(15, 10) == 0.5 and emigration_rate(30, 10) == 0.5 and emigration_rate(16, 10) == 0.5
assert emigration_rate(14, 10) == 14.0 / 10.0 - 1.0 < 0.5 and emigration_rate(2, 1) == 0.5


def kinship_by_keys(x, y, kin, depth=KINSHIP_DEPTH) -> tuple:
    """`(φ, S, visited)` of the genetics specification 2.7 with the depth of H2 (ADR-0020
    section 4), generic in its key (design note of the arrival, section 7: `kinship.rs`
    generic in `K: Ord + Copy`): `kin(key)` gives `(order, mother, father)`, each parent
    on its own (None if unknown); the younger of two is the greater `(order, key)` (the
    larger key the younger at a tie); a parent's order is strictly smaller than its child's
    (`parent_not_older`, asserted). The set `S` of the query holds `x`, `y` and every
    ancestor of either up to `depth` generations; within the query a parent counts iff it
    is in `S`; the recursion of `spec_reference.Pedigree.query` otherwise."""
    looked = {}

    def entry(key):
        if key not in looked:
            looked[key] = kin(key)
        return looked[key]

    def parents(key) -> tuple:
        return entry(key)[1:]

    def ancestors(key) -> set:
        found, frontier, generation = set(), {key}, 0
        while frontier and (depth is None or generation < depth):
            frontier = {parent for person in frontier for parent in parents(person) if parent is not None}
            found |= frontier
            generation += 1
        return found

    s = {x, y} | ancestors(x) | ancestors(y)
    for person in s:
        for parent in parents(person):
            assert parent is None or entry(parent)[0] < entry(person)[0], "parent_not_older"
    memo = {}

    def known(person) -> tuple:
        return tuple(parent if parent is not None and parent in s else None for parent in parents(person))

    def rec(a, b):
        if (entry(a)[0], a) < (entry(b)[0], b):
            a, b = b, a
        if (a, b) in memo:
            return memo[(a, b)]
        mother, father = known(a)
        if a == b:
            inbreeding = rec(mother, father) if mother is not None and father is not None else Fraction(0)
            value = (1 + inbreeding) / 2
        else:
            value = sum((rec(parent, b) for parent in (mother, father) if parent is not None), Fraction(0)) / 2
        memo[(a, b)] = value
        return value

    value = rec(x, y)
    return value, s, set(memo)


def first_birthday(born: int, now: int) -> int:
    """The first birthday of a person who comes into the world at `now` (a founder at the
    start; an arrival at `A`, design note of the arrival, section 5: `first_birthday(born,
    A)`): the first anniversary at or after `now`, at least the first."""
    whole = (now - born) // YEAR
    on_the_day = born + whole * YEAR == now
    age = whole if on_the_day and whole >= 1 else whole + 1
    return born + age * YEAR


def immigration_persons(n_start: int) -> int:
    """`q = (n_start + 50) div 100` (design note of the arrival, section 4)."""
    return (n_start + 50) // 100


def pool_order(pool: DataPool, seed: int, occasion: int, used: list) -> list:
    """The households of the pool not in `used`, as indices into the file, ascending by
    `(rank, candidate)` with `rank = rank(seed, POOL_RANK, SubjectKey::WORLD, occasion,
    candidate)` and the candidate the smallest lineage key of a member of 18 or more at
    `E_p` (design note of the arrival, section 3; ADR-0001 section 5)."""
    free = [index for index in range(len(pool.households)) if index not in used]
    return sorted(free, key=lambda index: (rank(seed, POOL_RANK, WORLD_KEY, occasion, pool.candidates[index]), pool.candidates[index]))


def pool_draw(pool: DataPool, seed: int, occasion: int, number: int, used: list) -> list:
    """The households taken in the order of `pool_order` until the number of their members
    (unborn children do not count) reaches or exceeds `number`; all of them if they cannot."""
    taken, members = [], 0
    for index in pool_order(pool, seed, occasion, used):
        if members >= number:
            break
        taken.append(index)
        members += pool.sizes[index]
    return taken


def village_candidates(pool: DataPool, sex: int) -> list:
    """The candidates of the village of one sex (0 a woman, 1 a man; design note of the
    village, section 6): the lineage keys of the pool's members (not its unborn children) of
    that sex whose age at the freeze is from their 18th birthday to before their 41st,
    `18 · YEAR <= E_p − born < 41 · YEAR`, in file order."""
    low, high = VILLAGE_AGES
    return [
        key for key, (_, member) in pool.members.items() if member["sex"] == sex and low * YEAR <= pool.frozen_at - member["born"] < high * YEAR
    ]


def village_draw(pool: DataPool, seed: int, n_start: int):
    """The persons of the village (design note of the village, section 6): `(women, men)`, per
    sex the `n_start / 2` candidates with the smallest `(rank, lineage key)`, `rank = rank(seed,
    POOL_RANK, SubjectKey::WORLD, VILLAGE_OCCASION, lineage key)` (ADR-0001 section 5: a tie
    of ranks the smaller key first), each list in that order; None if a sex has fewer
    candidates (`start_pool_village_candidates`). `n_start` even."""
    assert n_start % 2 == 0
    half = n_start // 2
    chosen = []
    for sex in (0, 1):
        ordered = sorted(village_candidates(pool, sex), key=lambda key: (rank(seed, POOL_RANK, WORLD_KEY, VILLAGE_OCCASION, key), key))
        if len(ordered) < half:
            return None
        chosen.append(ordered[:half])
    return chosen[0], chosen[1]


def village_person(member: dict) -> dict:
    """A chosen person as it arrives in the village (design note of the village, section 6;
    Grok V1 (a), V2 (a)): the pool's record with its carried plans struck (`conception_at`,
    `birth_at`, `separation_at`, `leave_home_at` None) and `moved_out` true; everything else
    as the file holds it (genome, birth, keys, the counts `conceptions`, `births` and
    `partnerships`, the social parents' keys, the afflictions). The carer is struck as well
    (`carer` None; decision of the coordinator, 2026-10-19: a single household holds no carer;
    the repository's pool has no member of 18 or more with one, the 18th birthday ends it)."""
    person = copy.deepcopy(member)
    for name in ("conception_at", "birth_at", "separation_at", "leave_home_at", "carer"):
        person[name] = None
    person["moved_out"] = True
    return person


def village_households(pool: DataPool, seed: int, n_start: int):
    """The households of `PoolStart` in the village mode (design note of the village, section
    6): one per chosen person, members `[village_person]`, no partnership, no pregnancies; the
    women in rank order, then the men in rank order (the order is the reference's decision,
    the note leaves it open). None if a sex has too few candidates."""
    drawn = village_draw(pool, seed, n_start)
    if drawn is None:
        return None
    women, men = drawn
    return [{"members": [village_person(pool.members[key][1])], "partnership": None, "pregnancies": []} for key in women + men]


def pool_start(data: Data, seed: int, n_start: int, immigration: bool, village: bool = False) -> dict:
    """The `pool` of `Start` the CLI fills in from the scenario's `{n_start, immigration,
    village}` and the data (design note of the arrival, section 3): the pool's hash and the
    drawn households with occasion 0, each as the file holds it; with `village` the single
    households of the chosen persons (`village_households`, design note of the village)."""
    assert data.pool is not None
    if village:
        households = village_households(data.pool, seed, n_start)
        assert households is not None, "start_pool_village_candidates"
    else:
        households = [data.pool.households[i] for i in pool_draw(data.pool, seed, 0, n_start, [])]
    return {"pool_hash": data.pool.hash, "n_start": n_start, "immigration": immigration, "village": village, "households": households}


def pool_start_bytes(pool) -> bytes:
    """`pool: Option<PoolStart>`, the last field of `Start` (after `capacity`): `PoolStart {
    pool_hash: u64, n_start: u64, immigration: bool, village: bool, households:
    Vec<PoolHousehold> }`, each household as the file writes it
    (`pool_reference.encode_household`); `village` (design note of the village, section 6)
    a bool byte after `immigration`, false where the dict names none."""
    if pool is None:
        return NONE
    import pool_reference

    assert 0 <= pool["pool_hash"] < 2**64 and 0 <= pool["n_start"] < 2**64
    return some(
        varint(pool["pool_hash"])
        + varint(pool["n_start"])
        + flag(pool["immigration"])
        + flag(pool.get("village", False))
        + seq([pool_reference.encode_household(household) for household in pool["households"]])
    )


def start_rejection(founders: list, couples, data: Data, capacity=None, pool=None, seed=None):
    """The reject code of the couples of `Start`, or None (design note of the market,
    section 4): a couple naming no founder (`unknown_founder`), of the wrong sexes, a
    founder in two couples (`already_partnered`), then a founder of a couple under the
    market age at the start (second 0: `0 − born < 16 · YEAR`, `partner_too_young`).
    Founders are unrelated (φ 0). Then a capacity of 0 (`capacity_zero`, design note of
    the emigration, section 1). Then the pool, the last field of `Start` (design note of the
    arrival, section 3), the first that applies: `founders` or `couples` not empty
    (`start_pool_and_founders`); no pool in the data or `pool_hash` not its hash
    (`start_pool_missing`); `n_start` 0 (`start_pool_n_start`); with `village` (design note of
    the village, section 6): `n_start` odd (`start_pool_village_n_start`), immigration on
    (`start_pool_village_immigration`), a sex with fewer candidates than `n_start / 2`
    (`start_pool_village_candidates`); `households` not the draw of `pool_start` with the
    world seed `seed` (`start_pool_households`)."""
    paired = set()
    for woman, man in couples:
        if not (0 <= woman < len(founders) and 0 <= man < len(founders)):
            return UNKNOWN_FOUNDER
        if founders[woman][1] != FEMALE or founders[man][1] != MALE:
            return PARTNERS_OF_WRONG_SEX
        if woman in paired or man in paired:
            return ALREADY_PARTNERED
        paired.update((woman, man))
    for woman, man in couples:
        if any(0 - founders[index][0] < data.market_age * YEAR for index in (woman, man)):
            return PARTNER_TOO_YOUNG
    if capacity is not None:
        assert 0 <= capacity < 2**64
        if capacity == 0:
            return CAPACITY_ZERO
    if pool is not None:
        if founders or couples:
            return START_POOL_AND_FOUNDERS
        if data.pool is None or pool["pool_hash"] != data.pool.hash:
            return START_POOL_MISSING
        if pool["n_start"] == 0:
            return START_POOL_N_START
        import pool_reference

        if pool.get("village", False):
            if pool["n_start"] % 2 != 0:
                return START_POOL_VILLAGE_N_START
            if pool["immigration"]:
                return START_POOL_VILLAGE_IMMIGRATION
            drawn = village_households(data.pool, seed, pool["n_start"])
            if drawn is None:
                return START_POOL_VILLAGE_CANDIDATES
            want = [pool_reference.encode_household(household) for household in drawn]
        else:
            want = [data.pool.encoded[i] for i in pool_draw(data.pool, seed, 0, pool["n_start"], [])]
        if [pool_reference.encode_household(household) for household in pool["households"]] != want:
            return START_POOL_HOUSEHOLDS
    return None


def check_afflictions(world: World) -> None:
    """The load check of the afflictions (ADR-0017 section 5; design note of the threshold
    disease, section 3): every living or archived sim holds exactly the afflictions its
    genotype, its sex and the draws give at its creation, in data order with their onsets
    (`World.afflictions_of`); for a threshold disease with a trait of liability that is
    `L > T`. A save that breaks it is `Inconsistent` in the core, an assertion here."""
    for sim in sorted(set(world.sims) | set(world.archive)):
        key, sex, person, parents, origin, age, held = world.creation_of(sim)
        got = [(affliction.disease.code, affliction.onset) for affliction in held]
        if world.is_pool_person(sim):
            # A pool person holds the pool's afflictions (design note of the arrival, sections
            # 6.1 and 6.3), not drawn again.
            record = world.pool_member(sim) if world.is_arrival(sim) else world.data.pool.unborn[key][2]
            want = [(world.cause_code(disease), onset) for disease, onset in record["afflictions"]]
            assert got == want, f"sim {sim}: afflictions {got}"
            continue
        e_seed, c_seed, env_parents = world.env_of(sim)
        assert e_seed == world.seed and env_parents == parents
        want = world.afflictions_of(key, sex, person, origin, parents, age, c_seed)
        assert got == [(affliction.disease.code, affliction.onset) for affliction in want], f"sim {sim}: afflictions {got}"


def check_market(world: World) -> None:
    """The load checks of the market (design note of the market, section 6; ADR-0018
    section 7; ADR-0006 section 5) on the state of `world`, as the loader would see its save
    with its input log; a broken one is `Inconsistent` there, an assertion here:

    - exactly one `market_round` series: period MONTH, at the first month start after the
      clock (after the start: `max(clock, 0)`, so MONTH right after Start), subject the
      world, planned by `core:started` at 0;
    - each partnership was formed by an accepted input (a couple of Start at 0, an
      ArrangePartnership in the second after its stamp) or in a round second (`formed` a
      positive multiple of MONTH) by the market; in one second the partnerships of inputs
      come before those of the round (PartnershipId order);
    - both were single then: an earlier partnership of either ended before `formed`;
    - both were at least 16 at `formed`, and `φ < 1/8`."""
    rounds = [event for event in world.queue if event.kind == MARKET_ROUND_KIND]
    if world.market:
        assert len(rounds) == 1, rounds
        (series,) = rounds
        assert series.time == month_start_after(max(world.closed, 0)) and series.period == MONTH
        assert series.subject == (WORLD_SUBJECT, 0) and series.cls == GENERAL and series.payload == MARKET_ROUND_PAYLOAD
        assert series.plan == (1, 0)
    else:
        assert not rounds
    pending = list(world.input_pairings)
    origin = {}
    for number_, (woman, man, formed, _) in sorted(world.partnerships.items()):
        if pending and pending[0] == (formed, woman, man):
            pending.pop(0)
            origin[number_] = "input"
        elif world.origins and world.arrival_partnership(number_) is not None:
            # Brought by an arrival (design note of the arrival, section 9): formed at
            # `formed_pool + Δ`, possibly before second 0, both partners arrivals of one
            # household.
            origin[number_] = "arrival"
        else:
            assert formed > 0 and formed % MONTH == 0, (number_, formed)
            origin[number_] = "market"
        assert all(formed - world.born_of(sim) >= world.data.market_age * YEAR for sim in (woman, man)), number_
        assert world.kinship(woman, man) < Fraction(world.data.kinship_limit), number_
        assert world.sex_of(woman) == FEMALE and world.sex_of(man) == MALE
    assert not pending, pending
    numbers = sorted(world.partnerships)
    for first in numbers:
        for later in numbers:
            if later <= first:
                continue
            a, b = world.partnerships[first], world.partnerships[later]
            if a[2] == b[2] and "arrival" not in (origin[first], origin[later]):
                assert not (origin[first] == "market" and origin[later] == "input"), (first, later)
            if set(a[:2]) & set(b[:2]):
                assert a[3] is not None and a[3][0] < b[2], (first, later)


def robust_seconds(seconds_of, dims: int = 3):
    """The seconds of a death that do not hang on the last bit of libm: `seconds_of(nudge)`
    gives the same with the budget, exp and ln moved by one ulp either way (and, with `dims`
    4, the exp of the multiplier `m` of a trait of longevity), so the pinned libm of the
    core (within an ulp of the correctly rounded value) gives this second."""
    nominal = seconds_of((0,) * dims)
    for nudge in itertools.product((-1, 0, 1), repeat=dims):
        assert seconds_of(nudge) == nominal, f"a due second hangs on the last bit: {nudge}"
    return nominal


def check_separations(world: World) -> None:
    """The load checks of the separation (design note of the separation, section 6;
    ADR-0018 section 7; ADR-0006 section 5) on the state of `world`, as the loader would
    see its save; a broken one is `Inconsistent` there, an assertion here:

    - `partnerships` of each woman (living or archived) is the number of partnerships whose
      woman she is; a man's is 0;
    - per partnership, with `due` as in section 3 (`k` from the partnerships of the same
      woman with a smaller PartnershipId): the first of {`due` if `due` <= the clock, the
      first death of the two, the emigration of either (design note of the emigration,
      section 3)} decides, a tie going to the separation, then to the death (the classes
      500, 900, 950 of one second); `ended` is that Ending, or None if none has come;
    - a pending `separation` exists exactly for each open partnership with a `due`, at
      `due`, subject that partnership; none else;
    - a series of attempts needs an open partnership (the check that stays)."""
    people = {sim: (value[2], value[8]) for sim, value in world.sims.items()}
    people.update({sim: (value[4], value[10]) for sim, value in world.archive.items()})
    for sim, (sex, count) in people.items():
        mine = sum(1 for value in world.partnerships.values() if value[0] == sim)
        # An arrival's count is the pool's plus the town's (design note of the arrival,
        # section 9): the pool's count already holds the partnership she brought.
        base = world.pool_partnerships_before(sim) if world.is_arrival(sim) else 0
        assert count == ((base + mine) if sex == FEMALE else 0) and (sex == FEMALE or mine == 0), (sim, count, mine, base)
    pending = [event for event in world.queue if event.kind == SEPARATION_KIND]
    for number_, (woman, man, formed, ended) in world.partnerships.items():
        due = world.separation_due(number_)
        # (time, order in the second, Ending): separation (class 500), death (900),
        # emigration (950).
        ends = []
        if due is not None and due <= world.closed:
            ends.append((due, 0, ENDED_BY_SEPARATION))
        for sim in (woman, man):
            if sim in world.archive:
                emigrated = world.archive[sim][2] is None
                ends.append((world.archive[sim][1], 2 if emigrated else 1, ENDED_BY_EMIGRATION if emigrated else ENDED_BY_DEATH))
        want = (min(ends)[0], min(ends)[2]) if ends else None
        assert ended == want, (number_, ended, want)
        events = [event for event in pending if event.subject == (PARTNERSHIP_SUBJECT, number_)]
        if ended is None and due is not None:
            assert [(e.time, e.payload) for e in events] == [(due, variant(5, varint(number_)))], number_
        else:
            assert not events, number_
    assert all(event.subject[0] == PARTNERSHIP_SUBJECT and event.subject[1] in world.partnerships for event in pending)
    for event in world.queue:
        if event.kind == ATTEMPT_KIND:
            assert world.partner_of(event.subject[1]) is not None, event.subject


def check_emigration(world: World) -> None:
    """The load checks of the emigration (design note of the emigration, section 4; ADR-0020
    section 6; ADR-0006 section 5) on the state of `world`, as the loader would see its save
    with its input log (the capacity of `Start`); a broken one is `Inconsistent` there, an
    assertion here:

    - with a capacity exactly one event `Emigration { capacity: K }` is planned (class 950,
      subject the world, kind 9, period YEAR, planned by `core:started` at 0) in the last
      second of the current year, or of the next if the clock stands at or after this
      year's (the smallest `n · YEAR − 1` after the clock); without one none, and nobody in
      the archive left by emigration;
    - every archived sim with `LeftBy::Emigration` left in the last second of a year, at or
      after the first year end (`YEAR − 1`) and at or before the clock, in a world with a
      capacity; the emigrants of one household and
      second are everyone who lived in that household in that second (no living sim lives
      there, and the household ended in that very second), at least one of them 18 or more
      (`born + 18 · YEAR ≤ t`); no event is planned for any of them and none is pregnant;
    - a partnership ended by `Emigration` has both partners among the emigrants of one
      household and second, and ended in that second."""
    events = [event for event in world.queue if event.kind == EMIGRATION_KIND]
    emigrants = {sim: value for sim, value in world.archive.items() if value[2] is None}
    if world.capacity is None:
        assert not events and not emigrants, (events, sorted(emigrants))
    else:
        assert len(events) == 1, events
        (event,) = events
        due = (world.closed // YEAR + 1) * YEAR - 1
        if due <= world.closed:
            due += YEAR
        assert event.time == due and event.time > world.closed, (event.time, world.closed)
        assert event.cls == EMIGRATION_CLASS and event.subject == (WORLD_SUBJECT, 0) and event.period == YEAR
        assert event.payload == variant(EMIGRATION_VARIANT, varint(world.capacity)) and event.plan == (1, 0)
    groups = {}
    for sim, value in emigrants.items():
        groups.setdefault((value[11], value[1]), []).append(sim)
    for (household, t), sims in groups.items():
        # The last second of a year, the first year end (YEAR − 1) or later: −1 is congruent
        # to a year's last second but lies before the start.
        assert t % YEAR == YEAR - 1 and YEAR - 1 <= t <= world.closed, (sims, t)
        assert household in world.households and world.households[household][1] == t, (household, t)
        assert not [sim for sim, value in world.sims.items() if value[9] == household], household
        assert any(world.archive[sim][0] + 18 * YEAR <= t for sim in sims), (household, sims)
        for sim in sims:
            assert not [event for event in world.queue if event.subject == (SIM_SUBJECT, sim)], sim
            assert sim not in world.pregnancies, sim
    for number_, (woman, man, _, ended) in world.partnerships.items():
        if ended is not None and ended[1] == ENDED_BY_EMIGRATION:
            assert woman in emigrants and man in emigrants, number_
            assert emigrants[woman][1] == emigrants[man][1] == ended[0], number_
            assert emigrants[woman][11] == emigrants[man][11], number_


def check_households(world: World, pairs: bool = True) -> None:
    """The load checks of the households (design note of the households, section 7; part 2,
    section 6; ADR-0018 section 7; ADR-0006 section 5) on the state of `world`, as the loader
    would see its save with its input log (the couples of `Start`); a broken one is
    `Inconsistent` there, an assertion here. `t` is the clock, but 0 right after `Start`
    (clock −1), as for the round of the market; an input acts in the second after the clock,
    so a household may be formed or ended there (`clock + 1`).

    - every living person in exactly one household that has not ended; every household that
      has not ended has at least one living member; the counter above every ID (and every
      ID below it is a household: they stay as history, formed and ended within the clock);
      an archived person's household is a household that existed;
    - `moved_out` as the rule gives it, for the living and the archived: true for every
      founder in a couple of `Start`, every founder of 18 or more at the start, every person
      who has formed a partnership, every person whose `leave_home` second has passed (at
      or before `t` for the living, at or before the death for the archived; a founder's
      second at or before 0 is passed at the start); else false;
    - a `leave_home` event exactly for each living person who is not moved out, at its
      drawn second, after the clock (class general, subject the sim, payload variant 7);
      none for others;
    - each dependent person has a carer who lives, lives in the same household and is fit
      to care, or has none and lives in a household no member of which is moved out (the
      coordinator's decision on finding (c) of part 2; it covers a child alone, siblings
      moved together, an orphan who turned 18 and stays, and a group taken in by such an
      orphan); a person who is not dependent has no carer. One
      exception (review of part 1, part 2 section 6): a child dependent at the clock but of
      age in the second after it may keep a carer who lives in a household formed in that
      second (an input paired the carer; the child stayed; its birthday clears the carer
      later in that second);
    - every social parent of a living or archived person (part 2, section 6) is a SimId
      below the counter of a living or archived person, not the person herself, not one of
      her biological parents, and listed once;
    - the living members of a household with `moved_out` are at most one, or exactly the
      woman and the man of one open partnership (`check_moved_out_households`; `pairs`
      False leaves it out, for the forged saves that only it rejects)."""
    t = max(world.closed, 0)
    households = world.households
    if not world.households_on:
        assert not households and not [event for event in world.queue if event.kind == LEAVE_HOME_KIND]
        return
    assert set(households) == set(range(1, world.next_household_id)), (sorted(households), world.next_household_id)
    for number_, (formed, ended) in households.items():
        assert 0 <= formed <= world.closed + 1 and (ended is None or formed <= ended <= world.closed + 1), (number_, formed, ended)
    for sim, value in world.sims.items():
        assert value[9] in households and households[value[9]][1] is None, (sim, value[9])
    homes = {value[9] for value in world.sims.values()}
    for number_, (_, ended) in households.items():
        assert ended is not None or number_ in homes, number_
    for sim, value in world.archive.items():
        assert value[11] in households and households[value[11]][0] <= value[1], (sim, value[11])
    in_start_couples = {sim for pair in world.start_couples for sim in pair}
    partnered = {sim for value in world.partnerships.values() for sim in value[:2]}
    # A partnership an arrival brought moves nobody out in the town (note section 5, step 3:
    # "nothing else"); the pool's `moved_out` holds what it did in the pool. Only a partnership
    # formed in the town (an input or a round) moves an arrival out (review of the reference,
    # NOTES2.md section 5c).
    town_partnered = {
        sim for number_, value in world.partnerships.items() if not (world.origins and world.arrival_partnership(number_) is not None) for sim in value[:2]
    }
    people = {sim: (value[0], value[7], value[10], t) for sim, value in world.sims.items()}
    people.update({sim: (value[0], value[9], value[12], value[1]) for sim, value in world.archive.items()})
    for sim, (born, parents, moved_out, limit) in people.items():
        # A founder is a sim without parents that is no pool person (design note of the
        # arrival, section 6.1). An arrival comes moved out or not as the pool has it, and
        # its leaving home is the carried one (`carried_leave_home`, the pool's plus `Δ`; not
        # recomputed with the world seed, section 9).
        founder = parents is None and not world.is_pool_person(sim)
        if world.is_arrival(sim):
            carried = world.carried_leave_home(sim)
            # The carried leaving home is an event at or after `A` (a carried time at `A` is no
            # break, section 5), so for the living it has passed only once the clock reached
            # it: right after `Start` (clock −1) one due at second 0 is still pending (not `t`,
            # which is 0 there; the founders' "at or before 0 is passed at the start" is a rule
            # of `_house_founders`, not of an arrival). Review of the reference, NOTES2.md.
            until = world.closed if sim in world.sims else limit
            want = world.pool_member(sim)["moved_out"] or sim in town_partnered or (carried is not None and carried <= until)
        else:
            want = (
                (founder and sim in in_start_couples)
                or (founder and 0 - born >= 18 * YEAR)
                or sim in partnered
                or world.leave_home_second(sim) <= limit
            )
        assert moved_out == want, (sim, moved_out, want)
    pending = [event for event in world.queue if event.kind == LEAVE_HOME_KIND]
    for event in pending:
        assert event.cls == GENERAL and event.subject[0] == SIM_SUBJECT and event.period is None
        assert event.payload == variant(LEAVE_HOME_VARIANT, varint(event.subject[1])) and event.time > world.closed
    wanted = sorted(
        (sim, world.carried_leave_home(sim) if world.is_arrival(sim) else world.leave_home_second(sim))
        for sim, value in world.sims.items()
        if not value[10]
    )
    assert sorted((event.subject[1], event.time) for event in pending) == wanted, (pending, wanted)
    for sim, value in world.sims.items():
        carer = value[11]
        if not world.dependent(sim, t):
            assert carer is None, (sim, carer)
        elif carer is None:
            # The coordinator's decision on finding (c) of part 2: a dependent person without
            # a carer is valid if no member of its household is moved out (households of the
            # orphan rule's last stage only ever gain members who are not moved out).
            home = world.members(value[9])
            assert all(not world.sims[other][10] for other in home), (sim, home)
        else:
            assert carer in world.sims and world.fit_to_care(carer, t), (sim, carer)
            if world.sims[carer][9] != value[9]:
                # The exception of the review: of age in the second after the clock, the
                # carer's household formed in that second.
                of_age_next = world.closed + 1 - value[0] >= 18 * YEAR
                formed_next = households[world.sims[carer][9]][0] == world.closed + 1
                assert of_age_next and formed_next, (sim, carer)
    social = {sim: (value[7], value[12]) for sim, value in world.sims.items()}
    social.update({sim: (value[9], value[13]) for sim, value in world.archive.items()})
    for sim, (parents, socials) in social.items():
        for person in socials:
            assert 1 <= person < world.next_sim_id and (person in world.sims or person in world.archive), (sim, person)
            # Biological by SimId, for a pool person by its PoolOrigin keys (NOTES.md).
            assert person != sim and not world.is_biological_parent(sim, person), (sim, person)
        assert len(set(socials)) == len(socials), (sim, socials)
    if pairs:
        check_moved_out_households(world)


def moved_out_violations(world: World) -> list:
    """The households that break the rule of the moved out (`check_moved_out_households`), as
    (household, its living members with `moved_out`, by SimId), by household."""
    moved = {}
    for sim, value in sorted(world.sims.items()):
        if value[10]:
            moved.setdefault(value[9], []).append(sim)
    pairs = {frozenset(value[:2]) for value in world.partnerships.values() if value[3] is None}
    return [(household, sims) for household, sims in sorted(moved.items()) if len(sims) > 1 and frozenset(sims) not in pairs]


def check_moved_out_households(world: World) -> None:
    """The load check of the moved out in one household (ADR-0006 section 5; the gap the
    village start showed, ADR-0020 section 8, G4, which puts each person alone): the living
    members of a household with `moved_out` true are at most one, or exactly the woman and
    the man of one open partnership (`ended` None; one brought by an arrival counts). A
    broken one is `Inconsistent` in the core, an assertion here; `check_households` ends
    with it. Every rule that moves a person out keeps it: a partnership founds the couple's
    household, `leave_home` and a separation found one of their own, an arrival comes as the
    pool's household holds it, the orphan rule only moves dependent persons. Without
    households (the switch of this reference) there is nothing to check."""
    if not world.households_on:
        return
    broken = moved_out_violations(world)
    assert not broken, broken


def check_pool(world: World) -> None:
    """The load checks of the pool persons (design note of the arrival, section 9) on the state
    of `world` with the pool of its data, as the loader would see its save with its input
    log; a broken one is `Inconsistent` there, an assertion here:

    - the used households are exactly the draws, in this order, of `Start` (occasion 0,
      `n_start`) and, with `immigration` and `q > 0`, of each `Immigration` whose second the
      clock has passed (year `y` at `y · YEAR <= clock`, occasion `y`, `q`, the households
      not yet used); a draw that cannot reach `q` stopped the run (`pool.empty`), so no
      state lies after its second (so they have no repeats); their members are exactly the
      arrivals, and each arrival's `arrived` is the second of its household's draw (0 for
      `Start` and year 0, `y · YEAR` else); review of the reference, NOTES2.md;
    - an arrival: its lineage key a member of the data's pool, its sex, genome, `born_pool`
      (and genome key, parents' keys, social parents' keys) equal to it, `born = born_pool +
      (arrived − E_p)`, `arrived` 0 or a year start with `arrival_year` its year;
    - a pool child: its lineage key an unborn child of a pool pregnancy (its sex, genome,
      genome key equal to it), `born_pool` None, `arrived` the birth, `arrival_year` its
      calendar year, its PoolOrigin mother the pregnancy's mother, an arrival, its father the
      pregnancy's father;
    - a running pregnancy of the pool (`pool_father`): its mother an arrival, the pool's
      pregnancy of her (father, conception number, children) with `conceived` shifted, and
      `father` the sim with the father's key exactly when the father's household came no
      later in the used households than the mother's (decision 8 of NOTES.md: set once, at
      her arrival; review, NOTES2.md), else None; any other pregnancy has a `father`;
    - every living pool person has the series of attempts the rules give it
      (`World.series_due`: an arrival's is the carried one only while it is the series she
      brought, else the town's rule; review, NOTES2.md);
    - with `immigration` and `q > 0` exactly one event `Immigration { persons: q }` (class 100,
      kind 10, subject the world, period YEAR, planned by `core:started` at 0) at the first
      year start after the clock; else none.
    In the village (design note of the village, section 6) instead of the first point: no
    household of the file is used, the arrivals are exactly the persons of `village_draw`
    (the world seed, `n_start`), all at second 0, no pool child and no pool pregnancy; every
    arrival's record is `village_person` of its member entry (no carried plan, moved out).
    Without a pool there is no pool person, no used household, no pool pregnancy."""
    import pool_reference

    pool = world.data.pool
    events = [event for event in world.queue if event.kind == IMMIGRATION_KIND]
    if pool is None or world.pool_start is None:
        assert not world.origins and not world.used and not world.pool_fathers and not events
        # Every pregnancy is a town one, with its father (review, NOTES2.md section 5b).
        assert all(value[0] is not None for value in world.pregnancies.values()), sorted(world.pregnancies)
        return
    q = immigration_persons(world.pool_start["n_start"])
    if world.village:
        assert not world.pool_start["immigration"] and world.used == [] and not world.pool_fathers, "village"
        women, men = village_draw(pool, world.seed, world.pool_start["n_start"])
        arrivals = [world.key_of(sim) for sim in sorted(world.origins) if world.is_arrival(sim)]
        assert arrivals == women + men, "the arrivals are the persons of the village, in rank order"
        assert all(world.is_arrival(sim) and world.origins[sim]["arrived"] == 0 for sim in world.origins), "no pool child in the village"
        for key in arrivals:
            assert world.village_records[key] == village_person(pool.members[key][1]), key
    draws = [(0, pool_draw(pool, world.seed, 0, world.pool_start["n_start"], []))] if not world.village else []
    if world.pool_start["immigration"] and q > 0:
        for year in range(0, world.closed // YEAR + 1):
            taken = [index for _, draw in draws for index in draw]
            free = [index for index in range(len(pool.households)) if index not in taken]
            assert sum(pool.sizes[index] for index in free) >= q, f"pool.empty in year {year}: no state after it"
            draws.append((year * YEAR, pool_draw(pool, world.seed, year, q, taken)))
    assert world.used == [index for _, draw in draws for index in draw], world.used
    second_of = {index: second for second, draw in draws for index in draw}
    if not world.village:
        members = sorted(m["lineage_key"] for index in world.used for m in pool.households[index]["members"])
        arrivals = sorted(world.key_of(sim) for sim in world.origins if world.is_arrival(sim))
        assert members == arrivals, "the members of the used households are the arrivals"
        for sim in world.origins:
            if world.is_arrival(sim):
                assert world.origins[sim]["arrived"] == second_of[pool.members[world.key_of(sim)][0]], sim
    for sim, origin in world.origins.items():
        key, sex, person = world.key_of(sim), world.sex_of(sim), (world.sims[sim][3] if sim in world.sims else world.archive[sim][5])
        born = world.born_of(sim)
        sex_code = 0 if sex == FEMALE else 1
        if origin["born_pool"] is not None:
            assert key in pool.members, sim
            member = pool.members[key][1]
            assert sex_code == member["sex"] and genome_of(person) == pool_reference.encode_genome(member["genome"]), sim
            assert origin["born_pool"] == member["born"] and origin["genome_key"] == member["genome_key"], sim
            assert (origin["mother"], origin["father"], origin["social_parents"]) == (member["mother"], member["father"], member["social_parents"]), sim
            assert born == member["born"] + (origin["arrived"] - pool.frozen_at), sim
            assert origin["arrived"] >= 0 and origin["arrived"] % YEAR == 0 and origin["arrival_year"] == origin["arrived"] // YEAR, sim
            assert (world.sims[sim][7] if sim in world.sims else world.archive[sim][9]) is None
        else:
            assert key in pool.unborn, sim
            _, pregnancy, child = pool.unborn[key]
            assert sex_code == child["sex"] and genome_of(person) == pool_reference.encode_genome(child["genome"]), sim
            assert origin["genome_key"] == child["genome_key"] and origin["arrived"] == born and origin["arrival_year"] == born // YEAR, sim
            assert origin["mother"] == pregnancy["mother"] and origin["father"] == pregnancy["father"] and origin["social_parents"] == [], sim
            mother = world.by_key.get(pregnancy["mother"])
            assert mother is not None and world.is_arrival(mother), sim
            assert (world.sims[sim][7] if sim in world.sims else world.archive[sim][9]) is None
            # Born in the town: at or before the clock, while its mother lived (her pregnancy
            # ends with her death, so an archived mother left at or after the birth). Review
            # of the reference, NOTES2.md section 5b.
            assert born <= world.closed, (sim, born, world.closed)
            assert mother in world.sims or world.archive[mother][1] >= born, (sim, mother)
    for mother, (father, conception_number, children, conceived) in world.pregnancies.items():
        if mother not in world.pool_fathers:
            assert father is not None, mother
            continue
        assert world.is_arrival(mother), mother
        index = pool.members[world.key_of(mother)][0]
        (pregnancy,) = [g for g in pool.households[index]["pregnancies"] if g["mother"] == world.key_of(mother)]
        delta = world.origins[mother]["arrived"] - pool.frozen_at
        assert world.pool_fathers[mother] == pregnancy["father"] and conception_number == pregnancy["conception"], mother
        assert conceived == pregnancy["conceived"] + delta, mother
        # Decision 8 of NOTES.md: `father` is set once, at her arrival, to the sim with the
        # father's key if he exists then: exactly when his household is a used one that came
        # no later than hers (the same household, or an earlier one in arrival order).
        his = pool.members[pregnancy["father"]][0] if pregnancy["father"] in pool.members else None
        attached = his is not None and his in world.used and world.used.index(his) <= world.used.index(index)
        if attached:
            assert father is not None and world.key_of(father) == pregnancy["father"], mother
        else:
            assert father is None, mother
        assert [(key, 0 if sex == FEMALE else 1, genome_of(person)) for key, sex, person in children] == [
            (child["lineage_key"], child["sex"], pool_reference.encode_genome(child["genome"])) for child in pregnancy["children"]
        ], mother
    # The counts of a pool person (ADR-0020 section 5: "ihre Zählungen sind die des Pools plus die
    # der Stadt"; review of the reference, NOTES2.md section 5g): `births` is the pool's plus its
    # children born in the town (town and pool children, living or archived); its town
    # conceptions (of its town children and its running town pregnancy) are numbered on from the
    # pool's count, and `conceptions` is the pool's plus them. An archived woman may hold one more:
    # a pregnancy that ended with her death or left with her.
    for sim, origin in world.origins.items():
        record = pool.members[world.key_of(sim)][1] if origin["born_pool"] is not None else {"conceptions": 0, "births": 0}
        living = sim in world.sims
        conceptions, births = (world.sims[sim][5], world.sims[sim][6]) if living else (world.archive[sim][7], world.archive[sim][8])
        assert births == record["births"] + len(world.town_births(sim)), (sim, births, record["births"])
        numbers = set()
        for child in set(world.sims) | set(world.archive):
            parents = world.sims[child][7] if child in world.sims else world.archive[child][9]
            if parents is not None and parents[0] == sim:
                numbers.add(parents[2])
        if sim in world.pregnancies and sim not in world.pool_fathers:
            numbers.add(world.pregnancies[sim][1])
        town = len(numbers)
        assert sorted(numbers) == list(range(record["conceptions"], record["conceptions"] + town)), (sim, sorted(numbers), record["conceptions"])
        allowed = (record["conceptions"] + town,) if living else (record["conceptions"] + town, record["conceptions"] + town + 1)
        assert conceptions in allowed, (sim, conceptions, allowed)
    # The series of attempts of every living pool person (`World.series_due`).
    for sim in sorted(world.sims):
        if world.is_pool_person(sim):
            assert world.pending_series(sim) == world.series_due(sim), (sim, world.pending_series(sim), world.series_due(sim))
    for event in world.queue:
        if event.kind == ATTEMPT_KIND and world.is_pool_person(event.subject[1]):
            assert event.cls == GENERAL and event.period == MONTH and event.payload == variant(3, varint(event.subject[1])), event.subject
    if world.pool_start["immigration"] and q > 0:
        assert len(events) == 1, events
        (event,) = events
        due = (world.closed // YEAR + 1) * YEAR
        assert event.time == due and event.cls == IMMIGRATION_CLASS and event.subject == (WORLD_SUBJECT, 0), (event.time, world.closed)
        assert event.period == YEAR and event.payload == variant(IMMIGRATION_VARIANT, varint(q)) and event.plan == (1, 0)
    else:
        assert not events


def carried_plans(world: World) -> list:
    """The pending carried plans of a save (design note of the carried plans, section 2.5), each
    `(event, owner, slot)`: the attempt of an exempt woman (her series is the one she brought,
    `World.series_exempt`; slot 0), the `Birth` of a pool pregnancy (slot 1), the `Separation` of
    a brought partnership (its woman; slot 2), the `LeaveHome` of an arrival (slot 3). The owner
    is the arrival whose step 5 planned it."""
    found = []
    for event in world.queue:
        subject = event.subject[1]
        if event.kind == ATTEMPT_KIND and subject in world.sims and world.is_arrival(subject):
            number_ = world.open_partnership(subject)
            if number_ is not None and world.series_exempt(subject, number_):
                found.append((event, subject, 0))
        elif event.kind == BIRTH_KIND and subject in world.pool_fathers:
            found.append((event, subject, 1))
        elif event.kind == SEPARATION_KIND and world.origins and world.arrival_partnership(subject) is not None:
            found.append((event, world.partnerships[subject][0], 2))
        elif event.kind == LEAVE_HOME_KIND and world.is_arrival(subject):
            found.append((event, subject, 3))
    return found


def check_carried_conceptions(world: World) -> None:
    """C2 (design note of the carried plans, section 2.2): a conception of an arrival `W` in the
    partnership her household brought, with no child of hers born in the town before it, lies on
    her carried series' grid: the pool carried a `conception_at`, and `conceived = s + k · MONTH`
    with `s = conception_at + Δ`, `k >= 0`. The conceptions are her town children (conceived
    `pregnancy_months` months before their birth) and her running town pregnancy."""
    if world.data.pool is None or world.pool_start is None:
        return
    months = world.data.pregnancy_months * MONTH
    conceptions = []
    # Town children only (a pool person with town parents, or a town pregnancy without its
    # father, is `check_pool`'s to reject, not a conception of a partnership).
    for sim in set(world.sims) | set(world.archive):
        parents = world.sims[sim][7] if sim in world.sims else world.archive[sim][9]
        if parents is not None and not world.is_pool_person(sim) and world.is_arrival(parents[0]):
            conceptions.append((parents[0], parents[1], world.born_of(sim) - months))
    for mother, (father, _, _, conceived) in world.pregnancies.items():
        if mother not in world.pool_fathers and father is not None and world.is_arrival(mother):
            conceptions.append((mother, father, conceived))
    for woman, man, conceived in sorted(set(conceptions)):
        numbers = [
            n for n, (w, m, formed, ended) in world.partnerships.items() if (w, m) == (woman, man) and formed <= conceived and (ended is None or ended[0] >= conceived)
        ]
        assert len(numbers) == 1, (woman, man, conceived, numbers)
        (number_,) = numbers
        if world.arrival_partnership(number_) is None or any(b < conceived for b in world.town_births(woman)):
            continue
        carried = world.pool_member(woman)["conception_at"]
        assert carried is not None, (woman, conceived)
        first = carried + world.origins[woman]["arrived"] - world.data.pool.frozen_at
        assert conceived >= first and (conceived - first) % MONTH == 0, (woman, conceived, first)


def check_carried_births(world: World) -> None:
    """C3 (design note of the carried plans, section 2.3): each pregnancy of a used household,
    mother `M`, `b = birth_at + Δ`, is
    - pending (`M` living, `b` after the clock): `M`'s pregnancy is there, from the pool, and its
      `Birth` at `b`, class birth, micro 0, no period;
    - consumed (`b` at or before the clock, `M` living or left at or after `b`): every child of the
      pregnancy is a pool child born at `b`, with consecutive SimIds in the pregnancy's order;
    - cancelled (`M` left before `b`): no child of it in the town."""
    pool = world.data.pool
    if pool is None or world.pool_start is None:
        return
    for index in world.used:
        for pregnancy in pool.households[index]["pregnancies"]:
            mother = world.by_key.get(pregnancy["mother"])
            assert mother is not None and world.is_arrival(mother), pregnancy["mother"]
            birth_at = world.pool_member(mother)["birth_at"]
            assert birth_at is not None, mother
            b = birth_at + world.origins[mother]["arrived"] - pool.frozen_at
            keys = [child["lineage_key"] for child in pregnancy["children"]]
            if mother in world.sims and b > world.closed:
                assert mother in world.pregnancies and world.pool_fathers.get(mother) == pregnancy["father"], mother
                births = [e for e in world.queue if e.kind == BIRTH_KIND and e.subject == (SIM_SUBJECT, mother)]
                assert [(e.time, e.cls, e.micro, e.period) for e in births] == [(b, BIRTH, 0, None)], (mother, b)
            elif mother in world.sims or world.archive[mother][1] >= b:
                assert mother not in world.pool_fathers, mother
                sims = [world.by_key.get(key) for key in keys]
                assert all(sim is not None and world.is_pool_person(sim) and not world.is_arrival(sim) for sim in sims), (mother, sims)
                assert all(world.born_of(sim) == b for sim in sims), (mother, b, sims)
                assert all(later == earlier + 1 for earlier, later in zip(sims, sims[1:])), (mother, sims)
            else:
                assert not [key for key in keys if key in world.by_key], (mother, keys)


def check_carried_plans(world: World) -> None:
    """C4 (design note of the carried plans, section 2.5; decision Q2): every pending carried plan
    (`carried_plans`) was planned at its owner's arrival second (`planned_at = A`); one owner's
    plans name one `by`; and within one household `by − place − onsets` is the same for every
    owner, `onsets` the number of onset entries of path 1 the members before the owner wrote at
    their arrival (`World.creation_onset_entries`; the `core:arrived` entries of a household
    are in member order, each followed by its own onset entries of path 1 and nothing else:
    ADR-0017 section 3, addendum; without such entries they are consecutive, as before)."""
    if world.data.pool is None or world.pool_start is None:
        return
    pool = world.data.pool
    by_owner, offsets = {}, {}
    for event, owner, _ in carried_plans(world):
        by, planned_at = event.plan
        assert planned_at == world.origins[owner]["arrived"], (owner, event.kind, event.plan)
        assert by_owner.setdefault(owner, by) == by, (owner, by_owner[owner], by)
        index, _ = pool.members[world.key_of(owner)]
        keys = [m["lineage_key"] for m in pool.households[index]["members"]]
        place = keys.index(world.key_of(owner))
        onsets = sum(world.creation_onset_entries(world.by_key[key]) for key in keys[:place])
        offset = by - place - onsets
        assert offsets.setdefault(index, offset) == offset, (owner, index, by, place, onsets)


def check_carried_order(world: World) -> None:
    """C5 (design note of the carried plans, section 2.6): the pending carried plans, sorted by
    (their household's place in the used households, the owner's place in its household, the
    slot: conception, birth, separation, leaving home), have strictly ascending insertion
    numbers (step 5 plans them in that order; none is planned again)."""
    if world.data.pool is None or world.pool_start is None:
        return
    pool = world.data.pool
    rows = []
    for event, owner, slot in carried_plans(world):
        index, _ = pool.members[world.key_of(owner)]
        place = [m["lineage_key"] for m in pool.households[index]["members"]].index(world.key_of(owner))
        rows.append(((world.used.index(index), place, slot), event.insertion))
    seqs = [insertion for _, insertion in sorted(rows)]
    assert all(a < b for a, b in zip(seqs, seqs[1:])), sorted(rows)


CARRIED_CHECKS = (check_carried_conceptions, check_carried_births, check_carried_plans, check_carried_order)


def check_deaths(world: World) -> None:
    """The planned deaths of every living sim are those of its causes (genetics specification
    7.1; design note of the arrival, section 6.4): senescence and accident from its origin
    with its detail, each lethal affliction whose exposure runs (`World.cause_due`,
    `World.disease_due`), and nothing else of the kind death. A check of this reference for
    the scenarios of the arrival (the loader recomputes the deaths, section 9)."""
    for sim in world.sims:
        want = []
        for code in (SENESCENCE, ACCIDENT):
            due = world.cause_due(sim, code)
            if due is not None:
                want.append((due, death_payload(sim, str_key(code))))
        for affliction in world.sims[sim][4]:
            if affliction.exposure is not None and affliction.exposure[1] is not None:
                due = world.disease_due(sim, affliction)
                if due is not None:
                    want.append((due, death_payload(sim, affliction.disease.key)))
        got = [(event.time, event.payload) for event in world.queue if event.kind == DEATH_KIND and event.subject == (SIM_SUBJECT, sim)]
        assert sorted(got) == sorted(want), (sim, got, want)


def check_onsets(world: World) -> None:
    """The load check of the onsets (ADR-0017 section 3, addendum, "Speicherstand"; ADR-0006):
    for every sim of the active population (living, `World.sims`) and every affliction with
    `B > E` and `B` after the last closed second exactly one open event `DiseaseOnset { sim,
    disease }` at `B` (no period); no other such event. A missing, doubled or misdated one, or
    one for a sim outside the active population (archived or unknown) or for a disease the sim
    does not have, is `Inconsistent` in the core, an assertion here. The event's class, kind and
    subject follow from its payload (the save holds the payload only)."""
    want = []
    for sim in sorted(world.sims):
        e = world.creation_second(sim)
        for affliction in world.sims[sim][4]:
            b = world.onset_second_of(sim, affliction)
            if b > e and b > world.closed and abs(b) <= MAX_ABS_SECS:
                want.append((sim, affliction.disease.key, b))
    got = []
    for event in world.queue:
        if event.kind != DISEASE_ONSET_KIND:
            continue
        sim, key = onset_of_payload(event.payload)
        assert event.subject == (SIM_SUBJECT, sim) and event.cls == GENERAL and event.period is None, (event.subject, event.period)
        got.append((sim, key, event.time))
    assert sorted(got) == sorted(want), (sorted(set(got) ^ set(want)), len(got), len(want))


def check_unstarted_households(next_household_id: int, households: dict) -> None:
    """A world that never started has no household and the household counter at 1 (review
    of part 1; part 2, section 6)."""
    assert next_household_id == 1 and not households, (next_household_id, households)


def check_unstarted_pool(used: list) -> None:
    """A world that never started has used no household of the pool (design note of the
    arrival, sections 4 and 9: the used households are those of `Start` and the
    immigrations, and there was neither; review of the reference, NOTES2.md)."""
    assert not used, used


def story(world: World) -> tuple:
    """What the households must not change (design note of the households, section 8: the
    stories stay): every sim with its birth, key, sex and parents; every death with its time
    and cause; the partnerships; the separations; every attempt with its chance and
    outcome; every conception with its kind and children; the pregnancies; every choice of
    the market; the counters of SimId and PartnershipId and the clock."""
    return (
        sorted((sim, value[0], value[1], value[2], value[7]) for sim, value in world.sims.items()),
        sorted((sim, value[0], value[1], value[2], value[3], value[9]) for sim, value in world.archive.items()),
        {number_: list(value) for number_, value in world.partnerships.items()},
        [(t, number_) for t, number_, *_ in world.separations],
        list(world.attempts),
        [row[:6] + ([(key, sex) for key, sex in row[6]],) for row in world.multiples],
        sorted((mother, value[0], value[1], [key for key, _, _ in value[2]], value[3]) for mother, value in world.pregnancies.items()),
        [(record["n"], [(choice["woman"], choice["chosen"], choice["outcome"]) for choice in record["choices"]]) for record in world.rounds],
        (world.next_sim_id, world.next_partnership_id, world.closed),
    )


def without_households(world: World) -> World:
    """The same world (its arguments and its calls) with the households switched off (a
    switch of this reference only): the model of the similarity."""
    twin = World(**world.args, households=False)
    for call in world.script:
        if call[0] == "advance":
            twin.advance(call[1])
        elif call[0] == "arrange":
            twin.arrange(call[1], call[2])
        elif call[0] == "override_nutrition":
            twin.override_nutrition(call[1], call[2])
        else:
            twin.override(call[1])
    return twin


def check_story_kept(world: World) -> World:
    """The households change no story: `story` of the world equals that of its twin without
    households (whose log is the log before the households, entry for entry). Returns the
    twin."""
    twin = without_households(world)
    assert story(world) == story(twin), "the households changed the story"
    assert not [kind for kind in twin.entry_kinds if kind in (HOUSEHOLD_FORMED_VARIANT, MOVED_VARIANT)]
    return twin


def pregnancy_bytes(father, conception_number: int, children: list, conceived: int, pool_father=None) -> bytes:
    """Pregnancy { father: Option<SimId>, pool_father: Option<LineageKey>, conception: u32,
    children: Vec<Unborn>, conceived: SimTime } with Unborn { lineage_key, sex, genome }, the
    children in index order (design note of the twins, section 4; design note of the arrival,
    section 9: `father` an option, `pool_father` right after it, decision of NOTES.md). A
    town pregnancy has `father` Some and `pool_father` None; a pool pregnancy `pool_father`
    Some and `father` Some only if the father has arrived."""
    unborn = [lineage_key(key) + sex + genome_of(person) for key, sex, person in children]
    assert len(unborn) in (1, 2)
    # A pregnancy without both (not reachable; `check_pool` rejects it) still has bytes.
    return (
        (NONE if father is None else some(varint(father)))
        + (NONE if pool_father is None else some(lineage_key(pool_father)))
        + varint(conception_number)
        + seq(unborn)
        + signed(conceived)
    )


def pool_origin_bytes(origin) -> bytes:
    """`pool: Option<PoolOrigin>`, the last field of Sim and ArchivedSim (design note of the
    arrival, sections 6.1 and 9; the position a decision of NOTES.md): `PoolOrigin {
    genome_key: GenomeKey, born_pool: Option<SimTime>, arrived: SimTime, arrival_year: u64,
    mother: Option<LineageKey>, father: Option<LineageKey>, social_parents: Vec<LineageKey> }`
    in the note's field order."""
    if origin is None:
        return NONE

    def option(value, encode) -> bytes:
        return NONE if value is None else some(encode(value))

    return some(
        varint(origin["genome_key"])
        + option(origin["born_pool"], signed)
        + signed(origin["arrived"])
        + varint(origin["arrival_year"])
        + option(origin["mother"], lineage_key)
        + option(origin["father"], lineage_key)
        + seq([lineage_key(key) for key in origin["social_parents"]])
    )


def used_households_bytes(used: list) -> bytes:
    """`pool_used: Vec<u32>`, the households of the pool used (indices into the file, in
    arrival order), the last field of the projection (decision of NOTES.md)."""
    return seq([varint(index) for index in used])


def parents_bytes(parents) -> bytes:
    """Option<Parents>, Parents { mother: SimId, father: SimId, conception: u32 }"""
    if parents is None:
        return NONE
    mother, father, conception_number = parents
    return some(varint(mother) + varint(father) + varint(conception_number))


def social_parents_bytes(social) -> bytes:
    """`social_parents: Vec<SimId>` (design note of the households, part 2, section 2): in
    the order they became social parents."""
    return seq([varint(sim) for sim in social])


def header(data_hash: int) -> bytes:
    """magic, format_version, core_version, data_hash"""
    return b"SIMAGENT" + varint(FORMAT_VERSION) + varint(CORE_VERSION) + varint(data_hash)


# A world that never started: nothing but the clock and the counters.
EMPTY_PROJECTION = (
    varint(PROJECTION_VERSION)
    + NONE  # world_seed
    + NONE  # world_params
    + signed(-1)
    + varint(1)  # next SimId
    + varint(1)  # next PartnershipId
    + varint(1)  # next HouseholdId
    + seq([])
    + varint(1)
    + seq([])  # sims
    + seq([])  # partnerships
    + seq([])  # households
    + seq([])  # pregnancies
    + seq([])  # archive
    + seq([])  # pool_used
)


def empty_save(data_hash: int) -> bytes:
    """The save of a world that never started, on data with the hash `data_hash`."""
    return header(data_hash) + EMPTY_PROJECTION + seq([]) + varint(1) + seq([]) + seq([])



# ---------------------------------------------------------------------------------------
# Known answers of the founders' draw (F1, FOUNDER_ALLELE = 28; slice S2b of the switch to
# map 4): `--founder-alleles`.

# The scenario, fixed before any run: the template of map 4 (tests/golden/map4_template with
# the repository's life course data) with the pool pool_b (made on the template, so its
# `frozen_hash` is the template's hash) and the frequencies it was made with
# (tests/golden/pool/pool_frequencies.json); world seed 0x28, founder tag
# `test:founder_allele`, the switch on, no couples, no capacity, `pool` None (founders on
# data with a pool are allowed; a `PoolStart` with founders is not). Founder 0 a woman of
# 30, founder 1 a man of 32, both naming nothing; founder 2 a woman of 30 who names
# `core:herc2` [0, 0], where her draws give allele 1 (asserted).
F1_SEED = 0x28
F1_TAG = "test:founder_allele"
F1_LOCI = ("core:growth_01", "core:cognition_01", "core:disease_ar", "core:herc2", "core:mc1r", "core:disease_xr", "core:disease_mt")
F1_NAMED = "core:herc2"


def f1_data() -> Data:
    golden = ROOT / "tests" / "golden"
    frequencies = data_reference.frequencies_file(golden / "pool" / "pool_frequencies.json")
    return Data(*data_reference.documents(golden / "map4_template"), pool=(golden / "pool" / "pool_b.bin").read_bytes(), frequencies=frequencies)


def f1_founders() -> list:
    return [(-30 * YEAR, FEMALE, []), (-32 * YEAR, MALE, []), (-30 * YEAR, FEMALE, [(F1_NAMED, [0, 0])])]


def founder_allele_rows() -> list:
    """The known answers of FOUNDER_ALLELE: per founder and chosen locus the draws (gamete,
    `q` and `u` with their bits, the allele), the founder's genome after `Start` (postcard,
    `genome_of`) and the number of its alleles 1, the `Start` input record (length, xxh3), the
    projection after `Start` (state hash), and the checks: founder 2's `[0, 0]` at
    `core:herc2` stays where her draws give 1; a man draws one X copy (gamete 0), everyone one
    mt copy (gamete 0); map 4 has no Y locus; the same founders on data without frequencies
    (the template alone) draw only the additive loci; on the map 3 fixture nothing is drawn. Then
    the share of allele 1 over 10 000 founders at an additive locus of the template (`q` =
    0.5) against 4 standard errors (genetics specification 8.1)."""
    data = f1_data()
    rows = [
        f"founder_allele scenario data tests/golden/map4_template + pool_b + tests/golden/pool/pool_frequencies.json data_hash {data.hash:016x}"
        f" frozen_hash {data.pool.frozen_hash:016x} pool_hash {data.pool.hash:016x} seed {F1_SEED:#x} tag {F1_TAG} tag_value {FOUNDER_ALLELE}",
        "founder_allele frequencies " + " ".join(f"{locus}={q!r}" for locus, q in data.pool_frequencies.items()),
    ]
    assert not any(locus["chromosome"] == "y" for locus in data.loci.values()), "map 4 has no Y locus"
    founders = f1_founders()
    world = World(F1_SEED, F1_TAG, True, founders, (), data)
    for index, (born, sex, genes) in enumerate(founders):
        sim = index + 1
        key = founder_key(F1_TAG, index)
        assert world.sims[sim][1] == key
        draws = world.founder_draws[sim]
        drawn_loci = sorted({locus for locus, *_ in draws})
        rows.append(
            f"founder_allele founder {index} sex {'female' if sex == FEMALE else 'male'} born {born} key {key:016x} names {named_alleles(genes)}"
            f" draws {len(draws)} drawn_loci {len(drawn_loci)}"
        )
        for locus_id in F1_LOCI:
            locus = data.loci[locus_id]
            mine = [d for d in draws if d[0] == locus_id]
            copies = copies_at(world.sims[sim][3], locus)
            if not mine:
                rows.append(f"founder_allele founder {index} locus {locus_id} chromosome {locus['chromosome']} not drawn copies {copies}")
                continue
            assert [d[1] for d in mine] == founder_gametes(locus, sex)
            assert [d[4] for d in mine] == copies
            for _, gamete, q, u, allele in mine:
                assert u == founder_allele_unit(F1_SEED, key, locus_id, gamete)
                rows.append(
                    f"founder_allele founder {index} locus {locus_id} chromosome {locus['chromosome']} str_key {str_key(locus_id):016x}"
                    f" gamete {gamete} q {q!r} q_bits {struct_bits(q)} u {u!r} u_bits {struct_bits(u)} allele {allele}"
                )
        genome = genome_of(world.sims[sim][3])
        ones = sum(sum(copies_at(world.sims[sim][3], locus)) for locus in data.loci.values())
        rows.append(f"founder_allele founder {index} genome_bytes {len(genome)} ones {ones} genome {genome.hex()}")
    # Founder 2 names core:herc2 [0, 0]; her draws there (the same address as without the
    # name) would give 1.
    key2 = founder_key(F1_TAG, 2)
    would = [1 if founder_allele_unit(F1_SEED, key2, F1_NAMED, g) < data.pool_frequencies[F1_NAMED] else 0 for g in (0, 1)]
    assert 1 in would and copies_at(world.sims[3][3], data.loci[F1_NAMED]) == [0, 0]
    unnamed = World(F1_SEED, F1_TAG, True, founders[:2] + [(founders[2][0], FEMALE, [])], (), data)
    assert copies_at(unnamed.sims[3][3], data.loci[F1_NAMED]) == would
    assert all(copies_at(unnamed.sims[3][3], locus) == copies_at(world.sims[3][3], locus) for locus_id, locus in data.loci.items() if locus_id != F1_NAMED)
    rows.append(
        f"founder_allele named_zero founder 2 locus {F1_NAMED} named [0, 0] keeps [0, 0]; unnamed it draws {would}"
        f" (u {[repr(founder_allele_unit(F1_SEED, key2, F1_NAMED, g)) for g in (0, 1)]}); every other locus equal"
    )
    record = world.inputs[0]
    rows.append(f"founder_allele start_input bytes {len(record)} xxh3 {hash64(record):016x} hex {record.hex()}")
    rows.append(f"founder_allele after_start state_hash {world.state_hash():016x} log_hash {world.log_hash():016x} save_bytes {len(world.save())} save_xxh3 {hash64(world.save()):016x}")
    # The same founders on the template without a pool: no frequencies, so the seven loci keep
    # the reference allele and only the 121 additive loci are drawn (P2 (a)).
    plain = Data(*data_reference.documents(ROOT / "tests" / "golden" / "map4_template"))
    no_q = World(F1_SEED, F1_TAG, True, founders, (), plain)
    for sim in (1, 2, 3):
        assert {locus for locus, *_ in no_q.founder_draws[sim]} == {locus["id"] for locus in plain.measured_loci}
        for locus_id in POOL_LOCI:
            assert copies_at(no_q.sims[sim][3], plain.loci[locus_id]) == [0] * len(founder_gametes(plain.loci[locus_id], no_q.sims[sim][2]))
        for locus in plain.measured_loci:
            assert copies_at(no_q.sims[sim][3], locus) == copies_at(world.sims[sim][3], locus), "the additive draws do not depend on the frequencies"
    rows.append(
        f"founder_allele without_frequencies data tests/golden/map4_template data_hash {plain.hash:016x}: founders 0 to 2 draw the {len(plain.measured_loci)} additive loci only,"
        f" the seven loci of the pool keep the reference allele, the additive alleles are those above"
    )
    rows.append(f"founder_allele without_frequencies genomes " + " ".join(genome_of(no_q.sims[sim][3]).hex() for sim in (1, 2, 3)))
    # `q = d_ref / 2` away from 0.5 and a threshold locus: the fixture with traits
    # (tests/golden/data_traits: `test:g1` additive `d_ref` 1.25, `test:f1` 1.0, `test:p1` 0.0,
    # `test:g2` a threshold locus with `d_ref` 0.5, the disease locus recessive), founders 0 and
    # 1 of the scenario naming nothing: the three additive loci are drawn at 0.625, 0.5 and 0.0
    # (never 1: `u < 0` never holds), the threshold and the recessive locus keep the reference
    # allele (no draw, though the threshold locus has a `d_ref`).
    traits = Data(*data_reference.documents(ROOT / "tests" / "golden" / "data_traits"))
    on_traits = World(F1_SEED, F1_TAG, True, founders[:2], (), traits)
    for sim in (1, 2):
        draws = on_traits.founder_draws[sim]
        assert {locus for locus, *_ in draws} == {locus["id"] for locus in traits.measured_loci}
        for locus_id, gamete, q, u, allele in draws:
            assert q == float(traits.loci[locus_id]["action"]["additive"]["d_ref"]) / 2.0
            rows.append(
                f"founder_allele data_traits founder {sim - 1} locus {locus_id} gamete {gamete} q {q!r} q_bits {struct_bits(q)}"
                f" u {u!r} u_bits {struct_bits(u)} allele {allele}"
            )
        rows.append(
            f"founder_allele data_traits founder {sim - 1} not drawn "
            + " ".join(f"{locus_id}={copies_at(on_traits.sims[sim][3], locus)}" for locus_id, locus in traits.loci.items() if locus_id not in {d[0] for d in draws})
            + f" genome {genome_of(on_traits.sims[sim][3]).hex()}"
        )
    map3 = World(F1_SEED, F1_TAG, True, founders[:2], (), MAP3)
    assert all(map3.founder_draws[sim] == [] for sim in (1, 2))
    rows.append(f"founder_allele map3 data tests/golden/map3 data_hash {MAP3.hash:016x}: nothing drawn (no additive locus, no frequencies)")
    # The share at q = 0.5 over 10 000 founders (gamete 0 at core:growth_01, women).
    locus_id, n = "core:growth_01", 10_000
    q = founder_allele_q(data, data.loci[locus_id])
    ones = sum(1 for index in range(n) if founder_allele_unit(F1_SEED, founder_key(F1_TAG, index), locus_id, 0) < q)
    se = math.sqrt(q * (1.0 - q) / n)
    share = ones / n
    assert abs(share - q) <= 4.0 * se, (share, q, se)
    rows.append(f"founder_allele share locus {locus_id} gamete 0 founders 0 to {n - 1} of {F1_TAG} seed {F1_SEED:#x} q {q!r} ones {ones} share {share!r} se {se!r} within 4 se")
    return rows


if __name__ == "__main__" and "--founder-alleles" in __import__("sys").argv:
    for row in founder_allele_rows():
        print(row)
    raise SystemExit(0)


if __name__ == "__main__":
    print(f"core_version {CORE_VERSION} projection_version {PROJECTION_VERSION}")
    # The repository's data (S5: map 4 with the pool and its frequencies, the CLI's data) and
    # the map 3 fixture (the core tests); the pure values below are those of the map 3 fixture
    # (the unit tests of the core read it).
    print(
        f"data_hash_repository {repository().hash:016x} definitions {repository().pool.frozen_hash:016x} pool_seed {repository().pool.seed:016x}"
        f" pool_hash {repository().pool.hash:016x} traits {len(repository().traits)} measured_loci {len(repository().measured_loci)} loci {len(repository().loci)}"
    )
    print(f"data_hash_map3 {MAP3.hash:016x}")
    print(f"data_hash_fork {FORK_DATA.hash:016x}")
    print("diseases " + " ".join(f"{d.code}={d.key:016x}" for d in MAP3.diseases))
    print(f"causes {SENESCENCE}={str_key(SENESCENCE):016x} {ACCIDENT}={str_key(ACCIDENT):016x}")
    for name, data in (("map3", MAP3), ("fork", FORK_DATA)):
        rates = " ".join(
            f"{sex_name} a {data.senescence[sex][0]!r} b {data.senescence[sex][1]!r} accident {data.accident[sex]!r}"
            for sex, sex_name in ((FEMALE, "female"), (MALE, "male"))
        )
        print(f"death_rates_{name} {rates} sigma_g {data.sigma_g!r} longevity_trait {data.longevity_trait}")
    for name, data in (("map3", MAP3), ("fork", FORK_DATA)):
        form = data.fertility
        window = f" from {form.start!r} until {form.until!r}" if form.kind == "window" else ""
        print(
            f"fertility_{name} form {form.kind}{window} end {data.fertility_end!r} adult_rate {data.adult_rate!r}"
            f" pause_months {data.pause_months} pregnancy_months {data.pregnancy_months}"
            f" wish_tilt {data.wish_tilt!r} wish_shape {data.wish_shape!r} twins {data.twins!r}"
        )
    print(f"life_values_fork {FORK_LIFE_VALUES}")
    print(f"separation_rate map3 {MAP3.separation_rate!r} fork {FORK_DATA.separation_rate!r}")
    print(
        f"tags FERTILITY_MENARCHE {FERTILITY_MENARCHE} FERTILITY_WISH {FERTILITY_WISH} MULTIPLE_BIRTH {MULTIPLE_BIRTH}"
        f" SEPARATION_BUDGET {SEPARATION_BUDGET}"
    )
    print(f"event kind separation {SEPARATION_KIND} class {GENERAL} subject partnership {PARTNERSHIP_SUBJECT}")
    print(
        f"tags PAIRING_RANK {PAIRING_RANK} MARKET_SEARCH {MARKET_SEARCH} MARKET_ORDER {MARKET_ORDER}"
        f" PAIRING_NOISE {PAIRING_NOISE}"
    )
    print(
        f"event kind market_round {MARKET_ROUND_KIND} class {GENERAL} subject world {WORLD_SUBJECT}"
        f" payload {MARKET_ROUND_PAYLOAD.hex()} period {MONTH}"
    )
    print(f"reject partner_too_young {PARTNER_TOO_YOUNG} partners_too_closely_related {PARTNERS_TOO_CLOSELY_RELATED}")
    for name, data in (("map3", MAP3), ("fork", FORK_DATA)):
        print(
            f"market_{name} age {data.market_age} c {data.market_height!r} m {data.market_center!r}"
            f" w {data.market_width!r} rho {data.market_minor!r} k {data.market_candidates}"
            f" w_a {data.pairing_weight_age!r} mu_a {data.pairing_gap!r} limit {data.kinship_limit!r}"
            f" cousin_odds {data.cousin_odds!r} K {kinship_weight(data)!r}"
        )
    # Pure values of the market on the repository's data, for unit tests: a woman born at 0
    # at `t = age · YEAR` seconds (17.999 · YEAR is no whole second: rounded down, and the
    # last second before 18 as well).
    for label, t in [
        ("16", 16 * YEAR),
        ("17.5", 35 * YEAR // 2),
        ("17.999", math.floor(Fraction(17_999, 1000) * YEAR)),
        ("18*YEAR-1", 18 * YEAR - 1),
        ("18", 18 * YEAR),
        ("25.23", 2523 * YEAR // 100),
        ("40", 40 * YEAR),
        ("60", 60 * YEAR),
    ]:
        print(f"market_search_chance age {label} t {t} a {as_years(t)!r} p {search_chance(t, MAP3)!r}")
    for gap, phi, noise in [(4.69, 0.0, 0.0), (0.0, 0.0, 1.5), (3.0, 0.0625, 0.0), (10.0, 0.03125, -2.0)]:
        print(f"market_utility gap {gap!r} phi {phi!r} noise {noise!r} U {utility(gap, phi, noise, MAP3)!r}")
    clan_keys = [founder_key("test:clan", index) for index in range(6)]
    order = ranked(0xFF, MARKET_ORDER, WORLD_KEY, 7, clan_keys)
    print(
        "market_order seed 0xff month 7 founders 0-5 of test:clan: "
        + " ".join(f"{key:016x} (founder {clan_keys.index(key)})" for key in order)
    )
    candidates = ranked(0xFF, PAIRING_RANK, clan_keys[0], 7, clan_keys[1:])
    print(
        "market_candidates seed 0xff month 7 seeker founder 0 of test:clan, founders 1-5: "
        + " ".join(f"{key:016x} (founder {clan_keys.index(key)})" for key in candidates)
    )
    print(
        "market_ranks MARKET_ORDER "
        + " ".join(f"{rank(0xFF, MARKET_ORDER, WORLD_KEY, 7, key):016x}" for key in clan_keys)
        + " PAIRING_RANK "
        + " ".join(f"{rank(0xFF, PAIRING_RANK, clan_keys[0], 7, key):016x}" for key in clan_keys[1:])
    )
    # The similarity terms (design note of the similarity, sections 1 and 2): the weights of
    # the life course data, the named traits (none in the repository's data), and pure values
    # of the utility with both terms, with one and without (a term left out, not added as 0).
    print(
        f"similarity_map3 w_K {MAP3.pairing_weight_cognition!r} w_H {MAP3.pairing_weight_height!r}"
        f" cognition_trait {MAP3.cognition_trait} height_trait {MAP3.height_trait}"
    )
    w_k, w_h = MAP3.pairing_weight_cognition, MAP3.pairing_weight_height
    for gap, phi, noise, d_k, d_h in [(4.69, 0.0, 0.0, 1.0, 1.0), (0.0, 0.0, 1.5, -0.5, 2.0), (3.0, 0.0625, 0.0, 0.3, -0.7), (10.0, 0.03125, -2.0, 1.25, 0.0)]:
        both = utility(gap, phi, noise, MAP3, [(w_k, d_k), (w_h, d_h)])
        cognition = utility(gap, phi, noise, MAP3, [(w_k, d_k)])
        height = utility(gap, phi, noise, MAP3, [(w_h, d_h)])
        print(
            f"similarity_utility gap {gap!r} phi {phi!r} noise {noise!r} dK {d_k!r} dH {d_h!r}"
            f" penalties {penalties(gap, phi, MAP3, [(w_k, d_k), (w_h, d_h)])!r}"
            f" U both {both!r} cognition_only {cognition!r} height_only {height!r} neither {utility(gap, phi, noise, MAP3)!r}"
        )
    # The households (design note of the households): the tag, the event kind, the payloads,
    # the rule, the table of the repository with its thresholds, and pure values of the age
    # of leaving (the unit draws at the ten thresholds and between them; the first founders
    # of the tag `test:homes` at seed 0xff).
    print(
        f"households tag LEAVE_HOME {LEAVE_HOME} event kind leave_home {LEAVE_HOME_KIND} class {GENERAL}"
        f" subject sim {SIM_SUBJECT} queue payload {variant(LEAVE_HOME_VARIANT, varint(1)).hex()} (sim 1)"
        f" log payloads core:household_formed {HOUSEHOLD_FORMED_VARIANT} core:moved {MOVED_VARIANT} rule {HOUSEHOLD_RULE}"
    )
    print(f"leave_home_weights_map3 {MAP3.leave_home_weights!r} fork {FORK_DATA.leave_home_weights!r}")
    print(f"leave_home_thresholds_map3 {leave_home_thresholds(MAP3.leave_home_weights)!r}")
    for u in [0.0, 0.019999999999999997, 0.02, 0.0499, 0.05, 0.3499, 0.35, 0.5, 0.7000000000000001, 0.9699, 0.9700000000000001, 0.9999999999999999]:
        print(f"leave_home_age u {u!r} age {leave_home_age(u, MAP3.leave_home_weights)}")
    homes_keys = [founder_key("test:homes", index) for index in range(4)]
    print(
        "leave_home_draws seed 0xff test:homes founders 0-3: "
        + " ".join(
            f"{key:016x} u {leave_home_unit(0xFF, key)!r} age {leave_home_age(leave_home_unit(0xFF, key), MAP3.leave_home_weights)}"
            for key in homes_keys
        )
    )
    # The households, part 2 (design note, sections 1 and 2): the tag, the field and pure
    # values of the foster draw (the ranks of four women of `test:homes` for the
    # representative founder 4 at the start and in month 100).
    print(f"households_part_2 tag FOSTER_RANK {FOSTER_RANK} social_parents empty {social_parents_bytes([]).hex()} [2, 7] {social_parents_bytes([2, 7]).hex()}")
    homes_women = [founder_key("test:homes", index) for index in (0, 1, 5)]
    for month in (0, 100):
        subject = founder_key("test:homes", 4)
        ranks = " ".join(f"{key:016x}:{rank(0xFF, FOSTER_RANK, subject, month, key):016x}" for key in homes_women)
        (chosen,) = k_smallest(0xFF, FOSTER_RANK, subject, month, homes_women, 1)
        print(f"foster_rank seed 0xff subject {subject:016x} (test:homes founder 4) month {month} candidates founders 0 1 5 ranks {ranks} chosen {chosen:016x} (founder {[founder_key('test:homes', i) for i in range(7)].index(chosen)})")
    print(f"empty_projection {EMPTY_PROJECTION.hex()}")
    print(f"empty_save {empty_save(MAP3.hash).hex()}")
    print(f"empty_save_repository {empty_save(repository().hash).hex()}")
    # The constants of this model against the code lists of tests/golden.
    golden = ROOT / "tests" / "golden"
    lines = {tuple(line.split()) for path in ("rng_tags.txt", "event_codes.txt", "log_codes.txt") for line in (golden / path).read_text().splitlines()}
    wanted = {
        ("tag", str(PAIRING_RANK), "PAIRING_RANK"),
        ("tag", str(MARKET_SEARCH), "MARKET_SEARCH"),
        ("tag", str(MARKET_ORDER), "MARKET_ORDER"),
        ("tag", str(PAIRING_NOISE), "PAIRING_NOISE"),
        ("tag", str(SEPARATION_BUDGET), "SEPARATION_BUDGET"),
        ("tag", str(ENV_SHARED), "ENV_SHARED"),
        ("tag", str(ENV_INDIVIDUAL), "ENV_INDIVIDUAL"),
        ("kind", "market_round", str(MARKET_ROUND_KIND)),
        ("kind", "separation", str(SEPARATION_KIND)),
        ("tag", str(LEAVE_HOME), "LEAVE_HOME"),
        ("tag", str(FOSTER_RANK), "FOSTER_RANK"),
        ("kind", "leave_home", str(LEAVE_HOME_KIND)),
        ("subject", "sim", str(SIM_SUBJECT)),
        ("payload", str(HOUSEHOLD_FORMED_VARIANT), "core:household_formed"),
        ("payload", str(MOVED_VARIANT), "core:moved"),
        ("cause", "0", "event"),
        ("subject", "world", str(WORLD_SUBJECT)),
        ("class", "general", str(GENERAL)),
        ("payload", "5", "core:partnered"),
        ("cause", "3", "rule"),
        ("cause", "4", "random"),
        ("tag", str(EMIGRATION), "EMIGRATION"),
        ("tag", str(PRERUN_ALLELE), "PRERUN_ALLELE"),
        ("tag", str(FOUNDER_ALLELE), "FOUNDER_ALLELE"),
        ("class", "emigration", str(EMIGRATION_CLASS)),
        ("kind", "emigration", str(EMIGRATION_KIND)),
        ("payload", str(EMIGRATED_VARIANT), "core:emigrated"),
        ("kind", "disease_onset", str(DISEASE_ONSET_KIND)),
        ("payload", str(DISEASE_ONSET_ENTRY), "core:disease_onset"),
    } | {("reject", str(code), name) for code, name in REJECT_NAMES.items() if code not in NEW_CODE_LINES_OF_THE_ARRIVAL + NEW_CODE_LINES_OF_THE_VILLAGE}
    missing = sorted(wanted - lines)
    print(f"code lists (rng_tags, event_codes, log_codes) against the constants: {'ok' if not missing else f'MISSING {missing}'}")
    # The onset of a disease (ADR-0017 section 3, addendum of 2026-10-20): the codes, the bytes
    # of a queue payload and of a log payload, and `B` with its year and whole years for the
    # onsets of the hand tests (tests/onset.rs: born −40 or −10 years, onset 0 and 30).
    print(
        f"disease_onset event kind {DISEASE_ONSET_KIND} class {GENERAL} subject sim {SIM_SUBJECT}"
        f" queue payload {onset_payload(1, str_key('core:disease_ad_late')).hex()} (sim 1, core:disease_ad_late)"
        f" log payload core:disease_onset {DISEASE_ONSET_ENTRY}"
        f" {onset_entry_payload('core:disease_ad_late', onset_second(-40 * YEAR, 30.0), 30.0).hex()} (-10, 30)"
    )
    for born_years, onset in [(-40, 0.0), (-40, 30.0), (-10, 30.0), (0, 30.0), (0, 0.5), (-1, 41.7)]:
        b = onset_second(born_years * YEAR, onset)
        print(f"disease_onset_second born {born_years * YEAR} onset {onset!r} B {b} since_year {b // YEAR} age_years {math.floor(onset)}")
    print("step hazard checks (disease_age, disease_hazard_used): ok")
    print("closed form checks (senescence_age, accident_age against mortality.rs W7.1): ok")
    # The pedigree (step 10): f64 in the JSON as serde_json 1.0.151 writes them (zmij 1.0.23),
    # the values of `json_number`'s checks.
    samples = [0.0, 0.5, 0.25, 0.125, 0.0625, 0.53125, 2.0**-14, 1e-5, 2.0**-17, 1e16, 48.02016537979952]
    print(f"pedigree_json_numbers {[(value.hex(), json_number(value)) for value in samples]}")
    # The emigration by soft capacity (design note of the emigration, step 11): the codes,
    # and the unit values of `e` (section 6): `P = K` draws nothing, `P = K + 1`, `P = 1.5 K`
    # (0.5 exactly), `P = 3 K` (2, held at 0.5), with the bits of `e`.
    print(
        f"emigration tag EMIGRATION {EMIGRATION} class {EMIGRATION_CLASS} kind {EMIGRATION_KIND} subject world {WORLD_SUBJECT}"
        f" queue payload {variant(EMIGRATION_VARIANT, varint(3)).hex()} (capacity 3) period {YEAR} first {YEAR - 1}"
        f" log payload core:emigrated {EMIGRATED_VARIANT} rule {CAPACITY_RULE} reject capacity_zero {CAPACITY_ZERO}"
        f" ended_by emigration {ENDED_BY_EMIGRATION} left_by death {LEFT_BY_DEATH} emigration {LEFT_BY_EMIGRATION}"
    )
    for k in (1, 2, 4, 7, 10, 12, 100):
        values = [("K", k, None)]
        half = (3 * k) // 2 if k % 2 == 0 else None
        for name, p_ in (("K+1", k + 1), ("1.5K-1", None if half is None else half - 1), ("1.5K", half), ("3K", 3 * k)):
            if p_ is None or p_ <= k:
                continue
            values.append((name, p_, emigration_rate(p_, k)))
        print(
            f"emigration_rate K {k} P=K nothing "
            + " ".join(f"P={name} {p_} e {e!r} bits {struct_bits(e)}" for name, p_, e in values[1:])
        )
    # The measurement of the genic spread and the doses (step 11, design note of the
    # measurement): on the fixture tests/golden/data_traits (`test:face` includes
    # `test:growth` with 0.25; `test:g2` a threshold locus), `A` of hand-made genomes against
    # the map's formulas typed by hand (exact fractions), `G` = `A / sigma_ref`, and the
    # measurement of three hand-made persons whose sums of `test:face` give other bits in the
    # other order: the persons with lineage keys 1, 2, 3 against the same genomes with the
    # keys 3, 2, 1 (so the fold runs the other way).
    traits_data = Data(*data_reference.documents(ROOT / "tests" / "golden" / "data_traits"))

    def traits_genome(doses: tuple) -> dict:
        genes = [(locus, [1, 1] if dose == 2 else [1, 0]) for locus, dose in zip(("test:g1", "test:f1", "test:g2", "test:p1"), doses) if dose]
        return founder_genome(FEMALE, genes, traits_data.loci)

    def traits_hand(doses: tuple) -> list:
        g1, f1, g2, _p1 = (Fraction(dose) for dose in doses)
        growth = Fraction(1, 2) * (g1 - Fraction(5, 4)) + Fraction(1, 4) * (g2 - Fraction(1, 2))
        face = Fraction(3, 4) * (f1 - 1) + Fraction(1, 4) * growth
        return [face, growth, Fraction(-1, 4) * (g1 - Fraction(5, 4)), Fraction(doses[3])]

    print(f"measure_format_version {MEASURE_FORMAT_VERSION} traits {[trait.id for trait in traits_data.traits]} measured_loci {[locus['id'] for locus in traits_data.measured_loci]} threshold_loci {[locus['id'] for locus in traits_data.threshold_loci]}")
    for doses in [(0, 0, 0, 0), (2, 2, 2, 2), (1, 0, 2, 1), (0, 1, 1, 0), (2, 1, 0, 0)]:
        person = traits_genome(doses)
        values = [additive_value(person, trait) for trait in traits_data.traits]
        assert [Fraction(value) for value in values] == traits_hand(doses), doses
        assert all(genetic_value(person, trait) == value / trait.sigma_ref for trait, value in zip(traits_data.traits, values))
        print(
            f"measure_additive_value data_traits doses (g1, f1, g2, p1) {doses} A "
            + " ".join(f"{trait.id} {value!r} bits {struct_bits(value)} G bits {struct_bits(genetic_value(person, trait))}" for trait, value in zip(traits_data.traits, values))
        )
    unit_doses = [(0, 0, 0, 0), (0, 0, 0, 1), (0, 1, 1, 0)]
    unit_genomes = [traits_genome(doses) for doses in unit_doses]
    forward = measure_of(list(zip((1, 2, 3), unit_genomes)), traits_data)
    backward = measure_of(list(zip((3, 2, 1), unit_genomes)), traits_data)
    assert forward["traits"][0][2] != backward["traits"][0][2] and forward["traits"][0][1] == backward["traits"][0][1]
    assert forward["loci"] == backward["loci"] and forward["traits"][1:] == backward["traits"][1:]
    assert measure_of([], traits_data) is None
    for name, measurement in (("keys_1_2_3", forward), ("keys_3_2_1", backward)):
        print(
            f"measure_unit {name} doses (g1, f1, g2, p1) in key order {unit_doses if name == 'keys_1_2_3' else unit_doses[::-1]} living {measurement['living']} "
            + " ".join(f"{trait} mean {mean!r} bits {struct_bits(mean)} sigma {sigma!r} bits {struct_bits(sigma)}" for trait, mean, sigma in measurement["traits"])
            + " loci " + " ".join(f"{locus} {mean_dose!r} bits {struct_bits(mean_dose)}" for locus, mean_dose in measurement["loci"])
        )
        print(f"measure_unit_json {name} clock 0 {measurement_json(0, measurement)}")
    print("measure_unit_empty no person: none")
