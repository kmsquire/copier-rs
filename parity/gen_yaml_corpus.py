"""Generate the PyYAML differential corpus used by `tests/yaml_corpus.rs`.

Run with the Python Copier environment:
    ../.venv/bin/python parity/gen_yaml_corpus.py > tests/fixtures/yaml_corpus.json
"""

import datetime
import json
import sys

import yaml

LOAD_CASES = [
    "a: 1", "a: yes\nb: No\nc: on\nd: OFF\ne: y\nf: n", "- 010\n- 0x1f\n- 0b101\n- 1_000\n- 190:20:30\n- 0o17\n- 09\n- +12\n- -0",
    "- 1.5\n- 1e5\n- 1.5e3\n- 1.5e+3\n- .5\n- 5.\n- .inf\n- -.INF\n- 1_0.2_5\n- 1:30.5\n- +.5", "- ~\n- null\n- Null\n- NULL\n-\n- None\n- nil",
    "- 2020-01-02\n- 2020-1-2\n- 2001-12-14t21:59:43.10-05:00\n- 2001-12-14 21:59:43\n- 2001-12-15 2:59:43.1Z\n- 2020-01-02x",
    "a: 'yes'\nb: \"1\"\nc: !!str 1\nd: !!int '010'\ne: !!float 1\nf: ! 12\ng: !!bool 'no'\nh: !!null x",
    "base: &b {x: 1, y: 2}\nother: &o {y: 9, z: 3}\nm:\n  <<: [*b, *o]\n  x: 5\n  w: 0\nn:\n  w: 0\n  <<: *b",
    "a: 1\nb: 2\na: 3", "1: a\ntrue: b\n~: c\n1.5: d", "a: |\n  line1\n  line2\nb: >\n  folded\n  text\n\n  para\nc: |-\n  strip\nd: |+\n  keep\n\n",
    "", "# just a comment", "---\n", "a:\n  - b\n  -\n    - c\n    - d: e\n      f: [g, {h: i}]", "? a\n: b\n? [1]\n: c" if False else "? a\n: b",
    "a: \"\\x41\\u00e9\\n\\t\\\\\"\nb: 'it''s'\nc: \"multi\n  line\"\nd: plain\n  continued", "a: {b: c, d: [e, f], g: }\nh: [ ]\ni: { }",
    "emoji: 🎤\numlaut: ä\ncjk: 日本", "a: b: c", "a: [1, 2", "- !!python/name:os.system x", "a: !unknown 1", "key: value\n  bad indent: here",
    "'quoted key': 1\n\"dq key\": 2\n? complex\n: 3", "a: 1 # comment\nb: 'x # not comment'\nc: x#y", "- - - deep\n- - a\n  - b",
    "a: -\nb: - x\nc: -1\nd: '-'", "str: 0o14\ncolon: a:b\nurl: http://x.y/z?q=1#frag\ntime: 12:30:45\nver: 1.2.3\ne: 1e3\ntilde: ~x",
    "a: &x 1\nb: *x\nc: &y [1, 2]\nd: *y", "%YAML 1.1\n---\na: 1", "a: 1\n---\nb: 2", "a: 1\n...\n", "- 123456789012345678901234567890\n- 0x7fffffffffffffff",
    "a: =\nb: '<<'\nc: <<", "\ta: 1", "a:\tb", "a: 'unterminated", "[a, b]: c", "{a: 1}: c", "a: *undefined",
]

DUMP_VALUES = [
    {"_commit": "v1.0.0", "_src_path": "gh:org/tpl", "name": "x", "flag": True, "n": 3, "f": 1.5, "none": None},
    {"a": {"b": [1, {"x": 1, "y": [2, 3]}, [4, 5]], "c": {}, "d": []}},
    ["yes", "No", "on", "y", "1", "1.0", "", "a: b", "a #b", "a#b", "- x", "-x", "it's", "null", "~", "a\nb", "@x", "2020-01-01", " x", "x ", "x:", ":x", "a:b"],
    ["0x10", "010", "1e5", "1.5e+3", ".5", "1_000", "1:30", "<<", "=", "---", "...", "--- x", "?", "? x", "?x", "!x", "&x", "*x", "|", ">", "%x", "`x", "[x", "]x", "{x", "}x", "x,y", ",x", "'q'", '"q"', "#x", "x #", "true", "True", "TRUE", "nan", ".nan", ".inf", "Null"],
    "foo", "yes", "", None, True, 3, 1.5, [], {}, [[]], [{}], {"a": []},
    [1.0, 1e16, 0.5, 1e-7, -2.5, float("inf"), float("-inf"), 123456789.123456789, 1e100, 0.0, -0.0],
    {"b": [1, "x, y"], "a": {"k": "v"}}, {"emoji": "🎤", "umlaut": "ä", "tab": "a\tb", "nbsp": "a\u00a0b", "ctrl": "a\x01b", "bs": "a\\b", "dq": 'a"b', "both": "a'b\"c"},
    {"multi": "line1\nline2\n", "trail": "x\n", "lead": "\nx", "crlf": "a\r\nb"},
    {1: "a", 2: "b", 10: "c"}, {"b": 1, "a": 2, "C": 3, "_x": 4, "1": 5}, [[1, 2], [3, [4, [5]]]], [{"a": 1, "b": 2}, {"c": [1, {"d": 2}]}],
    {"long": "word " * 30}, {"nested": {"deep": {"deeper": {"deepest": [1, 2, {"k": "v"}]}}}}, {"key with space": 1, "key:colon": 2, "": 3, "yes": 4, "1": 5, "- dash": 6},
    {"date": datetime.date(2020, 1, 2), "dt": datetime.datetime(2020, 1, 2, 3, 4, 5)},
    ["a b", "a  b", "a, b", "[a]", "{a}", "a]b", "a?b", "a&b", "a*b", "a!b", "a|b", "a>b", "a%b", "a@b", "a`b"],
]

DUMP_OPTIONS = [
    {"name": "nice", "kw": {"indent": 4, "allow_unicode": True, "default_flow_style": False}},
    {"name": "block2", "kw": {"allow_unicode": True, "default_flow_style": False}},
    {"name": "auto", "kw": {"allow_unicode": True, "default_flow_style": None}},
    {"name": "flow", "kw": {"default_flow_style": True, "width": 2**31 - 1}},
]


def jsonable(value):
    if isinstance(value, (datetime.date, datetime.datetime)):
        return {"__timestamp__": str(value)}
    if isinstance(value, float) and (value != value or value in (float("inf"), float("-inf"))):
        return {"__float__": repr(value)}
    if isinstance(value, dict):
        return {"__map__": [[jsonable(k), jsonable(v)] for k, v in value.items()]}
    if isinstance(value, list):
        return [jsonable(v) for v in value]
    return value


def main():
    loads = []
    for text in LOAD_CASES:
        try:
            docs = list(yaml.safe_load_all(text))
            loads.append({"text": text, "docs": [jsonable(d) for d in docs]})
        except Exception as exc:  # noqa: BLE001
            loads.append({"text": text, "error": type(exc).__name__})
    dumps = []
    for value in DUMP_VALUES:
        for option in DUMP_OPTIONS:
            dumper = yaml.SafeDumper if option["name"] == "flow" else yaml.Dumper
            dumps.append({"value": jsonable(value), "style": option["name"], "text": yaml.dump(value, Dumper=dumper, **option["kw"])})
    json.dump({"loads": loads, "dumps": dumps}, sys.stdout, ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
