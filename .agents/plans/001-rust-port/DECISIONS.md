# DECISIONS — SSVOpenHexagon Rust port

A running log of binding technical decisions, one per settled question. Each entry
records: the question, the decision, the evidence (real source), and the issue that
landed it. Decisions here are authoritative for workers; a worker that finds code
contradicting one must report it rather than silently diverge.

## D-000 — Port is literal, arithmetic-exact

- **Decision:** mirror C++ structure; `float`→`f32`, `double`→`f64`; no
  reordering of floating-point expressions; no "clean" API redesigns.
- **Evidence:** replay determinism depends on exact evaluation (plan 001 §Ground rules).
- **Landed by:** all issues.

## D-001 — Workspace layout

- **Decision:** single Cargo workspace at repo root; crates under `crates/` as
  listed in `PLAN.md`; binary crates `open-hexagon`, `oh-server-control`,
  `oh-workshop-uploader`.
- **Landed by:** issue 01.

## D-002 — Vendored C++ libs are folded, not crate-per-lib

- **Decision:** SSVUtils/SSVStart/SSVMenuSystem analogues live inside
  `oh-math`/`oh-timeline`/`oh-render`/`oh-menu-system`, not as standalone crates
  (they are thin and only used here).
- **Landed by:** issues 01, 10, 12.

## D-003 — PCG32 must be bit-exact

- **Decision:** reproduce `pcg32_fast` exactly (state/multiplier/increment/seeding)
  plus the exact `std::uniform_int_distribution` / `uniform_real_distribution`
  semantics used, so replay RNG streams match the C++ build.
- **Evidence:** mapping/`libs-ssv.md` (PCG section) + mapping/`gameplay.md`.
- **Landed by:** issue 01.

## D-004 — Third-party substitution table

- **Status:** candidates recorded in `PLAN.md`; each is finalised by the issue that
  first depends on it, and the decision is written here with evidence.
- Pending: SFML analogue (issue 10), Lua runtime (06), JSON (03), zlib (02),
  sodium (16), SQLite (17), ImGui (14), Steam (20), Discord (21).

## D-005 — In-repo skills are written from real source

- **Decision:** every library (vendored C++, Rust crate, system lib) gets a skill
  in `.agents/skills/` written from its real source; skills are authoritative for
  workers; a code/skill contradiction triggers a minimal factual skill correction
  reported under "skill corrections".
- **Landed by:** each issue that first touches a library.

## D-006 — Worker dispatch settings (infrastructure)

- **Decision:** dispatch implementation workers at `thinking=medium`
  (`model: "local/local:medium"`) and keep concurrency at 1–2.
- **Evidence:** during plan 001 execution, `xhigh` thinking produced per-worker
  contexts of 7.7M tokens and hour-long silent generations; the `tinyllb` KV
  bin-pack admission gate then wedged (`current_kv=0.0` but
  `ema_kv_per_flow=0.9059 > max_threshold=0.90`), holding **every** request
  forever — workers reported "no activity" while consuming **0 tokens**.
  Restarting `tinyllb` (`systemctl --user restart tinyllb.service`) cleared it
  and latency went 55s → 0.3s. Diagnosis used
  `journalctl --user -u tinyllb.service` (look for
  `kv bin-pack: holding — projected over threshold`) and
  `curl :8000/metrics | grep num_running_reqs` (direct-to-engine vs through-LB
  latency comparison).
- **Do NOT edit `~/.config/tinyllb/config.yaml`** to work around this — it is the
  user's infrastructure. Reduce worker thinking instead, and restart the service
  if it wedges.
