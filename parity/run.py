"""Differential check: run Python Copier and copier-rs on the same templates and
compare the generated trees.

Usage (from the crate root, after `cargo build --release`):

    python3 parity/run.py [--keep]

Needs the Python Copier checkout in the parent directory with its virtualenv
(`uv sync --frozen` there). Files are compared byte for byte, except YAML and JSON,
which are compared as parsed data (the two YAML emitters may format differently).
"""
import filecmp
import json
import os
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
RANDOM_FILES = {"config.py"}

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
        elif name in RANDOM_FILES:
            continue
        elif not filecmp.cmp(a, b, shallow=False):
            structured = name.endswith((".yml", ".yaml", ".json"))
            if not structured or load_structured(a) is None or load_structured(a) != load_structured(b):
                yield f"{rel}: contents differ"
        elif os.stat(a).st_mode != os.stat(b).st_mode:
            yield f"{rel}: modes differ"


def main() -> int:
    for binary in (PY_COPIER, RS_COPIER):
        if not os.path.exists(binary):
            print(f"missing {binary}", file=sys.stderr)
            return 2
    keep = "--keep" in sys.argv
    failures = 0
    with tempfile.TemporaryDirectory(prefix="copier-parity.") as root:
        for name, template, extra in CASES:
            outputs = {}
            for label, binary in (("python", PY_COPIER), ("rust", RS_COPIER)):
                destination = os.path.join(root, name, label)
                command = [binary, "copy", "--quiet", "--defaults", "--trust", *extra, os.path.join(FIXTURES, template), destination]
                done = subprocess.run(command, capture_output=True, text=True, stdin=subprocess.DEVNULL)
                outputs[label] = (done.returncode, destination)
            (py_code, py_dir), (rs_code, rs_dir) = outputs["python"], outputs["rust"]
            if py_code != rs_code:
                differences = [f"exit codes differ: python {py_code}, rust {rs_code}"]
            elif py_code != 0:
                differences = []
            else:
                differences = list(compare(py_dir, rs_dir))
            status = "ok" if not differences else "DIFFERENT"
            print(f"{name}: {status} (exit {py_code})")
            for difference in differences:
                print(f"    {difference}")
            failures += bool(differences)
        if keep:
            print(f"outputs kept in {root}")
            input("press Enter to delete them ")
    print(json.dumps({"cases": len(CASES), "different": failures}))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
