"""Un-ignore ported tests that pass.

Usage: python3 tools/unignore.py [--dry-run] test_module [...]

Runs each module with `--include-ignored`, then removes `#[ignore = "pending"]`
from every test function with no failing case. Tests ignored for another reason
(`needs-design: ...`) are left alone. Network tests are skipped while probing and
keep their ignore attribute.
"""
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PENDING = '#[ignore = "pending"]'


def run(module: str) -> tuple[set[str], set[str], str]:
    out = subprocess.run(
        ["cargo", "test", "--test", module, "--", "--include-ignored"],
        cwd=ROOT, capture_output=True, text=True, timeout=3600,
    ).stdout
    failed = {m.split("::")[0] for m in re.findall(r"^test (\S+) \.\.\. FAILED", out, flags=re.M)}
    passed = {m.split("::")[0] for m in re.findall(r"^test (\S+) \.\.\. ok", out, flags=re.M)}
    return passed - failed, failed, out


def main() -> None:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    dry = "--dry-run" in sys.argv
    for module in args:
        path = os.path.join(ROOT, "tests", f"{module}.rs")
        src = open(path).read()
        impure = set(re.findall(r"// impure[^\n]*\n(?:[^\n]*\n){0,12}?(?:pub )?fn (test_\w+)", src))
        passed, failed, out = run(module)
        lines, result, changed = src.split("\n"), [], 0
        for index, line in enumerate(lines):
            if line.strip() == PENDING:
                name = None
                for following in lines[index + 1 : index + 40]:
                    found = re.match(r"\s*(?:pub )?fn (test_\w+)", following)
                    if found:
                        name = found.group(1)
                        break
                if name in passed and name not in impure:
                    changed += 1
                    continue
            result.append(line)
        if not dry and changed:
            open(path, "w").write("\n".join(result))
        summary = re.search(r"test result: .*", out)
        print(f"{module}: un-ignored {changed}; failing: {sorted(failed)}; {summary.group(0)[:60] if summary else 'NO RESULT'}")


if __name__ == "__main__":
    main()
