from tellers_timeline import Timeline


def test_round_trip_simple():
    with open("spec/examples/simple.json", "r") as f:
        data = f.read()
    tl = Timeline.parse_json(data)
    errs = tl.validate()
    assert not errs
    out = tl.to_json()
    tl2 = Timeline.parse_json(out)
    assert tl2.to_json() == out
