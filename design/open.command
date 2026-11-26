#!/bin/bash
set -euo pipefail

design_dir=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
exec "$design_dir/../implementation/apps/design-reader/open.command"
