# luma-python examples

Build the extension into your active Python environment first:

```bash
cd ..
maturin develop
```

Then run any example from this directory:

```bash
python main.py
python parse_and_format.py
python lumba_roundtrip.py
```

`parse_and_format.py` opens `complex_config.luma` from disk, then shows the
parser, diagnostics, syntax index, and formatter.
`lumba_roundtrip.py` shows binary LUMBA value-image encoding and decoding through
both the top-level module functions and the `luma_python.lumba` submodule.
