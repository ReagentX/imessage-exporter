#!/bin/sh
# Launch the iMessage Exporter UI, building it first if needed.
set -e
cd "$(dirname "$0")"

if [ ! -x ./imessage-exporter-ui ]; then
    echo "Building UI..."
    swiftc -O -o imessage-exporter-ui main.swift
fi

open ./imessage-exporter-ui 2>/dev/null || (nohup ./imessage-exporter-ui >/dev/null 2>&1 &)
