//! Make the self-contained build: the app with the data appended (see src/store.rs).
//!   cargo build --release && cargo run --release --bin bundle [-- [--max] [<data dir> [<out.exe>]]]
//! Writes ../dist/WT-Ballistic-Visualizer-Lite.exe, or with --max ../dist/WT-Ballistic-Visualizer-Max.exe: the
//! same plus each whole level around its tank map (maps/*.far.*, extract.py --far) and the missions' object
//! groups (maps/*.groups.*: airfields, heli pads) with their ground slabs (slabs/*). Lite leaves those out. To keep both small:
//! - tank models: only a Leopard 2A4 (shown at A for every tank) and the T-72A at B
//! - heightmaps, objects and tank models stored raw and packed with xz; heightmaps as residuals of a gradient
//!   predictor (lossless), object mesh positions as 16-bit values over each model's bounding box (~3 mm on 200 m)
//! - ground colours at most 2048 px (the heightmap grid on 4 km maps), JPEG quality 85
//!   The detail textures that add the close-up pattern in 3D are kept as they are.

#[path = "../store.rs"]
#[allow(dead_code)] // the bundler only writes packs
mod store;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::Engine as _;
use serde_json::Value;

const LITE_A: &str = "leopard_2a4";
const LITE_B: &str = "t_72a";
const LITE_GROUND_PX: u32 = 2048;

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let max = std::env::args().any(|a| a == "--max");
    let args: Vec<String> = std::env::args().skip(1).filter(|a| a != "--lite" && a != "--max").collect(); // --lite: the old flag
    let data = args.first().map(PathBuf::from).unwrap_or_else(|| root.join("../data"));
    let out = args.get(1).map(PathBuf::from).unwrap_or_else(|| root.join(if max { "../dist/WT-Ballistic-Visualizer-Max.exe" } else { "../dist/WT-Ballistic-Visualizer-Lite.exe" }));
    let exe = std::env::current_exe()?.with_file_name("wtv.exe");
    anyhow::ensure!(exe.exists(), "{} not found: run `cargo build --release` first", exe.display());
    anyhow::ensure!(data.join("index.js").exists(), "{} has no index.js: run the extractor first", data.display());

    let mut jobs = list(&data)?;
    jobs.retain(|r| r.strip_prefix("tanks/").is_none_or(|m| m == format!("{LITE_A}.js") || m == format!("{LITE_B}.js")));
    jobs.retain(|r| max || !is_whole_level(r));
    if max {
        anyhow::ensure!(jobs.iter().any(|r| is_whole_level(r)), "{} has no whole-level files: run extract.py --far first", data.display());
    }
    let t0 = std::time::Instant::now();
    let progress = |done: usize, all: usize| eprint!("\rpacking {done}/{all} files");
    let (n, size) = store::bundle(&exe, &out, &jobs, |r| lite_entries(&data, r, max), progress)?;
    eprintln!("\nwrote {} ({n} entries, {:.0} MB) in {:.0} s", out.display(), size as f64 / 1e6, t0.elapsed().as_secs_f64());
    Ok(())
}

/// The Max build's files: a whole level around a tank map, the missions' object groups, or the ground slabs of
/// their airfields and heli pads.
fn is_whole_level(rel: &str) -> bool {
    rel.contains(".far.") || rel.contains(".groups.") || rel.starts_with("slabs/")
}

/// Every .js file under `data`, as paths relative to it with forward slashes.
fn list(data: &Path) -> Result<Vec<String>> {
    let mut rel = vec![];
    let mut stack = vec![data.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)? {
            let p = e?.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "js") {
                rel.push(p.strip_prefix(data)?.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    rel.sort();
    Ok(rel)
}

/// `NAME(arg, ...);` -> (NAME, args)
fn parse_call(text: &str) -> Result<(String, Vec<Value>)> {
    let t = text.trim();
    let open = t.find('(').context("not a data file")?;
    let body = t[open + 1..].trim_end_matches(';').trim_end().strip_suffix(')').context("unterminated call")?;
    Ok((t[..open].to_string(), serde_json::from_str(&format!("[{body}]"))?))
}
fn write_call(name: &str, args: &[Value]) -> Vec<u8> {
    let list: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    format!("{name}({});\n", list.join(",")).into_bytes()
}
fn b64(s: &str) -> Result<Vec<u8>> {
    Ok(base64::engine::general_purpose::STANDARD.decode(s)?)
}
fn inflate(z: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut out = vec![];
    flate2::read::ZlibDecoder::new(z).read_to_end(&mut out)?;
    Ok(out)
}
/// Bytes of a `data:<mime>;base64,<...>` URL.
fn data_url(v: &Value) -> Result<Vec<u8>> {
    b64(v.as_str().and_then(|s| s.split_once(";base64,")).context("not a data URL")?.1)
}

/// The lite entries of one data file: the file with its big payloads replaced by "@<entry>", and those entries.
fn lite_entries(data: &Path, rel: &str, max: bool) -> Result<Vec<(String, Vec<u8>)>> {
    let text = std::fs::read_to_string(data.join(rel))?;
    let base = rel.trim_end_matches(".js");
    let (name, mut a) = parse_call(&text)?;
    let mut out = vec![];
    match name.as_str() {
        "WTV_INDEX" => {
            for t in a[0]["tanks"].as_array_mut().context("index without tanks")? {
                t["model"] = Value::from(LITE_A); // every tank shows the same model at A
            }
            if !max {
                for m in a[0]["maps"].as_array_mut().context("index without maps")? {
                    m.as_object_mut().map(|o| o.remove("far")); // Lite has no whole-level files
                }
            }
        }
        "WTV_MAP" => {
            let w = a[0]["width"].as_u64().context("map width")? as usize;
            let mut raw = inflate(&b64(a[0]["h"].as_str().context("map has no heightmap")?)?)?;
            store::predict(&mut raw, w);
            let entry = format!("{base}{}", store::PREDICTED);
            a[0]["h"] = Value::from(format!("@{entry}"));
            out.push((entry, raw));
        }
        "WTV_OBJ" | "WTV_TANK" => {
            let entry = format!("{base}.bin");
            let raw = inflate(&b64(a[1].as_str().context("no payload")?)?)?;
            out.push((entry.clone(), if name == "WTV_OBJ" { store::quantize_objects(&raw)? } else { raw }));
            a[1] = Value::from(format!("@{entry}"));
        }
        "WTV_TEX" => {
            let img = image::load_from_memory(&data_url(&a[1])?)?;
            let img = if img.width() > LITE_GROUND_PX { img.resize(LITE_GROUND_PX, LITE_GROUND_PX, image::imageops::FilterType::Lanczos3) } else { img };
            let mut jpg = vec![];
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 85).encode_image(&img.to_rgb8())?;
            let entry = format!("{base}.ground.jpg");
            out.push((entry.clone(), jpg));
            a[1] = Value::from(format!("@{entry}"));
            if let Some(d) = a.get_mut(2).and_then(Value::as_object_mut) {
                for key in ["idx", "strip"] {
                    let entry = format!("{base}.detail-{key}.png");
                    out.push((entry.clone(), data_url(d.get(key).context("detail image")?)?));
                    d.insert(key.into(), Value::from(format!("@{entry}")));
                }
            }
        }
        "WTV_SLAB" => {} // a small mesh and its JPEG textures, kept as they are
        other => anyhow::bail!("{rel}: unknown data file {other}"),
    }
    out.push((rel.to_string(), write_call(&name, &a)));
    Ok(out)
}
