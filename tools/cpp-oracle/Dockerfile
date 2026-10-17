FROM ubuntu:24.04

# The C++ reference build environment (task 030b — the "C++ column").
#
# Why a container: the C++ fetches SFML/luajit/zlib/imgui/libsodium/boostpfr via
# CPM, but SFML still needs SYSTEM X11/GL interfaces (they cannot be vendored),
# and this machine has no passwordless sudo and is missing most of them. Docker
# works here, so the dependency set lives in a reproducible image instead of
# being hand-installed on a host where it can silently rot.
#
# This is the SFML-on-Linux dependency set (SFML 2.x/3.x): the X11 family it
# genuinely needs, GL/EGL headers, and the media libs (freetype/openal/vorbis/flac
# — SFML can vendor these, but having them keeps the configure honest).
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        cmake \
        ninja-build \
        git \
        ca-certificates \
        pkg-config \
        libx11-dev \
        libxrandr-dev \
        libxcursor-dev \
        libxi-dev \
        libxext-dev \
        libxrender-dev \
        libxinerama-dev \
        libgl1-mesa-dev \
        libglvnd-dev \
        libegl1-mesa-dev \
        libfreetype6-dev \
        libopenal-dev \
        libvorbis-dev \
        libflac-dev \
        libudev-dev \
        zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*

# The repo is mounted at /src; the build dir is a separate host mount (/build) so
# the checkout stays clean and incremental builds survive between runs.
WORKDIR /src
