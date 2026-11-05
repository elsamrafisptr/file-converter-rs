//! Conversion jobs. Mirrors FileConverter's ConversionJobFactory: office documents go through
//! LibreOffice, PDFs through Ghostscript, images through ImageMagick, everything else through ffmpeg.

use crate::preset::{ext_of, AudioMode, Output, PostAction, Preset, VideoSpeed};
use crate::tools::{ffmpeg, ghostscript, libreoffice, magick};
use indicatif::ProgressBar;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const OFFICE: &[&str] = &["doc", "docx", "odt", "xls", "xlsx", "ods", "ppt", "pptx", "odp"];
const IMAGES: &[&str] = &[
    "xcf", "arw", "avif", "bmp", "cr2", "dds", "dng", "exr", "heic", "ico", "jfif", "jpg", "jpeg", "nef", "png", "psd",
    "raf", "svg", "tga", "tif", "tiff", "webp",
];

type R<T> = Result<T, String>;

/// Private temp path, unique per process and call (jobs run in parallel).
fn temp_path(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("file-converter-{}-{n}-{name}", std::process::id()))
}

fn num(x: f64) -> String {
    // Matches C#'s "#.##" formatting used for ffmpeg filter arguments.
    format!("{}", (x * 100.0).round() / 100.0)
}

/// Picks the encoder quality index for the nearest supported bitrate.
fn nearest(table: &[(u32, &'static str)], bitrate: u32) -> &'static str {
    table.iter().min_by_key(|(b, _)| b.abs_diff(bitrate)).unwrap().1
}

// https://trac.ffmpeg.org/wiki/Encode/MP3
pub const MP3_VBR: &[(u32, &str)] = &[
    (245, "0"),
    (225, "1"),
    (190, "2"),
    (175, "3"),
    (165, "4"),
    (130, "5"),
    (115, "6"),
    (100, "7"),
    (85, "8"),
    (65, "9"),
];
// http://wiki.hydrogenaud.io/index.php?title=Recommended_Ogg_Vorbis
pub const OGG_VBR: &[(u32, &str)] = &[
    (500, "10"),
    (320, "9"),
    (256, "8"),
    (224, "7"),
    (192, "6"),
    (160, "5"),
    (128, "4"),
    (112, "3"),
    (96, "2"),
    (80, "1"),
    (64, "0"),
    (48, "-1"),
    (32, "-2"),
];
// FileConverter's "FFMPEG retro engineering" table for the native aac encoder.
pub const AAC_VBR: &[(u32, &str)] = &[
    (460, "3.9"),
    (340, "3"),
    (256, "2.2"),
    (224, "1.9"),
    (192, "1.6"),
    (155, "1.3"),
    (128, "1"),
    (112, "0.9"),
    (96, "0.75"),
    (80, "0.6"),
    (64, "0.45"),
    (48, "0.3"),
    (32, "0.2"),
    (16, "0.1"),
];

fn x264_preset(s: VideoSpeed) -> &'static str {
    use VideoSpeed::*;
    match s {
        UltraFast => "ultrafast",
        SuperFast => "superfast",
        VeryFast => "veryfast",
        Faster => "faster",
        Fast => "fast",
        Medium => "medium",
        Slow => "slow",
        Slower => "slower",
        VerySlow => "veryslow",
    }
}

/// `-vf` content: scale, rotation and (for h264) pixel format.
fn video_filters(p: &Preset) -> String {
    let s = p.o.video_scale;
    let h264 = matches!(p.output, Output::Mkv | Output::Mp4);
    let mut f = Vec::new();
    if h264 {
        // libx264 with yuv420p needs even dimensions.
        f.push(format!("scale=trunc(iw*{0}/2)*2:trunc(ih*{0}/2)*2", num(s)));
    } else if (s - 1.0).abs() >= 0.005 {
        f.push(format!("scale=iw*{0}:ih*{0}", num(s)));
    }
    match p.o.video_rotation.round() as i32 {
        90 => f.push("transpose=2".into()),
        180 => f.push("vflip,hflip".into()),
        270 => f.push("transpose=1".into()),
        _ => {}
    }
    if h264 {
        f.push("format=yuv420p".into());
    }
    f.join(",")
}

/// Encoder arguments for one ffmpeg pass (input/output excluded).
pub fn ffmpeg_args(p: &Preset) -> R<Vec<String>> {
    let o = &p.o;
    if !o.ffmpeg_custom.trim().is_empty() {
        return shlex::split(&o.ffmpeg_custom).ok_or_else(|| "invalid ffmpeg_custom quoting".to_string());
    }
    let channels = if o.audio_channels > 0 { format!("-ac {}", o.audio_channels) } else { String::new() };
    let vf = video_filters(p);
    let vf = if vf.is_empty() { vec![] } else { vec!["-vf".to_string(), vf] };
    let audio = |codec: &str, q: &str| if o.enable_audio { format!("-c:a {codec} -q:a {q}") } else { "-an".into() };
    let mp3_meta = "-id3v2_version 3 -write_id3v1 1";

    let args = match p.output {
        Output::Aac => format!("-vn -c:a aac -q:a {} {channels} -write_apetag 1", nearest(AAC_VBR, o.audio_bitrate)),
        Output::Flac => format!("-vn -compression_level 12 {channels}"),
        Output::Ogg => format!("-vn -c:a libvorbis -q:a {} {channels}", nearest(OGG_VBR, o.audio_bitrate)),
        Output::Wav => {
            let codec = match o.audio_mode {
                AudioMode::Wav8 => "pcm_u8",
                AudioMode::Wav24 => "pcm_s24le",
                AudioMode::Wav32 => "pcm_s32le",
                _ => "pcm_s16le",
            };
            format!("-vn -c:a {codec} {channels}")
        }
        Output::Mp3 => match o.audio_mode {
            AudioMode::Mp3Cbr => format!("-vn -c:a libmp3lame -b:a {}k {channels} {mp3_meta}", o.audio_bitrate),
            _ => format!("-vn -c:a libmp3lame -q:a {} {channels} {mp3_meta}", nearest(MP3_VBR, o.audio_bitrate)),
        },
        Output::Avi => format!(
            "-c:v mpeg4 -vtag xvid -q:v {} {}",
            31u32.saturating_sub(o.video_quality).max(1),
            audio("libmp3lame", nearest(MP3_VBR, o.audio_bitrate))
        ),
        Output::Mkv | Output::Mp4 => format!(
            "-c:v libx264 -preset {} -crf {} {}",
            x264_preset(o.video_speed),
            51u32.saturating_sub(o.video_quality),
            audio("aac", nearest(AAC_VBR, o.audio_bitrate))
        ),
        Output::Ogv => format!(
            "-c:v libtheora -q:v {} {}",
            o.video_quality.min(10),
            audio("libvorbis", nearest(OGG_VBR, o.audio_bitrate))
        ),
        Output::Webm => {
            // Maximum quality means lossless. https://trac.ffmpeg.org/wiki/Encode/VP9
            let q = if o.video_quality >= 63 {
                "-lossless 1".into()
            } else {
                format!("-crf {} -b:v 0", 63 - o.video_quality)
            };
            format!("-c:v libvpx-vp9 {q} {}", audio("libvorbis", nearest(OGG_VBR, o.audio_bitrate)))
        }
        other => return Err(format!("ffmpeg cannot produce {other:?} in this tool")),
    };
    Ok(args.split_whitespace().map(String::from).chain(vf).collect())
}

/// Runs ffmpeg, feeding `-progress` output into the bar.
fn run_ffmpeg(input: &Path, args: &[String], output: &Path, bar: &ProgressBar) -> R<()> {
    let mut child = Command::new(ffmpeg()?)
        .args(["-hide_banner", "-nostdin", "-n", "-nostats", "-progress", "pipe:1"])
        .arg("-i")
        .arg(input)
        .args(args)
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg: {e}"))?;

    let duration_us = AtomicU64::new(0);
    let stderr = child.stderr.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let log = std::thread::scope(|s| {
        let err = s.spawn(|| {
            let mut tail = Vec::new();
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                // "  Duration: 00:01:02.34, start: ..."
                if let Some(d) = line.trim_start().strip_prefix("Duration: ").and_then(|r| r.get(..11)) {
                    let mut it = d.split(':').map(|x| x.parse::<f64>().unwrap_or(0.0));
                    let (h, m, sec) = (it.next().unwrap_or(0.0), it.next().unwrap_or(0.0), it.next().unwrap_or(0.0));
                    if duration_us.load(Ordering::Relaxed) == 0 {
                        duration_us.store(((h * 3600.0 + m * 60.0 + sec) * 1e6) as u64, Ordering::Relaxed);
                    }
                }
                tail.push(line);
                if tail.len() > 8 {
                    tail.remove(0);
                }
            }
            tail
        });
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.parse::<u64>().ok()) {
                let total = duration_us.load(Ordering::Relaxed);
                if total > 0 {
                    bar.set_position((us * 100 / total).min(100));
                }
            }
        }
        err.join().unwrap_or_default()
    });

    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("ffmpeg failed:\n{}", log.join("\n")))
    }
}

fn run(cmd: &mut Command, name: &str) -> R<()> {
    let out = cmd.stdin(Stdio::null()).output().map_err(|e| format!("{name}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let mut msg = String::from_utf8_lossy(&out.stderr).into_owned();
        msg.push_str(&String::from_utf8_lossy(&out.stdout));
        Err(format!("{name} failed: {}", msg.trim()))
    }
}

fn magick_convert(input: &Path, p: &Preset, output: &Path) -> R<()> {
    let o = &p.o;
    let ext = ext_of(input);
    let mut src = input.as_os_str().to_owned();
    match ext.as_str() {
        // Raw files are otherwise read as TIFF.
        "cr2" | "dng" => src = format!("{ext}:{}", input.display()).into(),
        // First frame only.
        "gif" => src.push("[0]"),
        _ => {}
    }
    let mut cmd = Command::new(magick()?);
    cmd.arg(&src);
    // GIF presets share the video scale/rotation settings, whatever the input.
    let (scale, rotation) =
        if p.output == Output::Gif { (o.video_scale, o.video_rotation) } else { (o.image_scale, o.image_rotation) };
    if (scale - 1.0).abs() >= 0.005 {
        cmd.arg("-scale").arg(format!("{}%", num(scale * 100.0)));
    }
    if rotation.abs() >= 0.05 {
        // ImageMagick rotates clockwise; presets use counter-clockwise ("Rotate left" = 90).
        cmd.arg("-rotate").arg(num(-rotation));
    }
    if o.image_pow2 {
        // Largest power of two not above the smallest side, applied to both sides.
        cmd.args(["-scale", "%[fx:2^floor(log(min(w,h))/log(2))]x%[fx:2^floor(log(min(w,h))/log(2))]!"]);
    }
    if o.image_max_size > 0 {
        cmd.arg("-resize").arg(format!("{0}x{0}>", o.image_max_size));
    }
    match p.output {
        Output::Jpg | Output::Webp | Output::Avif => {
            cmd.arg("-quality").arg(o.image_quality.to_string());
        }
        Output::Png => {
            cmd.args(["-quality", "95"]);
        }
        Output::Pdf => {
            cmd.args(["-units", "PixelsPerInch", "-density", "200"]);
        }
        Output::Ico => {
            cmd.args(["-define", "icon:auto-resize=256,128,64,48,32,16"]);
        }
        Output::Gif => {}
        other => return Err(format!("ImageMagick cannot produce {other:?} in this tool")),
    }
    cmd.arg(output);
    run(&mut cmd, "ImageMagick")
}

/// One PNG per PDF page: `name-1.png`, `name-2.png`, ...
fn pdf_to_png(input: &Path, p: &Preset, output: &Path) -> R<Vec<PathBuf>> {
    let tmp = temp_path("pages");
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let dpi = 200.0 * p.o.image_scale;
    let res = run(
        Command::new(ghostscript()?)
            .args(["-dSAFER", "-dBATCH", "-dNOPAUSE", "-dQUIET", "-sDEVICE=png16m"])
            .args(["-dTextAlphaBits=4", "-dGraphicsAlphaBits=4"])
            .arg(format!("-r{}", num(dpi)))
            .arg(format!("-sOutputFile={}", tmp.join("%d.png").display()))
            .arg(input),
        "Ghostscript",
    )
    .and_then(|_| {
        // Rendered in a temp dir so existing `name-N.png` files are never overwritten.
        let base = output.with_extension("");
        let mut outs = Vec::new();
        for i in (1..).take_while(|i| tmp.join(format!("{i}.png")).exists()) {
            let dest = crate::preset::unique_path(PathBuf::from(format!("{}-{i}.png", base.display())));
            move_file(&tmp.join(format!("{i}.png")), &dest)?;
            outs.push(dest);
        }
        Ok(outs)
    });
    let _ = std::fs::remove_dir_all(&tmp);
    res
}

/// LibreOffice → PDF in a private temp dir (separate profile so parallel jobs don't collide).
fn office_to_pdf(input: &Path, tmp: &Path) -> R<PathBuf> {
    let profile = tmp.join("profile");
    let mut url = profile.to_string_lossy().replace('\\', "/").replace(' ', "%20");
    if !url.starts_with('/') {
        url.insert(0, '/');
    }
    run(
        Command::new(libreoffice()?)
            .arg(format!("-env:UserInstallation=file://{url}"))
            .args(["--headless", "--convert-to", "pdf", "--outdir"])
            .arg(tmp)
            .arg(input),
        "LibreOffice",
    )?;
    let pdf = tmp.join(input.file_stem().unwrap()).with_extension("pdf");
    if pdf.exists() {
        Ok(pdf)
    } else {
        Err("LibreOffice produced no PDF".into())
    }
}

fn move_file(from: &Path, to: &Path) -> R<()> {
    std::fs::rename(from, to)
        .or_else(|_| std::fs::copy(from, to).and_then(|_| std::fs::remove_file(from)))
        .map_err(|e| format!("move {} -> {}: {e}", from.display(), to.display()))
}

/// Converts one file; returns the produced files.
fn convert_inner(input: &Path, p: &Preset, output: &Path, bar: &ProgressBar) -> R<Vec<PathBuf>> {
    let ext = ext_of(input);
    if OFFICE.contains(&ext.as_str()) {
        let tmp = temp_path("office");
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        bar.set_message("LibreOffice");
        let res = office_to_pdf(input, &tmp).and_then(|pdf| match p.output {
            Output::Pdf => move_file(&pdf, output).map(|_| vec![output.to_path_buf()]),
            Output::Png => pdf_to_png(&pdf, p, output),
            other => Err(format!("office documents can only become PDF or PNG, not {other:?}")),
        });
        let _ = std::fs::remove_dir_all(&tmp);
        return res;
    }
    if ext == "pdf" {
        bar.set_message("Ghostscript");
        return match p.output {
            Output::Png => pdf_to_png(input, p, output),
            other => Err(format!("PDF can only become PNG, not {other:?}")),
        };
    }
    let image_in = IMAGES.contains(&ext.as_str());
    match p.output {
        Output::Ico | Output::Pdf | Output::Avif | Output::Jpg | Output::Png | Output::Webp => {
            bar.set_message("ImageMagick");
            magick_convert(input, p, output)?;
        }
        Output::Gif if image_in => {
            bar.set_message("ImageMagick");
            magick_convert(input, p, output)?;
        }
        Output::Gif if p.o.ffmpeg_custom.trim().is_empty() => {
            // Two passes for a good palette: http://blog.pkh.me/p/21-high-quality-gif-with-ffmpeg.html
            let mut vf = video_filters(p);
            if !vf.is_empty() {
                vf.push(',');
            }
            vf.push_str(&format!("fps={}", p.o.fps));
            let palette = temp_path("palette.png");
            bar.set_message("indexing colors");
            let res = run_ffmpeg(input, &["-vf".into(), format!("{vf},palettegen")], &palette, bar).and_then(|_| {
                bar.set_message("ffmpeg");
                let args = [
                    "-i".into(),
                    palette.display().to_string(),
                    "-lavfi".into(),
                    format!("{vf} [x]; [x][1:v] paletteuse"),
                ];
                run_ffmpeg(input, &args, output, bar)
            });
            let _ = std::fs::remove_file(&palette);
            res?;
        }
        _ => {
            bar.set_message("ffmpeg");
            run_ffmpeg(input, &ffmpeg_args(p)?, output, bar)?;
        }
    }
    Ok(vec![output.to_path_buf()])
}

/// Full job: output path, conversion, post-conversion action on the input.
pub fn convert(input: &Path, p: &Preset, bar: &ProgressBar) -> R<Vec<PathBuf>> {
    let abs = std::path::absolute(input).map_err(|e| e.to_string())?;
    let input = abs.as_path();
    if !input.is_file() {
        return Err("not a file".into());
    }
    if !p.accepts(input) {
        return Err(format!("preset '{}' does not accept .{} files", p.name, ext_of(input)));
    }
    let wanted = crate::preset::output_path(input, p);
    if let Some(dir) = wanted.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    // If the input will be removed anyway, move it aside so the output can take its exact name.
    let mut src = input.to_path_buf();
    if p.o.post_action != PostAction::None && wanted == input {
        let stem = input.file_stem().unwrap().to_string_lossy();
        src = crate::preset::unique_path(input.with_file_name(format!("{stem}_TEMP.{}", ext_of(input))));
        move_file(input, &src)?;
    }
    let output = crate::preset::unique_path(wanted);
    let res = convert_inner(&src, p, &output, bar);
    match (&res, p.o.post_action) {
        (Err(_), _) | (Ok(_), PostAction::None) => {
            if src != input {
                move_file(&src, input)?;
            }
        }
        (Ok(_), PostAction::Delete) => std::fs::remove_file(&src).map_err(|e| e.to_string())?,
        (Ok(_), PostAction::Archive) => {
            let dir = input.parent().unwrap_or(Path::new(".")).join("Conversion Archive");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            move_file(&src, &crate::preset::unique_path(dir.join(input.file_name().unwrap())))?;
        }
    }
    if res.is_err() {
        let _ = std::fs::remove_file(&output);
    }
    res
}
