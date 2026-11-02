#!/bin/sh
# Copy the trailer (built in trailer/, see trailer/README) next to the page.
# The video is ~24 MB, so it lives once in trailer/dist and is copied at deploy
# time instead of being committed twice. Run from the repository root.
set -eu
mkdir -p website/trailer
cp trailer/dist/dogubako-alpha-trailer.mp4 trailer/dist/poster.jpg website/trailer/
echo "trailer copied to website/trailer/"
