#!/usr/bin/env python3
"""Offline tests for tools/terrain/ingest_dem.py on synthetic grids (no real elevation data, no GDAL).
Run: python3 tools/test_ingest_dem.py"""
import hashlib
import importlib.util
import os
import struct
import subprocess
import sys
import tempfile
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(TOOLS, "terrain", "ingest_dem.py")
spec = importlib.util.spec_from_file_location("ingest_dem", SCRIPT)
ing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ing)
VOID = ing.VOID


def asc(path, rows_north_first, xll=1000, yll=2000, cell=1, nodata=-9999):
    with open(path, "w", encoding="ascii") as f:
        f.write("ncols %d\nnrows %d\nxllcorner %s\nyllcorner %s\ncellsize %s\nNODATA_value %d\n" % (
            len(rows_north_first[0]), len(rows_north_first), xll, yll, cell, nodata))
        for r in rows_north_first:
            f.write(" ".join(str(v) for v in r) + "\n")


def decode(dem):
    return list(struct.unpack("<%di" % (len(dem) // 4), dem))


class Merge(unittest.TestCase):
    def test_layout_rounding_and_void(self):
        r = ing.Raster(1, 0, 0, [[1.234, 1.235, None], [-0.004, 0.005, 10.0]])  # row 0 is south
        dem, cov, rep = ing.ingest([r], 3, 2)
        self.assertEqual(decode(dem), [123, 124, VOID, 0, 1, 1000])
        self.assertEqual(list(cov), [1, 1, 0, 1, 1, 1])
        self.assertEqual((rep["void_posts"], rep["void_ppm"]), (1, 166667))
        self.assertEqual(dem[:4], struct.pack("<i", 123))

    def test_overlap_lowest_id_wins_and_voids_fall_through(self):
        a = ing.Raster(2, 0, 0, [[5.0, 5.0]])
        b = ing.Raster(1, 0, 0, [[None, 3.0]])
        for order in ([a, b], [b, a]):
            dem, cov, rep = ing.ingest(order, 2, 1)
            self.assertEqual(decode(dem), [500, 300])
            self.assertEqual(list(cov), [2, 1])
            self.assertEqual(rep["posts_by_source"], {1: 1, 2: 1})

    def test_offsets_and_clipping(self):
        r = ing.Raster(7, -1, 1, [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]])
        dem, cov, _ = ing.ingest([r], 2, 3)
        self.assertEqual(decode(dem), [VOID, VOID, 200, 300, 500, 600])
        self.assertEqual(list(cov), [0, 0, 7, 7, 7, 7])

    def test_refusals(self):
        ok = ing.Raster(1, 0, 0, [[1.0]])
        for bad in ([ing.Raster(0, 0, 0, [[1.0]])], [ing.Raster(256, 0, 0, [[1.0]])], [ok, ing.Raster(1, 0, 0, [[2.0]])],
                    [ing.Raster(1, 0, 0, [[30_000_000.0]])]):
            with self.assertRaises(ing.IngestError):
                ing.ingest(bad, 1, 1)
        with self.assertRaises(ing.IngestError):
            ing.ingest([ok], 0, 1)
        with self.assertRaises(ing.IngestError):
            ing.Raster(1, 0, 0, [[1.0], [1.0, 2.0]])


class Files(unittest.TestCase):
    def test_asc_reader(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "a.asc")
            asc(p, [[3, 4], [1, -9999]], xll=1002, yll=2001)
            r = ing.read_asc(1, p, 1000, 2000)
            self.assertEqual((r.x0, r.y0), (2, 1))
            self.assertEqual(r.rows, [[1.0, None], [3.0, 4.0]])

    def test_asc_refusals(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "a.asc")
            for kwargs in ({"cell": 2}, {"xll": 1000.5}):
                asc(p, [[1, 2]], **kwargs)
                with self.assertRaises(ing.IngestError):
                    ing.read_asc(1, p, 1000, 2000)
            with open(p, "w") as f:
                f.write("ncols 2\nnrows 1\nxllcenter 0\nyllcorner 0\ncellsize 1\n1 2\n")
            with self.assertRaises(ing.IngestError):
                ing.read_asc(1, p, 0, 0)
            with open(p, "w") as f:
                f.write("ncols 3\nnrows 1\nxllcorner 0\nyllcorner 0\ncellsize 1\n1 2\n")
            with self.assertRaises(ing.IngestError):
                ing.read_asc(1, p, 0, 0)

    def run_cli(self, d, out, extra=()):
        a, b = os.path.join(d, "a.asc"), os.path.join(d, "b.asc")
        asc(a, [[2, 2, 2], [1, 1, 1]], xll=0, yll=0)
        asc(b, [[9, 9], [9, 9]], xll=2, yll=0)
        return subprocess.run([sys.executable, "-I", SCRIPT, "--origin-e", "0", "--origin-n", "0", "--width", "4", "--height", "3",
                               "--out", out, "--source", "1=" + a, "--source", "2=" + b, *extra], capture_output=True, text=True)

    def test_cli_writes_hashed_outputs_twice_identically(self):
        with tempfile.TemporaryDirectory() as d:
            outs = []
            for n in ("o1", "o2"):
                out = os.path.join(d, n)
                r = self.run_cli(d, out)
                self.assertEqual(r.returncode, 0, r.stderr)
                outs.append(out)
            for name in ("dem_cm.i32", "coverage.u8", "ingest.toml"):
                with open(os.path.join(outs[0], name), "rb") as f1, open(os.path.join(outs[1], name), "rb") as f2:
                    self.assertEqual(f1.read(), f2.read(), name)
            with open(os.path.join(outs[0], "dem_cm.i32"), "rb") as f:
                dem = f.read()
            with open(os.path.join(outs[0], "coverage.u8"), "rb") as f:
                cov = f.read()
            self.assertEqual(len(dem), 4 * 12)
            self.assertEqual(list(cov), [1, 1, 1, 2, 1, 1, 1, 2, 0, 0, 0, 0])
            self.assertEqual(decode(dem)[:4], [100, 100, 100, 900])  # row 0 is south; source 1 wins where it has values
            with open(os.path.join(outs[0], "ingest.toml"), encoding="ascii") as f:
                toml = f.read()
            self.assertIn(hashlib.sha256(dem).hexdigest(), toml)
            self.assertIn(hashlib.sha256(cov).hexdigest(), toml)
            self.assertIn("void_ppm", toml)

    def test_cli_refuses_tiff_without_gdal_and_bad_source(self):
        with tempfile.TemporaryDirectory() as d:
            try:
                import osgeo  # noqa: F401
                self.skipTest("GDAL present")
            except ImportError:
                pass
            t = os.path.join(d, "x.tif")
            open(t, "wb").close()
            r = subprocess.run([sys.executable, "-I", SCRIPT, "--origin-e", "0", "--origin-n", "0", "--width", "1", "--height", "1",
                                "--out", os.path.join(d, "o"), "--source", "1=" + t], capture_output=True, text=True)
            self.assertEqual(r.returncode, 2)
            self.assertIn("GDAL", r.stderr)
            self.assertFalse(os.path.exists(os.path.join(d, "o", "dem_cm.i32")))
            r = subprocess.run([sys.executable, "-I", SCRIPT, "--origin-e", "0", "--origin-n", "0", "--width", "1", "--height", "1",
                                "--out", os.path.join(d, "o"), "--source", "oops"], capture_output=True, text=True)
            self.assertEqual(r.returncode, 2)


if __name__ == "__main__":
    unittest.main()
