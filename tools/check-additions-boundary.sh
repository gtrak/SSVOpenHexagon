#!/usr/bin/env bash
# Issue 057 (deliverable 3) — the oh-additions boundary ratchet.
#
#   bash tools/check-additions-boundary.sh
#
# WHAT IT DOES: counts the OURS-class deviation marker lines in the
# PORTED files against the committed baseline (tools/additions-boundary-
# baseline.txt) and FAILS if any file's count has GROWN. A marker line
# is a comment line that references one of the register's seven OURS
# rows (the F28 / F31 / F32 / F33 / F34 / F35 / F36 IDs — the register's
# 057 classification section) or their lanes' references (the 031 / 039
# / 046 / 048 / 054 issue numbers) or this lane's own step notes. The
# scoped files are the ported files the task named (window.rs,
# windowed.rs, menu_game.rs, web.rs, hexagon_game.rs, oh-audio's lib);
# a file that newly becomes a patch target is added to the baseline in
# the commit that starts patching it.
#
# WHAT IT DOES NOT MEAN:
# * It counts MARKER LINES, not code: rewording that drops / adds a
#   reference shifts the count without moving any logic. Tighten the
#   baseline in the commit that rewords (the ratchet's ritual).
# * It does not punish a correct PORT: a FIX row (the F1-F27 / F30
#   lanes) or an INTEROP row (the F5 / F9 / F15 seams) may
#   legitimately add markers to ported files — they carry their own
#   row / lane references, which do not match the pattern.
# * It guards the rows it names: a future lane that classifies a NEW
#   OURS row (say an F37) adds that row's ID + its lane reference to
#   the pattern (below) in the same commit that classifies it — until
#   then the row's first appearance in a ported file is uncounted.
# * It bites only on GROWTH: a move OUT of a ported file (into
#   oh-additions) shrinks the count; the baseline tightens in the same
#   commit, and the ratchet reports the shrink as a note.
#
# On a FAIL it points the reader at the additions crate: the marker's
# code belongs in crates/oh-additions (the boundary contract, the
# crate's lib.rs docs).
#
# Exit code: 0 = no growth (shrink-notes aside), 1 = a count grew.

set -euo pipefail
cd "$(dirname "$0")/.."

BASELINE=tools/additions-boundary-baseline.txt
# The OURS-class marker: the seven OURS rows' IDs (the register's 057
# classification section), their lanes' issue references, this lane's
# own step notes.
PAT='\bF(28|31|32|33|34|35|36)\b|issue 0(31|39|46|48|54)|lane 057|057 step'

fail=0
while read -r want path; do
  case "$want" in
    \#* | '')
      continue
      ;;
  esac
  if [[ -f "$path" ]]; then
    have=$(grep -cE "$PAT" "$path" || true)
  else
    have=0
  fi
  if (( have > want )); then
    echo "FAIL: $path — $have OURS-class marker lines, baseline $want (grew by $(( have - want )))"
    echo "      The marker's code belongs in crates/oh-additions (the boundary contract — the crate's lib.rs). Move it; or, if this is a NEW OURS row, classify it in the register's F-section and add its ID / lane reference to this script's PAT + the baseline in the same commit."
    fail=1
  elif (( have < want )); then
    echo "note: $path — $have marker lines, baseline $want (shrank — tighten the baseline in the commit that moved them)"
  else
    echo "ok:   $path — $have marker lines (baseline)"
  fi
done < "$BASELINE"

exit "$fail"
