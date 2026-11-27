use flate2::write::GzEncoder;
use flate2::Compression;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn fnv1a_64(chunks: &[&[u8]]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for chunk in chunks {
        for &b in *chunk {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn gzip_bytes(raw: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(raw).expect("gzip write failed");
    encoder.finish().expect("gzip finish failed")
}

fn main() {
    println!("cargo:rerun-if-env-changed=MUTIL_ZALO_REQUIRE_PAYLOAD");
    println!("cargo:rerun-if-env-changed=MUTIL_ZALO_STAGING_DIR");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf();

    let staging_dir = env::var("MUTIL_ZALO_STAGING_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_root.join("target").join("staging"));

    let core_src = staging_dir.join("mutil-zalo-core.exe");
    let dll_src = staging_dir.join("WebView2Loader.dll");

    println!("cargo:rerun-if-changed={}", core_src.display());
    println!("cargo:rerun-if-changed={}", dll_src.display());

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let require_payload = env::var("MUTIL_ZALO_REQUIRE_PAYLOAD")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let (core_raw, dll_raw, has_real) = if core_src.exists() && dll_src.exists() {
        let c = fs::read(&core_src).expect("failed to read staged mutil-zalo-core.exe");
        let d = fs::read(&dll_src).expect("failed to read staged WebView2Loader.dll");
        (c, d, true)
    } else if require_payload {
        panic!(
            "Thiếu file staging để đóng gói 1 file duy nhất!\n  Cần: {}\n  Và:  {}\nHãy chạy .\\build.ps1 để build tự động.",
            core_src.display(),
            dll_src.display()
        );
    } else {
        // Placeholder bytes so `cargo test --workspace` works before running build.ps1
        (b"PLACEHOLDER_CORE".to_vec(), b"PLACEHOLDER_DLL".to_vec(), false)
    };

    let hash = fnv1a_64(&[&core_raw, &dll_raw]);
    let core_gz = gzip_bytes(&core_raw);
    let dll_gz = gzip_bytes(&dll_raw);

    fs::write(out_dir.join("core.exe.gz"), &core_gz).expect("write core.exe.gz");
    fs::write(out_dir.join("webview2loader.dll.gz"), &dll_gz).expect("write webview2loader.dll.gz");

    let meta = format!(
        "pub const HAS_REAL_PAYLOAD: bool = {has_real};\n\
         pub const PAYLOAD_HASH: &str = \"{hash:016x}\";\n\
         pub const CORE_RAW_LEN: u64 = {};\n\
         pub const DLL_RAW_LEN: u64 = {};\n",
        core_raw.len(),
        dll_raw.len()
    );
    fs::write(out_dir.join("payload_meta.rs"), meta).expect("write payload_meta.rs");
}
