#!/usr/bin/env python3
"""Serve the style sample and the cached glb and family.json on one origin.

    python3 art/style-sample/serve.py

Then open http://127.0.0.1:8766/
The page is this directory. /cache/ is /Users/dariushk/.cache/simagent-art,
so the body is /cache/body.glb and the family is /cache/family.json.
"""

from __future__ import annotations

import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlparse

PAGE = Path(__file__).resolve().parent
CACHE = Path("/Users/dariushk/.cache/simagent-art")


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, page: Path, cache: Path, **kwargs):
        self.page = page
        self.cache = cache
        super().__init__(*args, directory=str(page), **kwargs)

    def translate_path(self, path: str) -> str:
        clean = unquote(urlparse(path).path)
        if clean == "/cache" or clean.startswith("/cache/"):
            rest = clean[len("/cache") :].lstrip("/")
            target = (self.cache / rest).resolve()
            try:
                target.relative_to(self.cache.resolve())
            except ValueError:
                return str(self.page / "missing")
            return str(target)
        return super().translate_path(path)


def main() -> None:
    parser = argparse.ArgumentParser(description="Serve the style sample.")
    parser.add_argument("--port", type=int, default=8766)
    parser.add_argument("--cache", type=Path, default=CACHE)
    args = parser.parse_args()
    handler = partial(Handler, page=PAGE, cache=args.cache)
    server = ThreadingHTTPServer(("127.0.0.1", args.port), handler)
    print(f"http://127.0.0.1:{args.port}/", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
