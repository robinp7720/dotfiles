#!/usr/bin/env bash

set -euo pipefail

pkill -x polybar >/dev/null 2>&1 || true
pkill -x superpaper >/dev/null 2>&1 || true
