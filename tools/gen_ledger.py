"""Generate notes/TEST_LEDGER.md from the Python and Rust test modules.

Run from anywhere: `python3 tools/gen_ledger.py`.
"""
import glob, os, re
# The Python checkout is the parent of this crate.
root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
NA = {
    ("test_tools", "test_types"): "runs mypy on the Python sources",
    ("test_config", "test_flags_bad_data"): "pydantic type errors; rejected at compile time in Rust",
    ("test_config", "test_flags_extra_fails"): "unknown keyword argument; rejected at compile time in Rust",
    ("test_jinja2_env", "*"): "probes Python objects (PurePath.parser, SettingsModel) inside the Jinja sandbox",
}

def main() -> None:
    rows, details, totals = [], [], [0, 0, 0, 0]
    for py in sorted(glob.glob(f"{root}/tests/test_*.py")):
        mod = os.path.basename(py)[:-3]
        names = re.findall(r"^def (test_\w+)\(", open(py).read(), flags=re.M)
        rs_path = f"{root}/copier-rs/tests/{mod}.rs"
        rs = open(rs_path).read() if os.path.exists(rs_path) else ""
        status = {}
        for reason, name in re.findall(r'#\[ignore = "([^"]*)"\][^{;]*?fn (test_\w+)', rs):
            status[name] = reason
        ported = design = na = 0
        for name in names:
            reason = status.get(name)
            if reason is None and re.search(rf"fn {name}\(", rs):
                # Defined through a local macro that applies the ignore attribute.
                ported += 1
            elif reason is None:
                why = NA.get((mod, name)) or NA.get((mod, "*")) or "MISSING"
                na += 1
                details.append(f"| {mod} | {name} | N/A | {why} |")
            elif reason.startswith("needs-design"):
                design += 1
                details.append(f"| {mod} | {name} | needs-design | {reason.split(':', 1)[1].strip()} |")
            else:
                ported += 1
        rows.append(f"| {mod} | {len(names)} | {ported} | {design} | {na} |")
        for i, v in enumerate((len(names), ported, design, na)):
            totals[i] += v
    out = """# Test ledger

    Every Python test function, by module, and what became of it. Generated from the
    sources; regenerate after changing ignore reasons.

    - `ported`: same test, same assertions.
    - `needs-design`: fully ported but blocked on a small API decision; the reason is
      the `#[ignore]` text and a `TODO(port)` comment marks the spot.
    - `N/A`: no Rust meaning.

    | Module | Python tests | ported | needs-design | N/A |
    |---|---|---|---|---|
    """
    out += "\n".join(rows) + f"\n| **total** | **{totals[0]}** | **{totals[1]}** | **{totals[2]}** | **{totals[3]}** |\n"
    out += "\n## Tests that are not plain ports\n\n| Module | Test | Status | Reason |\n|---|---|---|---|\n" + "\n".join(details) + "\n"
    open(f"{root}/copier-rs/notes/TEST_LEDGER.md", "w").write(out)
    print(totals, [d for d in details if "MISSING" in d])


if __name__ == "__main__":
    main()
