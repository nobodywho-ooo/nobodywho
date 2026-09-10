#!/usr/bin/env bash

download_model() (
    set -euo pipefail
    url=""
    output=""
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --url) url="${2:?missing URL}"; shift 2 ;;
            --output) output="${2:?missing output path}"; shift 2 ;;
            *) echo "Unknown download option: $1" >&2; exit 2 ;;
        esac
    done
    if [[ -z "$url" || -z "$output" ]]; then
        echo "Usage: download-model.sh --url URL --output PATH" >&2
        exit 2
    fi

    if [[ -s "$output" ]]; then exit 0; fi
    mkdir -p "$(dirname "$output")"
    partial="${output}.part"
    trap 'rm -f "$partial"' EXIT
    deadline=$((SECONDS + 900))
    for attempt in {1..6}; do
        remaining=$((deadline - SECONDS))
        if (( remaining <= 0 )); then break; fi
        if (( remaining > 300 )); then remaining=300; fi
        # curl --retry rewinds; a new invocation resumes the partial file.
        if curl --location --fail --show-error --silent \
            --connect-timeout 30 --max-time "$remaining" \
            --speed-limit 1024 --speed-time 60 \
            --continue-at - --output "$partial" "$url"; then
            test -s "$partial"
            mv -f "$partial" "$output"
            echo "Downloaded $output"
            exit 0
        else
            status=$?
        fi
        if (( attempt == 6 )); then exit "$status"; fi
        if (( status == 33 || status == 36 )); then rm -f "$partial"; fi
        echo "Download attempt $attempt failed for $output (curl $status)" >&2
        sleep "$((5 * attempt))"
    done
    echo "Download timed out: $output" >&2
    exit 1
)

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    download_model "$@"
fi
