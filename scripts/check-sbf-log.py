from pathlib import Path
import re
import sys

text = re.sub(r"\x1b\[[0-9;]*m", "", Path(sys.argv[1]).read_text())
errors = [
    line
    for line in text.splitlines()
    if line.lstrip().startswith("Error:")
    or "undefined and not known syscalls" in line
    or "Calling them will trigger a run-time error" in line
]
if errors:
    raise SystemExit("SBF diagnostics rejected:\n" + "\n".join(errors))
