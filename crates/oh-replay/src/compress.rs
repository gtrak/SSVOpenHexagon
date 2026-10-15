//! C++ `compress_replay_file` / `decompress_replay_file`
//! (`src/SSVOpenHexagon/Core/Replay.cpp`) — zlib `compress2` / `uncompress`
//! at `Z_BEST_COMPRESSION` (== 9) into/out of the 2 MiB static buffer.
//!
//! Implemented with `flate2` on its **`zlib` feature** (system libz via
//! libz-sys, not the default miniz_oxide rust backend): flate2's C backend
//! drives the same system `deflate`/`inflate` the C++ build uses
//! (`deflateInit2(level, Z_DEFLATED, 15, 8, Z_DEFAULT_STRATEGY)` +
//! `deflate(Z_FINISH)` — the `compress2` setup), which is what makes the
//! compressed output byte-identical to the C++ build (pinned by `CZHEX` in
//! `.agents/plans/001-rust-port/mapping/oracle-vectors.md`; DECISIONS.md
//! D-006).

use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};

use crate::{CompressedReplayFile, ReplayFile, STATIC_BUF, STATIC_COMPRESSION_BUF};

/// C++ `std::optional<compressed_replay_file> compress_replay_file(const
/// replay_file& rf)` — serialize the replay into the 2 MiB static buffer,
/// then `compress2(..., Z_BEST_COMPRESSION)` into the 2 MiB compression
/// buffer; `None` on any failure.
#[must_use]
pub fn compress_replay_file(rf: &ReplayFile) -> Option<CompressedReplayFile> {
    STATIC_BUF.with(|src_cell| {
        STATIC_COMPRESSION_BUF.with(|dst_cell| {
            let src = &mut src_cell.borrow_mut()[..];
            let dst = &mut dst_cell.borrow_mut()[..];
            let sr = rf.serialize(src);
            if !sr.is_success() {
                return None;
            }
            let written = sr.written_bytes();

            // C++: `compress2(dst, &in_out_dest_len = buf_size, src,
            // written, Z_BEST_COMPRESSION)`; `in_out_dest_len` (2 MiB in) is
            // the output cap — `Status::BufError` below is the C++
            // `Z_BUF_ERROR` case.
            let mut c = Compress::new(Compression::best(), true /* zlib header */);
            match c.compress(&src[..written], dst, FlushCompress::Finish) {
                Ok(Status::StreamEnd) => {
                    let n = c.total_out() as usize;
                    Some(CompressedReplayFile {
                        data: dst[..n].to_vec(),
                    })
                }
                // C++: `rc != Z_OK` → error message + `std::nullopt`.
                other => {
                    // C++: `std::cerr << "Failed compression of replay file,
                    // error code: '" << rc << "'\n"`.
                    eprintln!("Failed compression of replay file, error code: '{other:?}'");
                    None
                }
            }
        })
    })
}

/// C++ `std::optional<replay_file> decompress_replay_file(const
/// compressed_replay_file& crf)` — `uncompress` the compressed bytes into
/// the 2 MiB static buffer (the cap is the destination size), then
/// deserialize; `None` on any failure.
#[must_use]
pub fn decompress_replay_file(crf: &CompressedReplayFile) -> Option<ReplayFile> {
    STATIC_BUF.with(|buf_cell| {
        // C++: `uncompress(buf, &in_out_dest_len = buf_size, crf._data.data(),
        // crf._data.size())` — C's `uncompress` = `inflateInit` +
        // `inflate(Z_FINISH)`; flate2's C backend does the same
        // (`inflateInit2(15)` + `inflate(FINISH)`).
        let buf = &mut buf_cell.borrow_mut()[..];
        let mut d = Decompress::new(true /* zlib header */);
        match d.decompress(&crf.data, buf, FlushDecompress::Finish) {
            Ok(Status::StreamEnd) => {
                let n = d.total_out() as usize;
                // C++: `replay_file result; result.deserialize(buf,
                // in_out_dest_len);` (fail → nullopt).
                let mut result = ReplayFile::default();
                let dr = result.deserialize(&buf[..n]);
                if dr.is_success() {
                    Some(result)
                } else {
                    None
                }
            }
            // C++: `rc != Z_OK` → error message + `std::nullopt`. (The C++
            // source prints the *compression* message here too.)
            other => {
                eprintln!("Failed compression of replay file, error code: '{other:?}'");
                None
            }
        }
    })
}
