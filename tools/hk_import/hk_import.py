#!/usr/bin/env python3
"""Convert Hong Kong Observatory nongli daily CSV to client month-start CSV.

Hong Kong input (one civil day per row):

    Gregorian Date,Chinese year (Gan-Zhi),Chinese year (Zodiac),Lunar month,Lunar Date
    17-Feb-26,丙午年,馬,正月,初一

Client output (DESIGN.md §3.1) — only lunar day 1 of each month:

    gregorian_date,lunar_year_fn,stem_id,branch_id,lunar_month,leap,lunar_day
    2026-02-17,2026,2,6,01,0,1

Tiết khí is not present in the HK file and is not written here.

Examples:

    python tools/hk_import/hk_import.py \\
        --in tools/hk_import/input/nongli_calendar_2026.csv \\
        --out app/assets/calendar/2026.csv

    python tools/hk_import/hk_import.py --all
"""

from __future__ import annotations

import argparse
import csv
import re
import sys
import unittest
from dataclasses import dataclass
from datetime import date
from pathlib import Path

STEMS = "甲乙丙丁戊己庚辛壬癸"
BRANCHES = "子丑寅卯辰巳午未申酉戌亥"

MONTH_ABBREV = {
    "Jan": 1,
    "Feb": 2,
    "Mar": 3,
    "Apr": 4,
    "May": 5,
    "Jun": 6,
    "Jul": 7,
    "Aug": 8,
    "Sep": 9,
    "Oct": 10,
    "Nov": 11,
    "Dec": 12,
}

LUNAR_MONTH_NUM = {
    "正": 1,
    "一": 1,
    "二": 2,
    "三": 3,
    "四": 4,
    "五": 5,
    "六": 6,
    "七": 7,
    "八": 8,
    "九": 9,
    "十": 10,
    "十一": 11,
    "十二": 12,
}

LUNAR_DAY_NUM = {
    "初一": 1,
    "初二": 2,
    "初三": 3,
    "初四": 4,
    "初五": 5,
    "初六": 6,
    "初七": 7,
    "初八": 8,
    "初九": 9,
    "初十": 10,
    "十一": 11,
    "十二": 12,
    "十三": 13,
    "十四": 14,
    "十五": 15,
    "十六": 16,
    "十七": 17,
    "十八": 18,
    "十九": 19,
    "二十": 20,
    "廿一": 21,
    "廿二": 22,
    "廿三": 23,
    "廿四": 24,
    "廿五": 25,
    "廿六": 26,
    "廿七": 27,
    "廿八": 28,
    "廿九": 29,
    "三十": 30,
}

GREGORIAN_RE = re.compile(r"^(\d{1,2})-([A-Za-z]{3})-(\d{2,4})$")
LUNAR_MONTH_RE = re.compile(r"^(閏|闰)?(正|一|二|三|四|五|六|七|八|九|十|十一|十二)月$")
YEAR_IN_NAME_RE = re.compile(r"(?:^|[^\d])(\d{4})(?:[^\d]|$)")

INPUT_HEADER = [
    "Gregorian Date",
    "Chinese year (Gan-Zhi)",
    "Chinese year (Zodiac)",
    "Lunar month",
    "Lunar Date",
]
OUTPUT_HEADER = [
    "gregorian_date",
    "lunar_year_fn",
    "stem_id",
    "branch_id",
    "lunar_month",
    "leap",
    "lunar_day",
]

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
DEFAULT_IN_DIR = SCRIPT_DIR / "input"
DEFAULT_OUT_DIR = REPO_ROOT / "app" / "assets" / "calendar"


class ConvertError(ValueError):
    pass


@dataclass(frozen=True)
class MonthStart:
    gregorian: date
    lunar_year_fn: int
    stem_id: int
    branch_id: int
    lunar_month: int
    leap: int
    lunar_day: int = 1

    def csv_row(self) -> list[str]:
        return [
            self.gregorian.isoformat(),
            str(self.lunar_year_fn),
            str(self.stem_id),
            str(self.branch_id),
            f"{self.lunar_month:02d}",
            str(self.leap),
            str(self.lunar_day),
        ]


def year_hint_from_path(path: Path) -> int | None:
    match = re.search(r"nongli_calendar_(\d{4})", path.name)
    if match:
        return int(match.group(1))
    match = YEAR_IN_NAME_RE.search(path.name)
    if match:
        return int(match.group(1))
    return None


def expand_year(yy: int, hint: int | None) -> int:
    if yy >= 100:
        return yy
    if hint is not None and yy == hint % 100:
        return hint
    candidates = [century + yy for century in (1800, 1900, 2000, 2100)]
    if hint is None:
        return 2000 + yy if yy < 100 else yy
    return min(candidates, key=lambda year: (abs(year - hint), year))


def parse_gregorian(text: str, year_hint: int | None = None) -> date:
    match = GREGORIAN_RE.match(text.strip())
    if not match:
        raise ConvertError(f"unrecognized Gregorian date: {text!r}")
    day = int(match.group(1))
    month_name = match.group(2).title()
    month = MONTH_ABBREV.get(month_name)
    if month is None:
        raise ConvertError(f"unrecognized month abbreviation: {match.group(2)!r}")
    year = expand_year(int(match.group(3)), year_hint)
    try:
        return date(year, month, day)
    except ValueError as exc:
        raise ConvertError(f"invalid Gregorian date: {text!r}") from exc


def parse_ganzhi(text: str) -> tuple[int, int]:
    raw = text.strip().rstrip("年").strip()
    if len(raw) < 2:
        raise ConvertError(f"unrecognized Gan-Zhi: {text!r}")
    try:
        stem_id = STEMS.index(raw[0])
        branch_id = BRANCHES.index(raw[1])
    except ValueError as exc:
        raise ConvertError(f"unrecognized Gan-Zhi: {text!r}") from exc
    return stem_id, branch_id


def parse_lunar_month(text: str) -> tuple[int, int]:
    match = LUNAR_MONTH_RE.match(text.strip())
    if not match:
        raise ConvertError(f"unrecognized lunar month: {text!r}")
    leap = 1 if match.group(1) else 0
    return LUNAR_MONTH_NUM[match.group(2)], leap


def parse_lunar_day(text: str) -> int:
    day = LUNAR_DAY_NUM.get(text.strip())
    if day is None:
        raise ConvertError(f"unrecognized lunar day: {text!r}")
    return day


def lunar_year_fn(gregorian_year: int, stem_id: int, branch_id: int) -> int:
    """Proleptic year = Gregorian year of Tết for this lunar year (DESIGN §3.1)."""
    for year in (gregorian_year, gregorian_year - 1):
        if (year - 4) % 10 == stem_id and (year - 4) % 12 == branch_id:
            return year
    raise ConvertError(
        f"Gan-Zhi stem={stem_id} branch={branch_id} does not match "
        f"lunar year {gregorian_year} or {gregorian_year - 1}"
    )


def _norm_header(name: str) -> str:
    return " ".join(name.replace("\ufeff", "").split())


def convert_rows(rows: list[dict[str, str]], year_hint: int | None = None) -> list[MonthStart]:
    starts: list[MonthStart] = []
    seen: set[tuple[date, int, int]] = set()
    for index, row in enumerate(rows, start=2):
        try:
            lunar_day = parse_lunar_day(row["Lunar Date"])
            if lunar_day != 1:
                continue
            gregorian = parse_gregorian(row["Gregorian Date"], year_hint)
            stem_id, branch_id = parse_ganzhi(row["Chinese year (Gan-Zhi)"])
            lunar_month, leap = parse_lunar_month(row["Lunar month"])
            year_fn = lunar_year_fn(gregorian.year, stem_id, branch_id)
        except ConvertError as exc:
            raise ConvertError(f"line {index}: {exc}") from exc
        key = (gregorian, lunar_month, leap)
        if key in seen:
            raise ConvertError(f"line {index}: duplicate month-start {gregorian.isoformat()}")
        seen.add(key)
        starts.append(
            MonthStart(
                gregorian=gregorian,
                lunar_year_fn=year_fn,
                stem_id=stem_id,
                branch_id=branch_id,
                lunar_month=lunar_month,
                leap=leap,
            )
        )
    starts.sort(key=lambda item: item.gregorian)
    if not starts:
        raise ConvertError("no lunar day 初一 rows found")
    if len(starts) < 11 or len(starts) > 13:
        raise ConvertError(
            f"expected 11–13 month-starts in a Gregorian year file, got {len(starts)}"
        )
    return starts


def read_hk_csv(path: Path) -> list[dict[str, str]]:
    with path.open(encoding="utf-8-sig", newline="") as handle:
        reader = csv.DictReader(handle)
        if reader.fieldnames is None:
            raise ConvertError(f"{path}: missing header")
        got = [_norm_header(name) for name in reader.fieldnames]
        if got != INPUT_HEADER:
            raise ConvertError(
                f"{path}: unexpected header {got}; expected {INPUT_HEADER}"
            )
        rows: list[dict[str, str]] = []
        for row in reader:
            rows.append({_norm_header(key): (value or "").strip() for key, value in row.items()})
    return rows


def write_month_start_csv(path: Path, starts: list[MonthStart]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as handle:
        writer = csv.writer(handle, lineterminator="\n")
        writer.writerow(OUTPUT_HEADER)
        for start in starts:
            writer.writerow(start.csv_row())


def convert_file(in_path: Path, out_path: Path | None = None) -> tuple[Path, list[MonthStart]]:
    year_hint = year_hint_from_path(in_path)
    starts = convert_rows(read_hk_csv(in_path), year_hint=year_hint)
    if out_path is None:
        year = year_hint if year_hint is not None else starts[0].gregorian.year
        out_path = DEFAULT_OUT_DIR / f"{year}.csv"
    write_month_start_csv(out_path, starts)
    return out_path, starts


def iter_input_files(in_dir: Path) -> list[Path]:
    files = sorted(in_dir.glob("nongli_calendar_*.csv"))
    if not files:
        raise ConvertError(f"no nongli_calendar_YYYY.csv files in {in_dir}")
    return files


class ImportTests(unittest.TestCase):
    def test_parse_gregorian_two_digit(self) -> None:
        self.assertEqual(parse_gregorian("17-Feb-26", 2026), date(2026, 2, 17))
        self.assertEqual(parse_gregorian("1-Jan-23", 2023), date(2023, 1, 1))
        self.assertEqual(parse_gregorian("31-Dec-24", 2024), date(2024, 12, 31))

    def test_parse_gregorian_four_digit(self) -> None:
        self.assertEqual(parse_gregorian("17-Feb-2026"), date(2026, 2, 17))

    def test_parse_ganzhi(self) -> None:
        self.assertEqual(parse_ganzhi("乙巳年"), (1, 5))
        self.assertEqual(parse_ganzhi("丙午年"), (2, 6))
        self.assertEqual(parse_ganzhi("癸卯"), (9, 3))
        self.assertEqual(parse_ganzhi("甲辰年"), (0, 4))
        self.assertEqual(parse_ganzhi("戊申年"), (4, 8))

    def test_parse_lunar_month(self) -> None:
        self.assertEqual(parse_lunar_month("正月"), (1, 0))
        self.assertEqual(parse_lunar_month("十二月"), (12, 0))
        self.assertEqual(parse_lunar_month("閏二月"), (2, 1))
        self.assertEqual(parse_lunar_month("闰六月"), (6, 1))

    def test_lunar_year_fn_changes_at_tet(self) -> None:
        self.assertEqual(lunar_year_fn(2026, 1, 5), 2025)  # 乙巳 before Tết 2026
        self.assertEqual(lunar_year_fn(2026, 2, 6), 2026)  # 丙午 from Tết 2026
        self.assertEqual(lunar_year_fn(2027, 2, 6), 2026)  # 丙午 in Jan 2027

    def test_month_start_csv_row(self) -> None:
        row = MonthStart(date(2026, 2, 17), 2026, 2, 6, 1, 0)
        self.assertEqual(row.csv_row(), ["2026-02-17", "2026", "2", "6", "01", "0", "1"])

    def test_convert_keeps_only_day_one_and_leap(self) -> None:
        lunar_names = ["正", "二", "三", "四", "五", "六", "七", "八", "九", "十", "十一"]
        rows = [
            {
                "Gregorian Date": "19-Jan-26",
                "Chinese year (Gan-Zhi)": "乙巳年",
                "Chinese year (Zodiac)": "蛇",
                "Lunar month": "十二月",
                "Lunar Date": "初一",
            },
            {
                "Gregorian Date": "20-Jan-26",
                "Chinese year (Gan-Zhi)": "乙巳年",
                "Chinese year (Zodiac)": "蛇",
                "Lunar month": "十二月",
                "Lunar Date": "初二",
            },
        ]
        for index, month_name in enumerate(lunar_names):
            rows.append(
                {
                    "Gregorian Date": f"1-{list(MONTH_ABBREV)[index]}-26",
                    "Chinese year (Gan-Zhi)": "丙午年",
                    "Chinese year (Zodiac)": "馬",
                    "Lunar month": f"{month_name}月",
                    "Lunar Date": "初一",
                }
            )
        rows.append(
            {
                "Gregorian Date": "23-Jun-26",
                "Chinese year (Gan-Zhi)": "丙午年",
                "Chinese year (Zodiac)": "馬",
                "Lunar month": "閏五月",
                "Lunar Date": "初一",
            }
        )
        starts = convert_rows(rows, year_hint=2026)
        self.assertEqual(len(starts), 13)
        self.assertEqual(starts[0].gregorian, date(2026, 1, 1))
        self.assertEqual(starts[0].lunar_month, 1)
        prev_year_start = next(item for item in starts if item.gregorian == date(2026, 1, 19))
        self.assertEqual(prev_year_start.lunar_year_fn, 2025)
        self.assertEqual(prev_year_start.stem_id, 1)
        self.assertEqual(prev_year_start.branch_id, 5)
        self.assertEqual(prev_year_start.lunar_month, 12)
        self.assertEqual(prev_year_start.leap, 0)
        leap = next(item for item in starts if item.leap == 1)
        self.assertEqual(leap.gregorian, date(2026, 6, 23))
        self.assertEqual(leap.lunar_month, 5)
        self.assertEqual(leap.lunar_year_fn, 2026)

    def test_real_2026_file(self) -> None:
        path = DEFAULT_IN_DIR / "nongli_calendar_2026.csv"
        if not path.is_file():
            self.skipTest(f"missing {path}")
        starts = convert_rows(read_hk_csv(path), year_hint=2026)
        by_date = {item.gregorian: item for item in starts}
        self.assertEqual(
            by_date[date(2026, 1, 19)],
            MonthStart(date(2026, 1, 19), 2025, 1, 5, 12, 0),
        )
        self.assertEqual(
            by_date[date(2026, 2, 17)],
            MonthStart(date(2026, 2, 17), 2026, 2, 6, 1, 0),
        )
        self.assertEqual(
            by_date[date(2026, 9, 11)],
            MonthStart(date(2026, 9, 11), 2026, 2, 6, 8, 0),
        )
        self.assertEqual(len(starts), 12)

    def test_real_leap_and_year_boundary_files(self) -> None:
        cases = {
            2023: (date(2023, 3, 22), MonthStart(date(2023, 3, 22), 2023, 9, 3, 2, 1)),
            2024: (date(2024, 1, 11), MonthStart(date(2024, 1, 11), 2023, 9, 3, 12, 0)),
            2025: (date(2025, 7, 25), MonthStart(date(2025, 7, 25), 2025, 1, 5, 6, 1)),
            2027: (date(2027, 1, 8), MonthStart(date(2027, 1, 8), 2026, 2, 6, 12, 0)),
            2028: (date(2028, 6, 23), MonthStart(date(2028, 6, 23), 2028, 4, 8, 5, 1)),
        }
        for year, (gregorian, expected) in cases.items():
            path = DEFAULT_IN_DIR / f"nongli_calendar_{year}.csv"
            if not path.is_file():
                self.skipTest(f"missing {path}")
            starts = convert_rows(read_hk_csv(path), year_hint=year)
            by_date = {item.gregorian: item for item in starts}
            self.assertEqual(by_date[gregorian], expected, msg=str(year))


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Convert Hong Kong nongli daily CSV to client lunar month-start CSV."
    )
    parser.add_argument("--in", dest="in_path", type=Path, help="HK nongli_calendar_YYYY.csv")
    parser.add_argument("--out", dest="out_path", type=Path, help="client YYYY.csv")
    parser.add_argument(
        "--all",
        action="store_true",
        help=f"convert every nongli_calendar_YYYY.csv in --in-dir (default {DEFAULT_IN_DIR})",
    )
    parser.add_argument("--in-dir", type=Path, default=DEFAULT_IN_DIR)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--self-test", action="store_true", help="run built-in unit tests")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(ImportTests)
        result = unittest.TextTestRunner(verbosity=2).run(suite)
        return 0 if result.wasSuccessful() else 1
    try:
        if args.all:
            written = 0
            for in_path in iter_input_files(args.in_dir):
                year = year_hint_from_path(in_path)
                if year is None:
                    raise ConvertError(f"cannot infer year from {in_path.name}")
                out_path = args.out_dir / f"{year}.csv"
                _, starts = convert_file(in_path, out_path)
                print(f"{in_path.name} -> {out_path} ({len(starts)} month-starts)")
                written += 1
            if written == 0:
                raise ConvertError("no files converted")
            return 0
        if args.in_path is None:
            parser.print_help()
            return 2
        out_path, starts = convert_file(args.in_path, args.out_path)
        print(f"{args.in_path} -> {out_path} ({len(starts)} month-starts)")
        return 0
    except ConvertError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
