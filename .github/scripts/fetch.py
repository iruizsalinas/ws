"""Downloads for CI scripts, retried: runners sometimes lose DNS or a
connection for a moment. Callers still check every file's digest."""
import time
import urllib.request

ATTEMPTS = 5


def download(url, path):
    for attempt in range(1, ATTEMPTS + 1):
        try:
            with urllib.request.urlopen(url, timeout=120) as response:
                data = response.read()
            break
        except OSError as e:
            if attempt == ATTEMPTS:
                raise
            delay = 2**attempt
            print(f"download of {url} failed ({e}); retrying in {delay}s", flush=True)
            time.sleep(delay)
    path.write_bytes(data)
