"""Put `#[ignore]` directly above `fn`: rstest attaches an attribute that precedes a `#[case]` to that case only."""
import re, sys
for path in sys.argv[1:]:
    lines = open(path).read().split("\n")
    out, pending = [], None
    changed = False
    i = 0
    while i < len(lines):
        line = lines[i]
        if re.match(r'#\[ignore = "[^"]*"\]$', line):
            # look ahead: is there a #[case before the fn?
            j = i + 1
            has_case = False
            while j < len(lines) and not re.match(r'(pub )?fn ', lines[j]):
                if lines[j].startswith("#[case"):
                    has_case = True
                j += 1
            if has_case:
                pending = line
                changed = True
                i += 1
                continue
        if pending and re.match(r'(pub )?fn ', line):
            out.append(pending)
            pending = None
        out.append(line)
        i += 1
    if changed:
        open(path, "w").write("\n".join(out))
        print("fixed", path)
