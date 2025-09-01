import argparse
import sys
from pathlib import Path


def ensure_local_package_on_path() -> None:
    base_dir = Path(__file__).resolve().parents[1]  # bindings/python
    pkg_dir = base_dir / "python"  # bindings/python/python
    sys.path.insert(0, str(pkg_dir))


def main() -> int:
    ensure_local_package_on_path()
    from tellers_timeline import Timeline

    parser = argparse.ArgumentParser(
        description="Load, sanitize, and print an OTIO JSON file"
    )
    parser.add_argument(
        "input",
        nargs="?",
        default=str(Path(__file__).parents[1].resolve() / "simple.json"),
        help="Path to input OTIO JSON (default: spec/examples/simple.json)",
    )
    parser.add_argument(
        "--output",
        "-o",
        default=None,
        help="Optional path to write sanitized JSON (prints to stdout if omitted)",
    )
    args = parser.parse_args()

    data = Path(args.input).read_text()
    tl = Timeline.parse_json(data)
    tl.sanitize()
    out = tl.to_json_with_precision(precision=2, pretty=True)

    if args.output:
        Path(args.output).write_text(out)
    else:
        print(out)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
