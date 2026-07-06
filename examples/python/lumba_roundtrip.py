from __future__ import annotations

import luma as luma


VALUES = [
    None,
    True,
    42,
    3.25,
    "hello",
    ["nested", 1, False],
    {"name": "Ada", "active": True},
    {"__luma_tag__": "example", "value": {"kind": "tagged"}},
]


def run() -> None:
    image = luma.to_lumba_value_image(VALUES)
    restored = luma.from_lumba_value_image(image)

    print("Encoded bytes:", len(image))
    print("Restored values:")
    for value in restored:
        print(" ", repr(value))

    nested_image = luma.lumba.write_lumba_value_image({"from_submodule": True})
    nested_values = luma.lumba.read_lumba_value_image(nested_image)
    print("Submodule restored:", nested_values)


if __name__ == "__main__":
    run()
