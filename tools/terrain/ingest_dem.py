#!/usr/bin/env python3
"""Elevation ingest (SPEC-014 sections 3 and 10): rasters in, `dem_cm.i32` and `coverage.u8` out, both hashed.

This is the only floating-point step of the terrain pipeline; everything after it is integer and a pure function of
the hashed outputs. The frame is local: post (i, j) is the 1 m post at `(origin_e + i, origin_n + j)` metres, row 0 is
the southernmost row, `dem_cm.i32` holds little-endian `i32` centimetres with void `i32::MIN`, and `coverage.u8`
holds the source id of the post (0 none).

Rules. Sources are applied in ascending id and the first valid value wins, so overlaps are deterministic. A
nodata or non-finite value is void; a value that does not fit `i32` (void excluded) is refused. Source ids are
1 to 255 because coverage is one byte. A source raster must have 1 m posts, a north-up grid and a lower-left
corner on a whole metre of the local frame; resampling is not done here, so such a source is refused rather than
guessed. Assumption (unverified): a 1 m raster cell with lower-left corner `(e, n)` supplies post `(e, n)`, which
shifts the surface by half a metre against cell-centre registration; that is below the 4 m cell of the pack.

Inputs: ESRI ASCII grids (`.asc`, read here with the standard library) and GeoTIFF (`.tif`, read through GDAL when
the `osgeo` package is importable; the GDAL path is not exercised by the offline tests). The ingest never reads a
sources manifest and states nothing about licences or coverage claims: those are recorded by a person in
`data/survey/<survey>/sources.toml`.

Usage:
  ingest_dem.py --origin-e E --origin-n N --width W --height H --out DIR --source ID=PATH [--source ID=PATH ...]
Writes DIR/dem_cm.i32, DIR/coverage.u8 and DIR/ingest.toml (sizes, per-source post counts, void ppm, sha256 of both
files, GDAL version when used). Exit 0 on success, 2 on a refused input."""
import argparse
import hashlib
import math
import os
import struct
import sys

VOID = -(2 ** 31)
I32_MAX = 2 ** 31 - 1


class IngestError(Exception):
    """An input that is refused (never guessed at)."""


class Raster:
    """A north-up grid of 1 m posts: `(x0, y0)` is the local offset in metres of the lower-left cell, `rows[0]` is
    the southernmost row and holds floats (metres) or None for nodata."""

    def __init__(self, source_id, x0, y0, rows):
        if not rows or not rows[0] or any(len(r) != len(rows[0]) for r in rows):
            raise IngestError("source %d: empty or ragged raster" % source_id)
        self.source_id, self.x0, self.y0, self.rows = source_id, x0, y0, rows


def _cm(v):
    if v is None or not math.isfinite(v):
        return VOID
    c = math.floor(v * 100 + 0.5)
    if not (VOID < c <= I32_MAX):
        raise IngestError("elevation %r m does not fit i32 centimetres" % v)
    return int(c)


def read_asc(source_id, path, origin_e, origin_n):
    """Reads an ESRI ASCII grid and returns a Raster in the local frame."""
    header, rows = {}, []
    with open(path, encoding="ascii") as f:
        for line in f:
            parts = line.split()
            if not parts:
                continue
            key = parts[0].lower()
            if len(parts) == 2 and key in ("ncols", "nrows", "xllcorner", "yllcorner", "cellsize", "nodata_value",
                                           "xllcenter", "yllcenter"):
                header[key] = parts[1]
            else:
                rows.append([float(p) for p in parts])
    if "xllcenter" in header or "yllcenter" in header:
        raise IngestError("source %d: cell-centre registration is not supported; use xllcorner and yllcorner" % source_id)
    for k in ("ncols", "nrows", "xllcorner", "yllcorner", "cellsize"):
        if k not in header:
            raise IngestError("source %d: header lacks %s" % (source_id, k))
    ncols, nrows = int(header["ncols"]), int(header["nrows"])
    if float(header["cellsize"]) != 1.0:
        raise IngestError("source %d: cellsize %s is not 1 m; resampling is not done here" % (source_id, header["cellsize"]))
    xll, yll = float(header["xllcorner"]) - origin_e, float(header["yllcorner"]) - origin_n
    if xll != int(xll) or yll != int(yll):
        raise IngestError("source %d: lower-left corner is not on a whole metre of the local frame" % source_id)
    if len(rows) != nrows or any(len(r) != ncols for r in rows):
        raise IngestError("source %d: data do not match ncols and nrows" % source_id)
    nodata = float(header["nodata_value"]) if "nodata_value" in header else None
    rows = [[None if (nodata is not None and v == nodata) else v for v in r] for r in reversed(rows)]
    return Raster(source_id, int(xll), int(yll), rows)


def read_geotiff(source_id, path, origin_e, origin_n):
    """Reads a GeoTIFF through GDAL (lazy import). Same refusals as `read_asc`."""
    try:
        from osgeo import gdal
    except ImportError as e:
        raise IngestError("source %d: GDAL (osgeo) is not available, cannot read %s" % (source_id, path)) from e
    ds = gdal.Open(path)
    if ds is None:
        raise IngestError("source %d: GDAL cannot open %s" % (source_id, path))
    gt = ds.GetGeoTransform()
    if gt[2] != 0 or gt[4] != 0 or gt[1] != 1.0 or gt[5] != -1.0:
        raise IngestError("source %d: not a north-up 1 m grid" % source_id)
    band = ds.GetRasterBand(1)
    nodata = band.GetNoDataValue()
    arr = band.ReadAsArray().tolist()
    ny = len(arr)
    xll, yll = gt[0] - origin_e, gt[3] - ny - origin_n
    if xll != int(xll) or yll != int(yll):
        raise IngestError("source %d: lower-left corner is not on a whole metre of the local frame" % source_id)
    rows = [[None if (nodata is not None and v == nodata) else v for v in r] for r in reversed(arr)]
    return Raster(source_id, int(xll), int(yll), rows)


def ingest(rasters, width, height):
    """Merges rasters (ascending id, first valid value wins). Returns (dem bytes, coverage bytes, report dict)."""
    ids = [r.source_id for r in rasters]
    if len(set(ids)) != len(ids) or any(not 1 <= i <= 255 for i in ids):
        raise IngestError("source ids must be unique and in 1..255")
    if width < 1 or height < 1:
        raise IngestError("extent must be at least one post")
    dem = [VOID] * (width * height)
    cov = bytearray(width * height)
    used = {}
    for r in sorted(rasters, key=lambda r: r.source_id):
        n = 0
        for j, row in enumerate(r.rows):
            y = r.y0 + j
            if not 0 <= y < height:
                continue
            for i, v in enumerate(row):
                x = r.x0 + i
                if not 0 <= x < width:
                    continue
                k = y * width + x
                if cov[k]:
                    continue
                c = _cm(v)
                if c != VOID:
                    dem[k], cov[k] = c, r.source_id
                    n += 1
        used[r.source_id] = n
    total = width * height
    voids = dem.count(VOID)
    report = {"width": width, "height": height, "void_posts": voids,
              "void_ppm": (voids * 1_000_000 + total - 1) // total, "posts_by_source": used}
    return struct.pack("<%di" % total, *dem), bytes(cov), report


def write_outputs(out_dir, dem, cov, report, gdal_version=None):
    """Writes the two rasters and `ingest.toml` (integers and hex strings only)."""
    os.makedirs(out_dir, exist_ok=True)
    for name, data in (("dem_cm.i32", dem), ("coverage.u8", cov)):
        with open(os.path.join(out_dir, name), "wb") as f:
            f.write(data)
        report[name.replace(".", "_") + "_sha256"] = hashlib.sha256(data).hexdigest()
    lines = ['schema = "survey-ingest/1"', "width = %d" % report["width"], "height = %d" % report["height"],
             "void_posts = %d" % report["void_posts"], "void_ppm = %d" % report["void_ppm"],
             'dem_cm_i32_sha256 = "%s"' % report["dem_cm_i32_sha256"],
             'coverage_u8_sha256 = "%s"' % report["coverage_u8_sha256"]]
    if gdal_version:
        lines.append('gdal_version = "%s"' % gdal_version)
    for sid, n in sorted(report["posts_by_source"].items()):
        lines += ["", "[[source]]", "id = %d" % sid, "posts = %d" % n]
    with open(os.path.join(out_dir, "ingest.toml"), "w", encoding="ascii", newline="\n") as f:
        f.write("\n".join(lines) + "\n")


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    for k in ("origin-e", "origin-n", "width", "height"):
        ap.add_argument("--" + k, type=int, required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--source", action="append", required=True, metavar="ID=PATH")
    a = ap.parse_args(argv)
    gdal_version = None
    try:
        rasters = []
        for s in a.source:
            sid, _, path = s.partition("=")
            if not sid.isdigit() or not path:
                raise IngestError("bad --source %r (expected ID=PATH)" % s)
            if path.lower().endswith((".tif", ".tiff")):
                rasters.append(read_geotiff(int(sid), path, a.origin_e, a.origin_n))
                from osgeo import gdal
                gdal_version = gdal.__version__
            else:
                rasters.append(read_asc(int(sid), path, a.origin_e, a.origin_n))
        dem, cov, report = ingest(rasters, a.width, a.height)
    except (IngestError, OSError, ValueError) as e:
        print("ingest_dem: refused: %s" % e, file=sys.stderr)
        return 2
    write_outputs(a.out, dem, cov, report, gdal_version)
    print("ingest_dem: %dx%d posts, %d ppm void" % (a.width, a.height, report["void_ppm"]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
