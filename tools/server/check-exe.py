"""check-exe.py: the version check of a Windows server program built on Linux.

    python3 tools/server/check-exe.py <program.exe> <version>

The exe can't run on the CI server, so this reads it instead: a PE32+ image for x86-64 with
the console subsystem (a server program prints to its console), carrying exactly <version>
(the string its --version prints, baked in at build time, as UTF-8 or in a UTF-16LE Windows
version resource). Exit 0 when all three hold. The
run of `<program>.exe --version` itself happens on a Windows PC (tools/server/README.md).
"""

import re
import struct
import sys

MACHINE_X86_64 = 0x8664
PE32_PLUS = 0x20B
SUBSYSTEM_CONSOLE = 3


def check(data: bytes, version: str) -> list[str]:
    """The problems found; empty when the exe passes."""
    if data[:2] != b"MZ" or len(data) < 0x40:
        return ["not an MZ executable"]
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe : pe + 4] != b"PE\0\0":
        return ["no PE header"]
    problems = []
    machine = struct.unpack_from("<H", data, pe + 4)[0]
    if machine != MACHINE_X86_64:
        problems.append(f"machine {machine:#x}, not x86-64")
    opt = pe + 24
    magic = struct.unpack_from("<H", data, opt)[0]
    if magic != PE32_PLUS:
        problems.append(f"optional header {magic:#x}, not PE32+")
    else:
        subsystem = struct.unpack_from("<H", data, opt + 68)[0]
        if subsystem != SUBSYSTEM_CONSOLE:
            problems.append(f"subsystem {subsystem}, not the console")
    # The version, not followed by another digit or part (0.1.1.1 must not match 0.1.1.12).
    # Optimized String construction may store UTF-8 bytes in separate machine
    # instructions. A Windows version resource keeps the UTF-16LE text intact.
    utf8 = re.escape(version.encode()) + rb"(?![0-9.])"
    utf16 = re.escape(version.encode("utf-16le")) + rb"(?![0-9.]\x00)"
    if not (re.search(utf8, data) or re.search(utf16, data)):
        problems.append(f"version {version} not found in the exe")
    return problems


def selftest() -> None:
    def exe(machine: int, magic: int, subsystem: int, tail: bytes) -> bytes:
        head = bytearray(0x40)
        head[:2] = b"MZ"
        struct.pack_into("<I", head, 0x3C, 0x40)
        pe = b"PE\0\0" + struct.pack("<H", machine) + bytes(18)
        opt = bytearray(96)
        struct.pack_into("<H", opt, 0, magic)
        struct.pack_into("<H", opt, 68, subsystem)
        return bytes(head) + pe + bytes(opt) + tail

    good = exe(MACHINE_X86_64, PE32_PLUS, SUBSYSTEM_CONSOLE, b"xx0.1.1.7yy")
    assert check(good, "0.1.1.7") == [], check(good, "0.1.1.7")
    assert check(good, "0.1.1.70"), "a longer version must not match"
    assert check(exe(MACHINE_X86_64, PE32_PLUS, SUBSYSTEM_CONSOLE, b"0.1.1.12"), "0.1.1.1")
    for version in ("0.1.1.1", "0.1.1.1730"):
        resource = exe(
            MACHINE_X86_64, PE32_PLUS, SUBSYSTEM_CONSOLE,
            ("FileVersion\0" + version + "\0").encode("utf-16le"),
        )
        assert check(resource, version) == [], check(resource, version)
        for bad in (version[:-1], version + "0", "9.9.9.9"):
            assert check(resource, bad), "a different resource version must not match"
    longer = exe(
        MACHINE_X86_64, PE32_PLUS, SUBSYSTEM_CONSOLE, "0.1.1.1.2\0".encode("utf-16le")
    )
    assert check(longer, "0.1.1.1"), "a shorter resource version must not match"
    assert check(exe(0x14C, PE32_PLUS, SUBSYSTEM_CONSOLE, b"0.1.1.7"), "0.1.1.7")
    assert check(exe(MACHINE_X86_64, 0x10B, SUBSYSTEM_CONSOLE, b"0.1.1.7"), "0.1.1.7")
    assert check(exe(MACHINE_X86_64, PE32_PLUS, 2, b"0.1.1.7"), "0.1.1.7")
    assert check(b"\x7fELF" + bytes(100), "0.1.1.7") == ["not an MZ executable"]
    print("check-exe selftest: ok")


def main() -> int:
    if sys.argv[1:] == ["--selftest"]:
        selftest()
        return 0
    if len(sys.argv) != 3:
        print(__doc__.strip().splitlines()[2].strip(), file=sys.stderr)
        return 2
    path, version = sys.argv[1], sys.argv[2]
    with open(path, "rb") as f:
        data = f.read()
    problems = check(data, version)
    for p in problems:
        print(f"check-exe: {path}: {p}", file=sys.stderr)
    if not problems:
        print(f"check-exe: {path}: x86-64 console exe, version {version}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
