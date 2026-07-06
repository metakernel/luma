from __future__ import annotations

import json
from pathlib import Path

import luma_python as luma


EXAMPLE_FILE = Path(__file__).with_name("example.luma")


def run() -> None:
    source = EXAMPLE_FILE.read_text(encoding="utf-8")
    parsed = luma.parse_str(1, EXAMPLE_FILE.name, source)

    print("Luma version:", luma.version())
    print("Opened:", EXAMPLE_FILE.name)
    print("Documents:", parsed["document_count"])
    print("Diagnostics:", json.dumps(parsed["diagnostics"], indent=2))
    print("Syntax node count:", len(parsed["syntax_index"]))
    if parsed["syntax_index"]:
        print("First syntax node:", json.dumps(parsed["syntax_index"][0], indent=2))

    formatted = luma.format_str(1, EXAMPLE_FILE.name, source)
    print("Formatter changed input:", formatted["changed"])
    print("Formatted preview:")
    print("\n".join(formatted["text"].splitlines()[:20]))


if __name__ == "__main__":
    run()
