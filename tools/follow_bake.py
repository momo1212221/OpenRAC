# SPDX-License-Identifier: GPL-3.0-or-later
"""Show an existing lighting bake's durable log; never start or stop the bake."""
import argparse
import subprocess
import sys
import time
from pathlib import Path


def follow(cache: Path, baker: Path, output=sys.stdout, interval=0.75):
    log = cache / 'probe-bake.log'
    with log.open('rb') as stream:
        size = log.stat().st_size
        stream.seek(max(0, size - 128 * 1024))
        if stream.tell():
            stream.readline()  # Start on a complete line of the bounded history.
        print('Following existing lighting preparation. Stopping this viewer leaves the bake running.',
              file=output, flush=True)
        while True:
            chunk = stream.read(64 * 1024)
            if chunk:
                output.write(chunk.decode('utf-8', errors='replace'))
                output.flush()
                continue
            status = subprocess.run([str(baker), '--cache-status', str(cache)],
                                    capture_output=True, text=True, check=True, timeout=10)
            if status.stdout.strip() == 'idle':
                # Include the final messages written between the read and the lock check.
                output.write(stream.read().decode('utf-8', errors='replace'))
                print('\nLighting preparation is no longer running.', file=output, flush=True)
                return
            if status.stdout.strip() != 'busy':
                raise RuntimeError('The baker returned an unknown cache status')
            if log.stat().st_size < stream.tell():
                stream.seek(0)
            time.sleep(interval)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache-dir', type=Path, required=True)
    parser.add_argument('--baker', type=Path, required=True)
    args = parser.parse_args()
    follow(args.cache_dir, args.baker)


if __name__ == '__main__':
    main()
