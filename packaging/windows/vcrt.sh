#!/bin/bash
# On ribs02 as radmin: the Visual C++ 2022 runtime DLLs (msvc-runtime wheel) for the bundled Python.
cd /srv/build/win/vcrt || exit 1
rm -rf x && mkdir x && for w in *.whl; do unzip -oq "$w" -d x; done
find x -iname "*.dll" | head -20
