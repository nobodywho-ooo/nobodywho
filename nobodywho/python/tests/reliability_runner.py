#!/usr/bin/env python3
"""Run a Python script repeatedly, retaining output from failed runs."""

import argparse
import math
import os
import signal
import subprocess
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--iterations", "-n", type=int, default=100)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--log-dir", type=Path, default=Path("smoke-test-logs"))
    parser.add_argument("script", type=Path)
    parser.add_argument("script_args", nargs=argparse.REMAINDER)
    arguments = parser.parse_args()

    if (
        arguments.iterations < 1
        or not math.isfinite(arguments.timeout)
        or arguments.timeout <= 0
    ):
        parser.error("iterations and timeout must be positive")
    if not arguments.script.is_file():
        parser.error(f"script not found: {arguments.script}")

    arguments.log_dir.mkdir(parents=True, exist_ok=True)
    failures = 0
    for run_number in range(1, arguments.iterations + 1):
        output_path = arguments.log_dir / f"run-{run_number}.log"
        with output_path.open(mode="w", encoding="utf-8") as log_file:
            process = subprocess.Popen(
                args=[
                    sys.executable,
                    "-u",
                    str(arguments.script),
                    *arguments.script_args,
                ],
                stdout=log_file,
                stderr=subprocess.STDOUT,
                start_new_session=(os.name != "nt"),
                creationflags=subprocess.CREATE_NEW_PROCESS_GROUP
                if os.name == "nt"
                else 0,
            )
            try:
                exit_code = process.wait(timeout=arguments.timeout)
            except subprocess.TimeoutExpired:
                if os.name != "nt":
                    os.killpg(process.pid, signal.SIGKILL)
                else:
                    subprocess.run(
                        args=["taskkill", "/PID", str(process.pid), "/T", "/F"],
                        stdout=log_file,
                        stderr=subprocess.STDOUT,
                        check=True,
                        timeout=10,
                    )
                process.wait()
                log_file.write(f"TIMEOUT after {arguments.timeout:g}s\n")
                print(f"Run {run_number} failed: timeout after {arguments.timeout:g}s")
                return 1

        if exit_code == 0:
            output_path.unlink(missing_ok=True)
            print(f"Run {run_number} succeeded")
        else:
            failures += 1
            with output_path.open(mode="a", encoding="utf-8") as log_file:
                log_file.write(f"exit code {exit_code}\n")
            print(f"Run {run_number} failed: exit code {exit_code}")

    print(
        f"Completed {arguments.iterations} runs: {arguments.iterations - failures} succeeded, {failures} failed"
    )
    return min(failures, 255)


if __name__ == "__main__":
    raise SystemExit(main())
