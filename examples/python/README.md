# lyma-python examples

Build the extension into your active Python environment first:

```bash
cd ../..
maturin develop
```

Then run any example from this directory:

```bash
python main.py
python parse_and_format.py
python lyba_roundtrip.py
```

`parse_and_format.py` opens `example.lyma` from disk, then shows the
parser, diagnostics, syntax index, and formatter.
`lyba_roundtrip.py` shows binary LYBA value-image encoding and decoding through
both the top-level module functions and the `lyma.lyba` submodule.
