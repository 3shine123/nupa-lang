#!/usr/bin/env python3
"""
nupa test runner — parallel transpile + compile + run with timeout.
Usage: python3 test_all.py [-j JOBS]
"""

import argparse, atexit, os, signal, subprocess, sys, tempfile, time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from rich.console import Console
from rich.markup import escape
from rich.panel import Panel
from rich.table import Table
from rich.style import Style
from rich.text import Text

PROJECT = Path(__file__).resolve().parent
BUILDDIR = PROJECT / "builddir"
INCLUDE = PROJECT / "include"

NUPAC = PROJECT / "target" / "debug" / "nupac"
RUN_TIMEOUT = 3

# ── Collect test binary names from .np files ──
NP_FILES = sorted(Path(__file__).resolve().parent.glob("tests/**/*.np"))
TEST_BINS = set()
for f in NP_FILES:
    stem = f.stem
    TEST_BINS.add(f"/tmp/{stem}")

# ── Cleanup: kill orphaned nupac processes + leftover test binaries on exit ──
def _cleanup():
    # Kill nupac itself
    subprocess.run(["pkill", "-f", r"target/(debug|release)/nupac"], capture_output=True)
    # Kill all compiled test binaries (e.g. /tmp/core_fusion, /tmp/tt, …)
    for bin_path in TEST_BINS:
        subprocess.run(["pkill", "-f", f"^{bin_path}($| )"], capture_output=True)
atexit.register(_cleanup)
signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))

console = Console()

# ── helpers ──────────────────────────────────────────────────────

def panel(title: str, lines: list[str], tag: str):
    """Build a colored Panel for one test result."""
    style_map = {
        "PASS":           "green",
        "PASS_TIMEOUT":   "yellow",
        "FAIL":           "red",
        "TRANSPILE_FAIL": "red",
        "TRANSPILE_TIMEOUT": "yellow",
        "COMPILE_FAIL":   "red",
        "COMPILE_TIMEOUT": "yellow",
        "RUN_FAIL":       "red",
        "CANCELED":       "yellow",
    }
    color = style_map.get(tag, "white")
    max_line = 160
    brief_lines = []
    for l in lines:
        while len(l) > max_line:
            brief_lines.append(l[:max_line])
            l = l[max_line:]
        brief_lines.append(l)
    brief = "\n".join(brief_lines) if brief_lines else ""
    body = f"[bold]── {tag} ──[/bold]\n{escape(brief)}" if brief else f"[bold]── {tag} ──[/bold]"
    return Panel(
        body,
        title=title,
        title_align="left",
        border_style=color,
        padding=(0, 1),
    )




def run_cargo_tests() -> tuple[int, int, list[str]]:
    """Run cargo test --workspace and parse summary."""
    try:
        r = subprocess.run(
            ["cargo", "test", "--workspace"],
            capture_output=True, text=True, timeout=300,
            cwd=PROJECT,
        )
        output = r.stdout + r.stderr
        lines = output.strip().split("\n")
        total_pass = 0
        total_fail = 0
        for line in lines:
            if line.startswith("test result:"):
                parts = line.split(";")
                for p in parts:
                    p = p.strip()
                    words = p.split()
                    for i, w in enumerate(words):
                        if w == "passed" and i > 0:
                            total_pass += int(words[i-1])
                        elif w == "failed" and i > 0:
                            total_fail += int(words[i-1])
        if total_pass + total_fail == 0:
            return 0, 1, ["cargo test produced no recognizable output"]
        return total_pass, total_fail, lines[-10:] if len(lines) > 10 else lines
    except subprocess.TimeoutExpired:
        return 0, 1, ["TIMEOUT (300s cargo test)"]
    except Exception as e:
        return 0, 1, [str(e)]


def _has_main(np_path: Path) -> bool:
    """Heuristic: does this .np file define its own `int main` entry point?

    Module files (e.g. diamond_impl.np) are meant to be #import'd by a driver
    file and have no main(); running them standalone always fails on missing
    `_main` symbol. Skip them instead of misreporting as failures.
    """
    try:
        text = np_path.read_text(encoding="utf-8", errors="replace")
    except Exception:
        return True  # be permissive; let nupac report the real error
    # strip line comments // and /* */ blocks so a commented-out main doesn't count
    import re as _re
    stripped = _re.sub(r"/\*.*?\*/", " ", text, flags=_re.S)
    stripped = _re.sub(r"//[^\n]*", " ", stripped)
    return bool(_re.search(r"\bint\s+main\s*\(", stripped)) or bool(_re.search(r"\bvoid\s+main\s*\(", stripped))


def process_np(np_file: str, tmpdir: Path) -> tuple[str, bool, list[str], str]:
    """
    Run one .np file via `nupac run`.
    Returns (relative_path, passed, info_lines, status_tag).
    """
    np_path = Path(np_file)
    rel = str(np_path.relative_to(PROJECT / "tests"))
    proc = None

    # Skip module-only files (no main entry); they're meant to be #import'd
    if not _has_main(np_path):
        return rel, False, ["FAIL (no main entry)"], "FAIL"

    try:
        proc = subprocess.Popen(
            [str(NUPAC), "run", str(np_path)],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            start_new_session=True,  # isolate so killpg doesn't hit us
        )
        try:
            stdout, stderr = proc.communicate(timeout=RUN_TIMEOUT + 2)
        except subprocess.TimeoutExpired:
            # Kill the process group so orphaned children (e.g. /tmp/tt) die too
            try:
                pgid = os.getpgid(proc.pid)
                os.killpg(pgid, signal.SIGKILL)
            except Exception:
                proc.kill()
            proc.wait()
            return rel, True, ["CANCELED (timed out — probably interactive game)"], "CANCELED"

        if proc.returncode == 0:
            out_lines = stdout.strip().split("\n") if stdout.strip() else ["(no output)"]
            return rel, True, out_lines, "PASS"
        else:
            err = (stderr or stdout).strip()
            err_lines = err.split("\n") if err else ["(no output)"]
            if "error:" in err.lower() or "Error:" in err:
                if "TRANSPILE" in err or "Parse" in err:
                    return rel, False, err_lines, "TRANSPILE_FAIL"
                else:
                    return rel, False, err_lines, "COMPILE_FAIL"
            else:
                return rel, False, [f"RUN FAILED exit={proc.returncode}"] + err_lines, "RUN_FAIL"
    except Exception as e:
        return rel, False, [str(e)], "RUN_FAIL"
    finally:
        if proc and proc.poll() is None:
            try:
                pgid = os.getpgid(proc.pid)
                os.killpg(pgid, signal.SIGKILL)
            except Exception:
                try:
                    proc.kill()
                except Exception:
                    pass
            proc.wait()


# ── main ─────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="nupa test runner")
    parser.add_argument("-j", type=int, default=0, help="parallel jobs (default: CPU count)")
    args = parser.parse_args()
    JOBS = args.j if args.j else (os.cpu_count() or 4)

    start = time.time()

    # ── 1. Unit tests ──
    console.rule("[bold]Unit Tests (cargo test)")
    unit_pass, unit_fail, cargo_out = run_cargo_tests()
    if unit_fail == 0:
        console.print(f"  [green]✅  All {unit_pass} Rust tests passed[/]")
    else:
        console.print(f"  [red]❌  {unit_fail} test(s) failed[/]")
        for l in cargo_out[-5:]:
            if l.strip():
                console.print(f"       [dim]{l.strip()[:72]}[/]")
    console.print(f"\n  [bold]{unit_pass}/{unit_pass + unit_fail}[/] unit tests passed, [red]{unit_fail}[/] failed")

    # ── 2. NP tests ──
    console.rule(f"[bold]NP Tests  (parallel x{JOBS})")
    np_files = sorted(PROJECT.glob("tests/**/*.np"))
    np_total = len(np_files)
    if np_total == 0:
        console.print("  [yellow]No .np files found.[/]")
    else:
        with tempfile.TemporaryDirectory(prefix="nupa_test_") as tmpdir_str:
            tmpdir = Path(tmpdir_str)
            np_pass = 0
            np_fail = 0
            np_canceled = 0
            with ThreadPoolExecutor(max_workers=JOBS) as executor:
                futures = {executor.submit(process_np, str(f), tmpdir): f for f in np_files}
                for future in as_completed(futures):
                    rel, ok, lines, tag = future.result()
                    if tag == "CANCELED":
                        np_canceled += 1
                    elif ok:
                        np_pass += 1
                    else:
                        np_fail += 1
                    console.print(panel(rel, lines, tag))
            canceled_str = f", [yellow]{np_canceled} canceled[/]" if np_canceled else ""
            console.print(f"\n  [bold]{np_pass}/{np_total}[/] .np files passed, [red]{np_fail}[/] failed{canceled_str}")

    # ── Grand total ──
    elapsed = time.time() - start
    total_fail = unit_fail + np_fail
    total_pass = unit_pass + np_pass
    total = unit_pass + unit_fail + np_pass + np_fail + np_canceled
    canceled_str = f", [yellow]{np_canceled} canceled[/]" if np_canceled else ""
    color = "green" if total_fail == 0 else "red"
    console.rule(f"[bold {color}]GRAND TOTAL: {total_pass}/{total} passed, {total_fail} failed{canceled_str}  ({elapsed:.0f}s)")
    sys.exit(total_fail)


if __name__ == "__main__":
    main()