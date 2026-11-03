#!/opt/rembg/bin/python
"""`rembg` for the Docker image (DOGUBAKO_WITH_REMBG=1).

The part of rembg's command line that Dogubako's background-removal tool uses,
on top of the rembg library. rembg's own CLI needs its [cli] extra, which adds
a web server and about 200 MB.

    rembg i [-m MODEL] [-a] [-om] INPUT OUTPUT
    rembg --version
"""

import argparse
import os
from importlib.metadata import version


def main() -> None:
    ap = argparse.ArgumentParser(prog="rembg")
    ap.add_argument("--version", action="version", version=f"rembg {version('rembg')}")
    sub = ap.add_subparsers(dest="command", required=True)
    i = sub.add_parser("i", help="remove the background of one image")
    i.add_argument("-m", "--model", default="u2net")
    i.add_argument("-a", "--alpha-matting", action="store_true")
    i.add_argument("-om", "--only-mask", action="store_true")
    i.add_argument("input")
    i.add_argument("output")
    args = ap.parse_args()
    # pymatting's numba functions cache compiled code next to their source by
    # default, and the image's /opt is read-only.
    os.environ.setdefault("NUMBA_CACHE_DIR", os.path.join(os.environ.get("TMPDIR", "/tmp"), "numba"))
    from rembg import new_session, remove

    with open(args.input, "rb") as f:
        data = f.read()
    # The same call rembg's `i` command makes; the alpha-matting thresholds
    # are remove()'s defaults, as they are for the CLI.
    out = remove(
        data,
        session=new_session(args.model),
        alpha_matting=args.alpha_matting,
        only_mask=args.only_mask,
    )
    with open(args.output, "wb") as f:
        f.write(out)


if __name__ == "__main__":
    main()
