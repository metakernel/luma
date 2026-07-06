from __future__ import annotations

import lyma as lyma


VALUES = [
    None,
    True,
    42,
    3.25,
    "hello",
    ["nested", 1, False],
    {"name": "Ada", "active": True},
    {"__lyma_tag__": "example", "value": {"kind": "tagged"}},
]


def run() -> None:
    image = lyma.to_lyba_value_image(VALUES)
    restored = lyma.from_lyba_value_image(image)

    print("Encoded bytes:", len(image))
    print("Restored values:")
    for value in restored:
        print(" ", repr(value))

    nested_image = lyma.lyba.write_lyba_value_image({"from_submodule": True})
    nested_values = lyma.lyba.read_lyba_value_image(nested_image)
    print("Submodule restored:", nested_values)


if __name__ == "__main__":
    run()
