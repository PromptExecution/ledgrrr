#!/usr/bin/env bash
# Entry point for the SysML v2 Pilot Implementation "conformance oracle" container.
#
# Runs the reference implementation's own standalone SysML2JSON utility
# (org.omg.sysml.xtext.util.SysML2JSON) against an input .sysml/.kerml file, resolving
# cross-references (e.g. ScalarValues::Integer) against the bundled standard library.
# A clean exit + generated .json means the reference implementation accepted the model
# as conformant; a non-zero exit / stack trace means it rejected it — that reject/accept
# signal is the "oracle" this tool exists to provide.
#
# Usage: podman run -v /path/to/models:/work:Z <image> <input-file-relative-to-/work>
set -euo pipefail

if [ "$#" -lt 1 ]; then
  echo "Usage: $0 <input-file-under-/work> [extra SysML2JSON args...]" >&2
  exit 2
fi

INPUT="$1"
shift

exec java -cp /app/sysml-kernel-all.jar org.omg.sysml.xtext.util.SysML2JSON \
  -l /app/sysml.library \
  "$INPUT" \
  "Kernel Libraries" "Systems Library" "Domain Libraries" \
  "$@"
