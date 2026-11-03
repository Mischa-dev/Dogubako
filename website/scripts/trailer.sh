#!/bin/sh
# Copy only the exact reviewed cut for the release linked by the website.
set -eu
cd "$(dirname "$0")/../.."
# A previous successful copy must not survive a later failed validation.
rm -f website/trailer/dogubako-alpha-trailer.mp4 website/trailer/poster.jpg
python3 website/scripts/trailer_review.py
mkdir -p website/trailer
cp trailer/dist/dogubako-alpha-trailer.mp4 trailer/dist/poster.jpg website/trailer/
echo "trailer copied to website/trailer/"
