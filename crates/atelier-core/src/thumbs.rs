//! Thumbnails (spec §5): `<cache>/thumbs/<project id>.webp`, fit within
//! 300×450 (and a 2× variant), WebP quality 80. Never written into the
//! library. Bookrack's existing `.bookrack_thumb.webp` files are copied in
//! instead of regenerating when they are at least as new as the source.

use std::fs;
use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use rayon::prelude::*;
use serde::Serialize;

use crate::scan::{mtime_ms, BOOKRACK_THUMB};

pub const THUMB_W: u32 = 300;
pub const THUMB_H: u32 = 450;
pub const QUALITY: f32 = 80.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Size {
    /// 300×450 — grid.
    Small,
    /// 600×900 — large covers and HiDPI.
    Large,
}

impl Size {
    pub fn dims(self) -> (u32, u32) {
        match self {
            Size::Small => (THUMB_W, THUMB_H),
            Size::Large => (THUMB_W * 2, THUMB_H * 2),
        }
    }
    fn suffix(self) -> &'static str {
        match self {
            Size::Small => "",
            Size::Large => "@2x",
        }
    }
}

pub fn thumb_path(cache_dir: &Path, project_id: i64, size: Size) -> PathBuf {
    cache_dir.join("thumbs").join(format!("{project_id}{}.webp", size.suffix()))
}

#[derive(Debug, Clone)]
pub struct ThumbJob {
    pub project_id: i64,
    /// Image to thumbnail (picked cover or detected cover).
    pub source: PathBuf,
    /// The project folder, to look for Bookrack's thumbnail. None when the
    /// source is a picked cover (Bookrack's thumb may be of the old one).
    pub project_dir: Option<PathBuf>,
    pub force: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    UpToDate,
    Generated,
    CopiedFromBookrack,
    Failed,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchReport {
    pub generated: Vec<i64>,
    pub copied: Vec<i64>,
    pub failed: Vec<i64>,
    pub up_to_date: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ThumbError {
    #[error("could not read image {0}: {1}")]
    Decode(PathBuf, image::ImageError),
    #[error("could not write thumbnail {0}: {1}")]
    Write(PathBuf, std::io::Error),
    #[error("could not encode thumbnail")]
    Encode,
}

/// Is `dest` missing or older than `src`?
pub fn is_stale(src: &Path, dest: &Path) -> bool {
    match (mtime_ms(src), mtime_ms(dest)) {
        (Some(s), Some(d)) => d < s,
        (_, None) => true,
        (None, Some(_)) => false,
    }
}

/// Decode `src`, fit within `max_w`×`max_h` keeping aspect ratio (triangle
/// filter, never upscaled), encode WebP from RGBA, write atomically.
pub fn make(src: &Path, dest: &Path, max_w: u32, max_h: u32) -> Result<(), ThumbError> {
    let img = image::open(src).map_err(|e| ThumbError::Decode(src.to_path_buf(), e))?;
    let (w, h) = (img.width(), img.height());
    let scale = (max_w as f32 / w as f32).min(max_h as f32 / h as f32).min(1.0);
    let (tw, th) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
    let small = if scale < 1.0 { img.resize_exact(tw, th, FilterType::Triangle) } else { img };
    let rgba = small.to_rgba8();
    let encoded = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height()).encode(QUALITY);
    if encoded.is_empty() {
        return Err(ThumbError::Encode);
    }
    write_atomic(dest, &encoded).map_err(|e| ThumbError::Write(dest.to_path_buf(), e))
}

fn write_atomic(dest: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = dest.with_extension("webp.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, dest)
}

/// Bring one project's small thumbnail up to date.
pub fn ensure(job: &ThumbJob, cache_dir: &Path) -> Outcome {
    let dest = thumb_path(cache_dir, job.project_id, Size::Small);
    if !job.force && !is_stale(&job.source, &dest) {
        return Outcome::UpToDate;
    }
    if !job.force {
        if let Some(dir) = &job.project_dir {
            let bk = dir.join(BOOKRACK_THUMB);
            if bk.is_file() && !is_stale(&job.source, &bk) {
                if let Some(parent) = dest.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if fs::copy(&bk, &dest).is_ok() {
                    return Outcome::CopiedFromBookrack;
                }
            }
        }
    }
    let (w, h) = Size::Small.dims();
    match make(&job.source, &dest, w, h) {
        Ok(()) => Outcome::Generated,
        Err(e) => {
            log::warn!("thumbnail for project {} failed: {e}", job.project_id);
            Outcome::Failed
        }
    }
}

/// Large (2×) thumbnail on demand; generated from the source, never copied.
pub fn ensure_large(job: &ThumbJob, cache_dir: &Path) -> Result<PathBuf, ThumbError> {
    let dest = thumb_path(cache_dir, job.project_id, Size::Large);
    if job.force || is_stale(&job.source, &dest) {
        let (w, h) = Size::Large.dims();
        make(&job.source, &dest, w, h)?;
    }
    Ok(dest)
}

/// Run many jobs in parallel; report back in one batch.
pub fn ensure_all(jobs: &[ThumbJob], cache_dir: &Path) -> BatchReport {
    let outcomes: Vec<(i64, Outcome)> = jobs.par_iter().map(|j| (j.project_id, ensure(j, cache_dir))).collect();
    let mut report = BatchReport::default();
    for (id, o) in outcomes {
        match o {
            Outcome::Generated => report.generated.push(id),
            Outcome::CopiedFromBookrack => report.copied.push(id),
            Outcome::Failed => report.failed.push(id),
            Outcome::UpToDate => report.up_to_date += 1,
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, w: u32, h: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbImage::from_pixel(w, h, image::Rgb([200, 80, 40])).save(path).unwrap();
    }

    #[test]
    fn generates_fitted_thumbnail_once() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("lib/P/cover.png");
        write_png(&src, 1200, 1200);
        let cache = tmp.path().join("cache");
        let job = ThumbJob { project_id: 7, source: src, project_dir: None, force: false };

        assert_eq!(ensure(&job, &cache), Outcome::Generated);
        let out = image::open(thumb_path(&cache, 7, Size::Small)).unwrap();
        assert_eq!((out.width(), out.height()), (300, 300));
        assert_eq!(ensure(&job, &cache), Outcome::UpToDate);
        assert_eq!(ensure(&ThumbJob { force: true, ..job }, &cache), Outcome::Generated);
    }

    #[test]
    fn copies_fresh_bookrack_thumb() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("lib/P");
        let src = dir.join("cover.png");
        write_png(&src, 100, 150);
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(dir.join(BOOKRACK_THUMB), b"pretend-webp").unwrap();
        let cache = tmp.path().join("cache");
        let job = ThumbJob { project_id: 1, source: src, project_dir: Some(dir), force: false };
        assert_eq!(ensure(&job, &cache), Outcome::CopiedFromBookrack);
        assert_eq!(fs::read(thumb_path(&cache, 1, Size::Small)).unwrap(), b"pretend-webp");
    }

    #[test]
    fn bad_image_fails_without_panicking() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("broken.jpg");
        fs::write(&src, b"not an image").unwrap();
        let job = ThumbJob { project_id: 2, source: src, project_dir: None, force: false };
        let report = ensure_all(&[job], &tmp.path().join("cache"));
        assert_eq!(report.failed, vec![2]);
    }
}
