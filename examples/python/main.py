from __future__ import annotations

import lumba_roundtrip
import parse_and_format


def main() -> None:
    print("== Parse and format ==")
    parse_and_format.run()
    print()
    print("== LUMBA round-trip ==")
    lumba_roundtrip.run()


if __name__ == "__main__":
    main()
