"""Differential check: run Python Copier and copier-rs on the same templates and
compare the generated trees.

Usage (from the crate root, after `cargo build --release`):

    python3 parity/run.py [--keep]

Needs the Python Copier checkout in the parent directory with its virtualenv
(`uv sync --frozen` there). Files are compared byte for byte, except YAML and JSON,
which are compared as parsed data (the two YAML emitters may format differently).
"""
import filecmp
import os
import shutil
import subprocess
import sys
import tempfile

CRATE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PYTHON_REPO = os.path.dirname(CRATE)
PY_COPIER = os.path.join(PYTHON_REPO, ".venv", "bin", "copier")
PY_PYTHON = os.path.join(PYTHON_REPO, ".venv", "bin", "python")
RS_COPIER = os.path.join(CRATE, "target", "release", "copier")
FIXTURES = os.path.join(CRATE, "tests", "fixtures")

DEMO_DATA = ["-d", "py3=true", "-d", "myvar=awesome", "-d", "what=world", "-d", "project_name=Copier",
             "-d", "version=2.0.0", "-d", "description=A library for rendering projects templates"]

# Files whose content is random by design (`make_secret()`), so they never match.
RANDOM_FILES = {"config.py"}  # paths relative to the generated project

# Cases both implementations must refuse (the template names Jinja extensions
# that do not exist).
EXPECTED_FAILURES = {"demo_merge_options"}

# (name, template directory, extra arguments)
CASES = [
    ("demo", "demo", DEMO_DATA),
    ("demo-exclude-option", "demo", DEMO_DATA + ["-x", "*.txt"]),
    ("demo_exclude", "demo_exclude", []),
    ("demo_exclude_negate", "demo_exclude_negate", []),
    ("demo_extensions_default", "demo_extensions_default", []),
    ("demo_skip_src", "demo_skip_src", []),
    ("demo_data", "demo_data", []),
    ("demo_transclude", "demo_transclude", []),
    ("demo_merge_options", "demo_merge_options_from_answerfiles", []),
]


def load_structured(path):
    """Parse a YAML or JSON file with PyYAML; None when it is not parseable."""
    code = "import sys, json, yaml; print(json.dumps(yaml.safe_load(open(sys.argv[1], encoding='utf-8')), default=str, sort_keys=True))"
    result = subprocess.run([PY_PYTHON, "-c", code, path], capture_output=True, text=True)
    return result.stdout if result.returncode == 0 else None


def compare(left, right, relative=""):
    """Yield a description of every difference between two directory trees."""
    names = sorted(set(os.listdir(left)) | set(os.listdir(right)))
    for name in names:
        a, b, rel = os.path.join(left, name), os.path.join(right, name), os.path.join(relative, name)
        if not os.path.lexists(a) or not os.path.lexists(b):
            yield f"{rel}: only in {'python' if os.path.lexists(a) else 'rust'}"
        elif os.path.islink(a) or os.path.islink(b):
            if not (os.path.islink(a) and os.path.islink(b)) or os.readlink(a) != os.readlink(b):
                yield f"{rel}: symlinks differ"
        elif os.path.isdir(a) != os.path.isdir(b):
            yield f"{rel}: file versus directory"
        elif os.path.isdir(a):
            yield from compare(a, b, rel)
        else:
            if os.stat(a).st_mode != os.stat(b).st_mode:
                yield f"{rel}: modes differ"
            if rel in RANDOM_FILES or filecmp.cmp(a, b, shallow=False):
                continue
            parsed = load_structured(a) if name.endswith((".yml", ".yaml", ".json")) else None
            if parsed is None or parsed != load_structured(b):
                yield f"{rel}: contents differ"


def run_case(root, name, template, extra):
    """Render one template with both implementations and list the differences."""
    runs = {}
    for label, binary in (("python", PY_COPIER), ("rust", RS_COPIER)):
        destination = os.path.join(root, name, label)
        command = [binary, "copy", "--quiet", "--defaults", "--trust", *extra,
                   os.path.join(FIXTURES, template), destination]
        done = subprocess.run(command, capture_output=True, text=True, stdin=subprocess.DEVNULL)
        runs[label] = (done.returncode, destination, done.stderr.strip().splitlines()[-1:] or [""])
    (py_code, py_dir, py_error), (rs_code, rs_dir, rs_error) = runs["python"], runs["rust"]
    if py_code != rs_code:
        return py_code, [f"exit codes differ: python {py_code}, rust {rs_code} ({rs_error[0]})"]
    if py_code != 0:
        # Both refuse the template. Their wording differs by nature (a Python
        # traceback versus a message), so only cases listed as expected failures pass.
        if name in EXPECTED_FAILURES:
            return py_code, []
        return py_code, [f"both fail: python {py_error[0]!r}, rust {rs_error[0]!r}"]
    missing = [label for label, path in (("python", py_dir), ("rust", rs_dir)) if not os.path.isdir(path)]
    if missing:
        return py_code, [f"no output directory from {', '.join(missing)}"]
    return py_code, list(compare(py_dir, rs_dir))


def main() -> int:
    for binary in (PY_COPIER, RS_COPIER):
        if not os.path.exists(binary):
            print(f"missing {binary}", file=sys.stderr)
            return 2
    keep = "--keep" in sys.argv
    root = tempfile.mkdtemp(prefix="copier-parity.")
    failures = 0
    for name, template, extra in CASES:
        code, differences = run_case(root, name, template, extra)
        outcome = "DIFFERENT" if differences else ("ok" if code == 0 else "ok, both refuse it")
        print(f"{name}: {outcome} (exit {code})")
        for difference in differences:
            print(f"    {difference}")
        failures += bool(differences)
    if keep:
        print(f"outputs kept in {root}")
    else:
        shutil.rmtree(root, ignore_errors=True)
    print(f"{len(CASES)} cases, {failures} different")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
