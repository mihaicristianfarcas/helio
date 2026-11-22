#!/usr/bin/env python3
"""Known answers for analyze and compare: synthetic run folders whose
figures are known by construction.

    python3 tools/bench/test_tools.py

A tool that has not reproduced its known answer measures nothing (the
benchmarks' rule), so this runs before a series is read.
"""
import contextlib
import importlib.machinery
import importlib.util
import io
import os
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))


def load(name):
    loader = importlib.machinery.SourceFileLoader(name, os.path.join(HERE, name))
    spec = importlib.util.spec_from_loader(name, loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


analyze = load("analyze")
compare = load("compare")


def write(path, text):
    with open(path, "w") as f:
        f.write(text)


def make_run(root, name, clicks_ms, frames, presented, fps=60, wire_mbps=20.0):
    """A run folder as run-mac leaves it.

    clicks_ms: latency of clicks 0, 1, 2... (the first two are left out).
    frames: (display time in ms, counter) of every composited frame.
    presented: the frame numbers the host's scene presented.
    """
    run = os.path.join(root, name)
    os.makedirs(run)
    write(os.path.join(run, "meta.txt"),
          f"stack=helio mode=1920x1080@{fps} kbps=20000 vsync=vsync pacing=nopacing\nfirst_video_s=1.50\n")
    rows = ["click,down_ns,flash_ns,latency_ms,arrival_ms"]
    for i, ms in enumerate(clicks_ms):
        rows.append(f"{i},{i * 10**9},{i * 10**9 + int(ms * 1e6)},{ms:.3f},{ms:.3f}")
    write(os.path.join(run, "flash.csv"), "\n".join(rows) + "\n")
    rows = ["display_ns,arrived_ns,counter"] + [f"{int(t * 1e6)},{int(t * 1e6)},{c}" for t, c in frames]
    write(os.path.join(run, "counter.csv"), "\n".join(rows) + "\n")
    write(os.path.join(run, f"{name}-counter-scene.csv"),
          "".join(f"present,{f * 1000},0,{f},0\n" for f in presented))
    write(os.path.join(run, "wire.csv"), f"phase,bytes,seconds,mbps\ncounter,0,20.0,{wire_mbps}\n")
    return run


class Analyze(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name

    def tearDown(self):
        self.tmp.cleanup()

    def test_clicks_leave_out_the_first_two_and_give_the_median_and_95th_percentile(self):
        # Clicks 2..41 take 10, 11, ... 49 ms: median 29.5; the 95th
        # percentile by analyze's rank rule is the 38th of 40 (index 37): 47.
        run = make_run(self.root, "r1", [999, 999] + [10.0 + i for i in range(40)], [(0, 0), (16.7, 1)], [0, 1])
        r = analyze.row(run)
        self.assertEqual(r["clicks"], 40)
        self.assertAlmostEqual(r["click_ms"], 29.5)
        self.assertAlmostEqual(r["click_p95"], 47.0)

    def test_every_frame_shown_on_time_is_100_percent_with_no_hitch_or_freeze(self):
        period = 1000 / 60
        frames = [(i * period, i) for i in range(120)]
        r = analyze.row(make_run(self.root, "r1", [20] * 4, frames, range(120)))
        self.assertAlmostEqual(r["counter_shown_pct"], 100.0)
        self.assertAlmostEqual(r["counter_hitch_pct"], 0.0)
        self.assertEqual(r["counter_freezes_per_min"], 0.0)
        self.assertEqual(r["counter_frozen_pct"], 0.0)
        self.assertEqual(r["counter_damaged_pct"], 0.0)

    def test_a_gap_of_ten_frames_is_one_hitch_and_one_freeze_of_its_length(self):
        # Frames 50..59 never shown: 110 of 120 presented ones seen, one gap
        # of 11 periods among 109 gaps, which is both a hitch and a freeze.
        period = 1000 / 60
        frames = [(i * period, i) for i in range(120) if not 50 <= i < 60]
        r = analyze.row(make_run(self.root, "r1", [20] * 4, frames, range(120)))
        self.assertAlmostEqual(r["counter_shown_pct"], 100.0 * 110 / 120)
        self.assertAlmostEqual(r["counter_hitch_pct"], 100.0 / 109)
        self.assertAlmostEqual(r["counter_freeze_max_ms"], 11 * period, places=3)
        self.assertAlmostEqual(r["counter_freeze_median_ms"], 11 * period, places=3)
        span = 119 * period
        self.assertAlmostEqual(r["counter_freezes_per_min"], 1 / (span / 60000), places=3)
        self.assertAlmostEqual(r["counter_frozen_pct"], 100.0 * 11 * period / span, places=3)

    def test_a_gap_of_two_frames_is_a_hitch_but_not_a_freeze(self):
        period = 1000 / 60
        frames = [(i * period, i) for i in range(60) if i != 30]
        r = analyze.row(make_run(self.root, "r1", [20] * 4, frames, range(60)))
        self.assertGreater(r["counter_hitch_pct"], 0)
        self.assertEqual(r["counter_freezes_per_min"], 0.0)

    def test_unreadable_and_older_counters_are_damaged_frames(self):
        # 100 composited frames: 4 unreadable (-1) and 1 older than one shown.
        period = 1000 / 60
        frames = [(i * period, i) for i in range(95)]
        frames += [(95 * period, -1), (96 * period, -1), (97 * period, -1), (98 * period, -1), (99 * period, 10)]
        r = analyze.row(make_run(self.root, "r1", [20] * 4, frames, range(95)))
        self.assertAlmostEqual(r["counter_damaged_pct"], 5.0)
        self.assertAlmostEqual(r["counter_shown_pct"], 100.0)

    def test_wire_rate_is_read_as_written(self):
        r = analyze.row(make_run(self.root, "r1", [20] * 4, [(0, 0), (16.7, 1)], [0, 1], wire_mbps=27.25))
        self.assertAlmostEqual(r["wire_counter_mbps"], 27.25)


class Compare(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name

    def tearDown(self):
        self.tmp.cleanup()

    def runs(self, prefix, offset):
        # Five runs whose clicks spread 10..30 ms around a run-to-run phase.
        out = []
        for k in range(5):
            lat = [999, 999] + [offset + 10 + (i * 8 + k * 3) % 21 for i in range(40)]
            out.append(make_run(self.root, f"{prefix}{k}", lat, [(0, 0), (16.7, 1)], [0, 1]))
        return out

    def verdict(self, left, right):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            compare.main(left + ["--"] + right)
        return buf.getvalue().strip().splitlines()[-1]

    def test_the_same_clicks_on_both_sides_match_with_an_interval_holding_zero(self):
        a, b = self.runs("a", 0), self.runs("b", 0)
        line = self.verdict(a, b)
        self.assertIn("difference +0.00 ms", line)
        self.assertTrue(line.endswith("match"), line)

    def test_every_click_5_ms_slower_on_the_left_is_a_5_ms_difference_with_the_right_faster(self):
        a, b = self.runs("a", 5), self.runs("b", 0)
        line = self.verdict(a, b)
        self.assertIn("difference +5.00 ms", line)
        self.assertTrue(line.endswith("right faster"), line)

    def test_a_difference_smaller_than_the_spread_within_runs_matches(self):
        a, b = self.runs("a", 0.5), self.runs("b", 0)
        self.assertTrue(self.verdict(a, b).endswith("match"))


if __name__ == "__main__":
    unittest.main()
