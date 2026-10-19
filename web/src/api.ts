/**
 * What an answer must say it is (ADR-0021 section 2): what the server builds carries
 * `format` and `version`, a file of the command line its `format_version`. The viewer
 * shows nothing of an answer whose kind or version it does not know.
 */
export type Expected = { format: string; version: number } | { format_version: number };

export type Json = { [key: string]: unknown };

/** An answer of another kind or version, or no JSON object at all. */
export class UnknownAnswer extends Error {
  override name = "UnknownAnswer";
}

/** The answer as a JSON object if it is of the expected kind and version. */
export function accept(body: unknown, expected: Expected): Json {
  if (typeof body !== "object" || body === null || Array.isArray(body)) {
    throw new UnknownAnswer("the answer is no JSON object");
  }
  const json = body as Json;
  const fits =
    "format" in expected
      ? json["format"] === expected.format && json["version"] === expected.version
      : json["format_version"] === expected.format_version;
  if (!fits) {
    const came = JSON.stringify({
      format: json["format"],
      version: json["version"],
      format_version: json["format_version"],
    });
    throw new UnknownAnswer(`expected ${JSON.stringify(expected)}, came ${came}`);
  }
  return json;
}

/** Reads an answer of the local server, which must be of the expected kind. */
export async function getJson(path: string, expected: Expected): Promise<Json> {
  if (!path.startsWith("/api/")) {
    throw new Error(`not a route of the server: ${path}`);
  }
  const response = await fetch(path, { headers: { Accept: "application/json" } });
  if (!response.ok) {
    throw new Error(`${path}: ${response.status}`);
  }
  return accept(await response.json(), expected);
}
