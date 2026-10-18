#!/usr/bin/env bash
# Persistent lane worktrees for parallel subagent lanes.
#
# Operator directive (D-035, amended): lanes run in PERSISTENT worktrees — one
# stable branch + worktree per lane — so that parallel writers never share a
# working tree, an index, or a build directory. The earlier worktree pain came
# from EPHEMERAL worktrees whose refs were deleted while lanes were live (which
# left commits dangling and needed 4 recoveries); nothing here deletes anything
# while a lane is alive, and `remove` refuses unless the branch is merged.
#
# Usage:
#   tools/lane-worktree.sh create <lane> [base]   # branch lane/<lane> off base (default: master)
#   tools/lane-worktree.sh list
#   tools/lane-worktree.sh status <lane>          # diffstat + untracked + last commit
#   tools/lane-worktree.sh env <lane>             # print the env a lane must export
#   tools/lane-worktree.sh land <lane>            # print the landing commands (does NOT run them)
#   tools/lane-worktree.sh remove <lane>          # REFUSES unless merged into master
#
# Worktrees live OUTSIDE the repo (${REPO%/}/../oh-lanes/<lane>) so the main
# checkout's greps/finds/censuses and `git status` stay clean.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# If this script is run from INSIDE a lane worktree, the line above resolves REPO to
# that worktree, so "$REPO/.." is oh-lanes/ itself and the lane path doubles
# (/…/oh-lanes/oh-lanes/043 — reported by lane 043). The lanes dir always lives
# beside the MAIN checkout, so resolve that through git's common dir when possible.
_git_common="$(git -C "$REPO" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)"
if [ -n "$_git_common" ]; then
  _main="$(cd "$(dirname "$_git_common")" && pwd)"
  [ -d "$_main" ] && REPO="$_main"
fi
LANES_DIR="$(cd "$REPO/.." && pwd)/oh-lanes"
LANE="${2:-}"

die() { echo "error: $*" >&2; exit 1; }

require_lane() {
  [ -n "$LANE" ] || die "usage: $0 <create|list|status|env|land|remove> [lane] [base]"
}

path_for() { echo "$LANES_DIR/$1"; }
branch_for() { echo "lane/$1"; }

cmd_create() {
  require_lane
  local base="${3:-master}" wt; wt="$(path_for "$LANE")"
  git -C "$REPO" rev-parse --verify --quiet "$base" >/dev/null || die "base '$base' not found"
  [ -e "$wt" ] && die "$wt already exists (persistent — use 'status'/'land', do not recreate)"
  if git -C "$REPO" show-ref --verify --quiet "refs/heads/$(branch_for "$LANE")"; then
    die "branch $(branch_for "$LANE") already exists — reuse it: git worktree add '$wt' $(branch_for "$LANE")"
  fi
  mkdir -p "$LANES_DIR"
  git -C "$REPO" worktree add -b "$(branch_for "$LANE")" "$wt" "$base"
  echo
  echo "created: $wt  (branch $(branch_for "$LANE") off $base)"
  cmd_env "$@"
}

cmd_list() {
  git -C "$REPO" worktree list
  echo
  echo "lanes dir: $LANES_DIR"
}

cmd_status() {
  require_lane
  local wt; wt="$(path_for "$LANE")"
  [ -d "$wt" ] || die "no worktree at $wt"
  echo "=== $wt ($(branch_for "$LANE"))"
  git -C "$wt" log --oneline -3
  echo "--- diff vs master ---"
  git -C "$wt" diff --stat master...HEAD | tail -20
  echo "--- working tree ---"
  git -C "$wt" status --short | head -20
}

cmd_env() {
  require_lane
  local wt; wt="$(path_for "$LANE")"
  cat <<EOF
#
# Export these in the lane. The target dir is the lane's OWN (inside its
# worktree): separate worktrees have different absolute source paths, so a
# SHARED target dir would thrash cargo's fingerprints (every alternation looks
# like a full rebuild). Disk is the reason for the two profile overrides: the
# main checkout's target/ is 63 GB, almost all debug info — three of those would
# exhaust the disk, so lanes build without debug symbols (does not change test
# semantics, only backtrace quality).
#
export CARGO_TARGET_DIR="$wt/target"
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_PROFILE_DEV_INCREMENTAL=false
cd "$wt"
#
# Parallel-lane rules (see .agents/WORKER-HYGIENE.md):
#   * your worktree has its own index and files — 'git add -A' here can no longer
#     sweep another lane's work (that was the shared-checkout hazard)
#   * still commit with explicit paths where practical, and never force-remove a
#     worktree or branch: your branch is PERSISTENT and the orchestrator lands it
#   * a cold first build is expensive; prefer targeted checks
#     ('cargo test -p <crate>') over the whole workspace while other lanes run
#   * if the disk gets tight, report it — do not 'cargo clean' another lane's dir
EOF
}

cmd_land() {
  require_lane
  local wt; wt="$(path_for "$LANE")"; local br; br="$(branch_for "$LANE")"
  [ -d "$wt" ] || die "no worktree at $wt"
  echo "=== $br vs master"
  git -C "$REPO" log --oneline master.."$br"
  echo "--- diffstat ---"
  git -C "$REPO" diff --stat master..."$br"
  cat <<EOF
--- NOT run automatically; the orchestrator decides (D-025: squash landing).
    To land on master:
      git -C "$REPO" checkout master
      git -C "$REPO" merge --squash $br && git -C "$REPO" commit
    To preserve per-item commits:
      git -C "$REPO" cherry-pick <sha>...
    Then (only after the branch is fully merged AND the lane is closed):
      git -C "$REPO" worktree remove '$wt'     # refuses if dirty
      git -C "$REPO" branch -d $br             # -d refuses if unmerged
EOF
}

cmd_remove() {
  require_lane
  local wt; wt="$(path_for "$LANE")"; local br; br="$(branch_for "$LANE")"
  [ -d "$wt" ] || die "no worktree at $wt"
  git -C "$wt" diff --quiet && git -C "$wt" diff --cached --quiet \
    || die "$wt has uncommitted changes — commit them first (persistent: nothing is force-deleted)"
  # D-025 lands a lane by SQUASH, so the branch tip is never an ancestor of master and the
  # plain ancestor test refuses every correctly-landed lane. Accept a squash landing too:
  # every path the branch touched must be byte-identical between master and the branch tip.
  local how
  if git -C "$REPO" merge-base --is-ancestor "$br" master; then
    how="ancestor"
  elif [ "${LANE_FORCE:-0}" = "1" ]; then
    how="FORCED by LANE_FORCE=1"
  else
    local base paths
    base="$(git -C "$REPO" merge-base master "$br")"
    paths="$(git -C "$REPO" diff --name-only "$base" "$br")"
    if [ -n "$paths" ] && git -C "$REPO" diff --quiet master "$br" -- $paths; then
      how="squash landing (all $(echo "$paths" | wc -l) touched paths byte-identical in master)"
    else
      die "$br is NOT merged into master - no ancestor, and its touched paths differ. Refusing (this is the discipline that prevents lost commits). If you already landed it as a squash and master has since moved under it, re-run with LANE_FORCE=1."
    fi
  fi
  git -C "$REPO" worktree remove "$wt"
  git -C "$REPO" branch -D "$br" >/dev/null
  echo "removed $wt and $br (merged: $how)"
}

case "${1:-}" in
  create) cmd_create "$@";;
  list)   cmd_list;;
  status) cmd_status "$@";;
  env)    cmd_env "$@";;
  land)   cmd_land "$@";;
  remove) cmd_remove "$@";;
  *) sed -n '2,25p' "${BASH_SOURCE[0]}"; exit 1;;
esac
