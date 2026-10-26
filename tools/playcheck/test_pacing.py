"""Synthetic tests: each pacing rule fires on a planted fault and stays quiet on a good run."""
import random
import sys
import tempfile
import unittest
from pathlib import Path

import animgen
import offscreen
import pacing


def report(intervals_ms, ticks_per_frame=None, render_ms=2.0, work_ms=2.0, flags=1):
    """A `perf 2` report with the given frame intervals, ticks spread by time."""
    frames, ticks, render, t = [], [], [], 1_000_000
    tick_ms = 1000 / 60
    next_tick = t
    for i, iv in enumerate(intervals_ms):
        us = int(iv * 1000)
        w = int(work_ms * 1000)
        frames.append([us, 100, w // 2, w // 2, t, flags])
        render.append(int(render_ms * 1000))
        if ticks_per_frame is not None:
            for k in range(ticks_per_frame(i)):
                ticks.append([len(ticks) + 1, 0, 100, 0, t + 1 + k])
        else:
            while next_tick < t + us:
                ticks.append([len(ticks) + 1, 0, 100, 0, next_tick])
                next_tick += int(tick_ms * 1000)
        t += us
    return {"format": "perf 2", "frames": frames, "ticks": ticks, "render": render}


def rules(result):
    return sorted({v["rule"] for v in result["violations"]})


class Pacing(unittest.TestCase):
    def test_a_held_limit_passes(self):
        rnd = random.Random(1)
        r = pacing.analyse(report([16.667 + rnd.uniform(-0.2, 0.2) for _ in range(600)]), 60)
        self.assertEqual(rules(r), [], r["violations"])
        self.assertTrue(r["metrics"]["cap_reachable"])

    def test_a_limit_that_does_not_limit(self):
        r = pacing.analyse(report([8.0] * 900), 60)
        self.assertIn("cap-exceeded", rules(r))
        self.assertGreater(r["violations"][0]["fps"], 100)

    def test_a_missed_limit_with_headroom(self):
        r = pacing.analyse(report([25.0] * 400), 60)
        self.assertIn("cap-missed", rules(r))

    def test_a_slow_machine_is_unreachable_not_missed(self):
        r = pacing.analyse(report([40.0] * 300, render_ms=38.0), 60)
        self.assertFalse(r["metrics"]["cap_reachable"])
        self.assertNotIn("cap-missed", rules(r))

    def test_jitter(self):
        r = pacing.analyse(report([10.0, 23.333] * 300), 60)
        self.assertIn("jitter", rules(r))

    def test_stall(self):
        iv = [16.667] * 600
        iv[400] = 180.0
        r = pacing.analyse(report(iv), 60)
        self.assertEqual(rules(r), ["stall"])
        self.assertEqual(r["violations"][0]["worst"][0]["ms"], 180.0)

    def test_skipped_ticks(self):
        # 60 fps, but every other frame runs three ticks and the next none.
        r = pacing.analyse(report([16.667] * 600, ticks_per_frame=lambda i: 3 if i % 2 else 0), 60)
        self.assertIn("skipped-ticks", rules(r))
        # 30 fps with two ticks a frame is what that rate needs.
        r = pacing.analyse(report([33.333] * 300, ticks_per_frame=lambda i: 2), 30)
        self.assertNotIn("skipped-ticks", rules(r))

    def test_smooth_costs_more(self):
        good = {"metrics": {"interval_ms": {"p95": 17.0}, "fps": 60}}
        bad = {"metrics": {"interval_ms": {"p95": 30.0}, "fps": 40}}
        found = pacing.compare_modes([{"limit": 60, "mode": "original", "analysis": good},
                                      {"limit": 60, "mode": "smooth", "analysis": bad}])
        self.assertEqual([v["rule"] for v in found], ["smooth-costs-more"])
        self.assertEqual(pacing.compare_modes([{"limit": 60, "mode": "original", "analysis": bad},
                                               {"limit": 60, "mode": "smooth", "analysis": good}]), [])

    def test_too_short(self):
        self.assertIn("error", pacing.analyse(report([16.0] * 5), 60))


class Animgen(unittest.TestCase):
    def test_generated_file_names(self):
        self.assertTrue(animgen.generated("cache/animation/v1/ab.d2frames"))
        self.assertTrue(animgen.generated("x/y.d2frames"))
        self.assertFalse(animgen.generated("cache/atlas/page0.bin"))


class Settings(unittest.TestCase):
    def test_settings_file(self):
        text = offscreen.settings_toml({"vsync": False, "frame_limit": 60, "animation_frames": "smooth"})
        self.assertEqual(text, f'version = {offscreen.SETTINGS_VERSION}\n[video]\nvsync = false\nframe_limit = 60\n'
                               'animation_frames = "smooth"\n')

    def test_settings_files_are_the_version_the_game_writes(self):
        # A file older than the game's own version is read the old way (a
        # version-7 file ignores `hd_text`), so every tool that writes one
        # must write the version app/config.rs writes, not a copy of it.
        rust = offscreen.CONFIG_RS.read_text(encoding="utf-8")
        line = next(ln for ln in rust.splitlines() if ln.startswith("pub const SETTINGS_VERSION: i64 = "))
        game = int(line.rsplit("=", 1)[1].strip(" ;"))
        self.assertEqual(offscreen.SETTINGS_VERSION, game)
        self.assertEqual(offscreen.settings_toml({}).splitlines()[0], f"version = {game}")
        sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "perf"))
        import frame_pacing
        case = frame_pacing.parse_case("n60s")
        self.assertEqual(frame_pacing.settings_toml(case).splitlines()[0], f"version = {game}")

    def test_the_version_reader_wants_exactly_one_constant(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "config.rs"
            path.write_text("pub const SETTINGS_VERSION: i64 = 12;\n", encoding="utf-8")
            self.assertEqual(offscreen.game_settings_version(path), 12)
            path.write_text("const OTHER: i64 = 3;\n", encoding="utf-8")
            with self.assertRaises(RuntimeError):
                offscreen.game_settings_version(path)


if __name__ == "__main__":
    unittest.main()
