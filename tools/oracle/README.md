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
