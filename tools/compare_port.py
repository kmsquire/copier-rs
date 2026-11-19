"""Compare a Python test module with its Rust port: test names and per-test assertion counts.

A heuristic: assertions are counted with regular expressions, so helper functions and
multi-line macros can make a faithful port look short. Read the flagged tests.
"""
import os, re, sys
# The Python checkout is the parent of this crate.
root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
for mod in sys.argv[1:]:
    py = open(f"{root}/tests/{mod}.py").read()
    rs = open(f"{root}/copier-rs/tests/{mod}.rs").read()
    def split(src, pat):
        parts = re.split(pat, src, flags=re.M)
        return {parts[i]: parts[i + 1] for i in range(1, len(parts) - 1, 2)}
    pyt = {k: re.split(r"^@pytest", v, flags=re.M)[0] for k, v in split(py, r"^def (test_\w+)\(").items()}
    rst = split(rs, r"^(?:pub )?fn (test_\w+)\(")
    ign = {v: k for k, v in re.findall(r'#\[ignore = "([^"]*)"\][^{]*?fn (test_\w+)', rs)}
    print(f"== {mod}: py {len(pyt)} tests, rs {len(rst)} tests")
    for name, body in pyt.items():
        pa = len(re.findall(r"^\s*assert\b", body, flags=re.M)) + len(re.findall(r"pytest\.(raises|warns|deprecated_call)", body)) + len(re.findall(r"\.expect_exact\(", body))
        if name not in rst:
            print(f"  MISSING {name} (py asserts {pa})"); continue
        rb = rst[name]
        ra = len(re.findall(r"\bassert(_eq|_ne|_err|_file)?!?\(", rb)) + len(re.findall(r"\.expect_(exact|eof)\(", rb)) + len(re.findall(r"expect_prompt\(", rb))
        pa += len(re.findall(r"expect_prompt\(", body)) + len(re.findall(r"assert_file\(", body))
        flag = "" if ra >= pa else "  <-- FEWER"
        tag = ign.get(name, "NOT IGNORED")
        if flag or tag != "pending":
            print(f"  {name}: py {pa} rs {ra} [{tag}]{flag}")
    for name in rst:
        if name not in pyt: print(f"  EXTRA {name}")
