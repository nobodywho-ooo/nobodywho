#!/usr/bin/env bash
# Re-push the remaining release tags one-by-one, sequenced so the
# SHA-keyed workflow concurrency group never cancels a pending run.
# Each tag is re-created on the current main commit.
#
# Run from the repo root. Takes a few hours — the runs serialize
# on the same-SHA concurrency group. Safe to Ctrl-C and re-run:
# already-in-progress runs are not cancelled by later pushes.
set -euo pipefail
cd "$(dirname "$0")"

SHA=$(git rev-parse origin/main)
echo "Tagging on: $SHA ($(git log -1 --format='%s' $SHA))"
echo

wait_started () {
    local tag=$1 id status i
    for i in $(seq 1 240); do   # up to 2 h (may queue behind the previous run)
        sleep 30
        id=$(gh api 'repos/nobodywho-ooo/nobodywho/actions/runs?per_page=10' \
            --jq ".workflow_runs[] | select(.head_branch == \"$tag\") | .id" | head -1)
        status=$(gh api "repos/nobodywho-ooo/nobodywho/actions/runs/$id" --jq .status)
        echo "  [$tag] run $id: $status ($(date +%H:%M))"
        if [ "$status" = "in_progress" ]; then return 0; fi
    done
    echo "  [$tag] TIMED OUT waiting for run to start" >&2
    return 1
}

for tag in nobodywho-python-v2.0.0 nobodywho-flutter-v3.0.0 \
           nobodywho-kotlin-v3.0.0 nobodywho-react-native-v3.0.0; do
    echo "=== $tag ==="
    git tag -d "$tag" 2>/dev/null || true
    git push origin ":refs/tags/$tag" 2>/dev/null || true
    git tag "$tag" "$SHA"
    git push origin "refs/tags/$tag"
    sleep 45   # let the run be created before polling
    wait_started "$tag"
    echo "  [$tag] in progress — safe to dispatch the next tag"
    echo
done
echo "All four tags dispatched. Track with: gh run list --limit 10"
