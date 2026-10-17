#!/usr/bin/env python3
"""issue 029k1 (K3): raise the executable TLS segment alignment to 64.

NDK 27's Bionic aborts STATIC aarch64 executables whose executable TLS
segment alignment is below 64 bytes (its own `__libc_tls_generation_copy`
static TLS is 8-aligned, so any static binary gets alignment 8):

    error: "<bin>": executable's TLS segment is underaligned:
    alignment is 8 (skew 0), needs to be at least 64 for ARM64 Bionic

The check reads the ELF metadata (PT_TLS p_align / .tdata .tbss section
alignment). Raising those fields to 64 is a pure alignment-metadata
change (the segment's offset/addr are already 64-aligned in practice, so
the layout is unchanged and the larger alignment only reserves slack).
qemu-user ignores the value entirely; real Android devices would honor
it. Use only for the K3 host-side experiment binaries, never for
shipped artifacts.
"""
import struct
import sys


def patch(path: str) -> None:
    d = bytearray(open(path, "rb").read())
    if d[:4] != b"\x7fELF" or d[4] != 2:
        raise SystemExit(f"{path}: not an ELF64")

    e_phoff = struct.unpack_from("<Q", d, 0x20)[0]
    e_phentsize = struct.unpack_from("<H", d, 0x36)[0]
    e_phnum = struct.unpack_from("<H", d, 0x38)[0]
    for i in range(e_phnum):
        off = e_phoff + i * e_phentsize
        if struct.unpack_from("<I", d, off)[0] == 7:  # PT_TLS
            old = struct.unpack_from("<Q", d, off + 48)[0]
            d[off + 48: off + 56] = struct.pack("<Q", 64)
            print(f"{path}: PT_TLS p_align {old} -> 64")

    e_shoff = struct.unpack_from("<Q", d, 0x28)[0]
    e_shentsize = struct.unpack_from("<H", d, 0x3A)[0]
    e_shnum = struct.unpack_from("<H", d, 0x3C)[0]
    e_shstrndx = struct.unpack_from("<H", d, 0x3E)[0]
    if e_shoff == 0 or e_shnum == 0:
        return
    strs_off = struct.unpack_from(
        "<Q", d, e_shoff + e_shstrndx * e_shentsize + 24
    )[0]
    for i in range(e_shnum):
        base = e_shoff + i * e_shentsize
        no = struct.unpack_from("<I", d, base)[0]
        end = d.index(b"\x00", strs_off + no)
        name = d[strs_off + no: end].decode()
        if name in (".tdata", ".tbss"):
            old = struct.unpack_from("<Q", d, base + 48)[0]
            d[base + 48: base + 56] = struct.pack("<Q", 64)
            print(f"{path}: section {name} align {old} -> 64")

    open(path, "wb").write(bytes(d))


if __name__ == "__main__":
    patch(sys.argv[1])
