"""Keep docs/reading-otio.md honest: its worked example must parse, and its
Python snippet, run on that example, must print exactly the output the guide
shows.
"""
import contextlib
import io
import json
import re
from pathlib import Path

GUIDE = Path(__file__).resolve().parents[3] / "docs" / "reading-otio.md"


def fenced_blocks(text: str, section: str) -> list[tuple[str, str]]:
    """The (language, body) of each fenced code block in `## section`."""
    body = text.split(f"## {section}\n", 1)[1].split("\n## ", 1)[0]
    return re.findall(r"```(\w*)\n(.*?)```", body, flags=re.S)


def test_worked_example_parses_and_snippet_output_matches(tmp_path, monkeypatch):
    from tellers_timeline import Timeline

    guide = GUIDE.read_text()
    [(lang, example)] = fenced_blocks(guide, "A worked example")
    assert lang == "json"
    json.loads(example)
    Timeline.parse_json(example)

    snippet_blocks = fenced_blocks(guide, "Inspecting a file from Python")
    [snippet] = [body for lang, body in snippet_blocks if lang == "python"]
    [expected] = [body for lang, body in snippet_blocks if lang == ""]

    # The snippet opens "promo.otio" from the working directory.
    (tmp_path / "promo.otio").write_text(example)
    monkeypatch.chdir(tmp_path)
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        exec(compile(snippet, str(GUIDE), "exec"), {})
    assert out.getvalue() == expected, (
        "the output shown under 'Inspecting a file from Python' in "
        "docs/reading-otio.md no longer matches what the snippet prints"
    )
