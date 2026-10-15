# C++ oracle probes

Scripts that compile parts of the **real C++ sources** against minimal SFML shims
and print golden values used as the compatibility pins for the Rust port. The
values they produce are recorded in
`.agents/plans/001-rust-port/mapping/oracle-vectors.md`.

Run `./run.sh` from the repo root.

| Probe | Pins |
|---|---|
| `pcg_probe.cpp` | `pcg32_fast` raw stream, `uniform_int`/`uniform_real`, `advance`, edge seeds |
| `replay_probe.cpp` | replay file bytes, zlib-compressed bytes, packet bytes |
| `concat_probe.cpp` | `Utils::concat` float formatting, `buildPackId`, `getLevelValidator` |

`shim/` contains the minimal `sf::Packet` / `sf::Vector2` / `sf::Color` /
`sf::Drawable` / `sf::RenderTarget` headers needed to compile these sources.

## Shim fidelity

`shim/` is a minimal reimplementation of the few SFML headers the probes need.
Its fidelity is checked by compiling the gameplay probe **both** ways:

- with `shim/`, and
- with the real SFML fork headers
  (`vittorioromeo/SFML@dacceddbd1adee49c7bfd120266e200800550394`, clone +
  `git checkout <sha> -- include src/SFML/System/Vector2.cpp`) plus the fork's
  `Vector2.cpp`.

Both produce byte-identical probe output (wall vertices, player position, push
result, line-circle codes). The fork is therefore NOT vendored; `shim/` is used.
If a future probe exercises a shim path not yet covered by this comparison, run
the two-way check before trusting the numbers.
