/** The storage of the browser, as far as a pinned family needs it. */
export type Storage = Pick<globalThis.Storage, "getItem" | "setItem" | "removeItem">;

const PREFIX = "simagent.pinned.";

/**
 * The family pinned to a run page (ADR-0021 section 6): the lineage key of its person, per
 * run, in the browser's storage (section 2: a followed family is no input and stays in the
 * browser). A storage that is missing or throws (a private window, blocked site data) pins
 * nothing and fails nothing.
 */
export function pinnedKey(storage: Storage | null, run: string): string | null {
  try {
    return storage?.getItem(`${PREFIX}${run}`) ?? null;
  } catch {
    return null;
  }
}

export function pin(storage: Storage | null, run: string, key: string): void {
  try {
    storage?.setItem(`${PREFIX}${run}`, key);
  } catch {
    // Nothing is pinned.
  }
}

export function unpin(storage: Storage | null, run: string): void {
  try {
    storage?.removeItem(`${PREFIX}${run}`);
  } catch {
    // Nothing was pinned.
  }
}

/** A new fork pins the family its run pins (the fourth playtest, K2): the person is the same
 * in the fork, born before it or not yet; where the run pins nothing, the fork neither. */
export function pinFork(storage: Storage | null, run: string, fork: string): void {
  const key = pinnedKey(storage, run);
  if (key === null) {
    unpin(storage, fork);
  } else {
    pin(storage, fork, key);
  }
}

/** The page's storage, if the browser gives one. */
export function browserStorage(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}
