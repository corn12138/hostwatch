#!/usr/bin/env python3
"""Sample a process's RSS, physical footprint and CPU time via proc_pid_rusage (macOS).

Usage: scripts/proc-usage.py PID [SECONDS] [EVERY]
Prints one tab-separated row per sample: elapsed seconds, RSS MiB (2^20 bytes),
physical footprint MB (10^6 bytes; other tools may print 2^20-byte units) and
cumulative user+system CPU seconds. A final line summarizes deltas.
Read-only: it only queries the kernel about an existing process you own.
"""
import ctypes
import sys
import time

libc = ctypes.CDLL("/usr/lib/libSystem.B.dylib")


class RusageInfoV0(ctypes.Structure):
    _fields_ = [
        ("uuid", ctypes.c_uint8 * 16),
        ("user_time", ctypes.c_uint64),
        ("system_time", ctypes.c_uint64),
        ("pkg_idle_wkups", ctypes.c_uint64),
        ("interrupt_wkups", ctypes.c_uint64),
        ("pageins", ctypes.c_uint64),
        ("wired_size", ctypes.c_uint64),
        ("resident_size", ctypes.c_uint64),
        ("phys_footprint", ctypes.c_uint64),
        ("proc_start_abstime", ctypes.c_uint64),
        ("proc_exit_abstime", ctypes.c_uint64),
    ]


class Timebase(ctypes.Structure):
    _fields_ = [("numer", ctypes.c_uint32), ("denom", ctypes.c_uint32)]


def main():
    pid = int(sys.argv[1])
    duration = float(sys.argv[2]) if len(sys.argv) > 2 else 120
    every = float(sys.argv[3]) if len(sys.argv) > 3 else 10
    timebase = Timebase()
    libc.mach_timebase_info(ctypes.byref(timebase))
    # rusage times are Mach absolute units; convert to nanoseconds with the timebase.
    scale = timebase.numer / timebase.denom / 1e9
    print(f"# pid={pid} timebase={timebase.numer}/{timebase.denom} every={every}s")
    print("elapsed_s\trss_mib\tfootprint_mb\tcpu_s")
    start = time.monotonic()
    rows = []
    while True:
        info = RusageInfoV0()
        if libc.proc_pid_rusage(pid, 0, ctypes.byref(info)) != 0:
            print(f"# proc_pid_rusage failed (errno {ctypes.get_errno()}); process gone?")
            break
        elapsed = time.monotonic() - start
        cpu = (info.user_time + info.system_time) * scale
        row = (elapsed, info.resident_size / 2**20, info.phys_footprint / 1e6, cpu)
        rows.append(row)
        print("%.3f\t%.3f\t%.3f\t%.4f" % row, flush=True)
        if elapsed >= duration:
            break
        time.sleep(every)
    if len(rows) > 1:
        span = rows[-1][0] - rows[0][0]
        cpu = rows[-1][3] - rows[0][3]
        rss = [r[1] for r in rows]
        fp = [r[2] for r in rows]
        print(
            "# samples=%d span=%.1fs rss_mib=%.3f..%.3f footprint_mb=%.3f..%.3f "
            "cpu_delta=%.4fs avg_core=%.4f%%"
            % (len(rows), span, min(rss), max(rss), min(fp), max(fp), cpu, cpu / span * 100)
        )


if __name__ == "__main__":
    main()
