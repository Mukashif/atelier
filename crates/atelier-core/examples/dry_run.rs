//! Run the core end to end against a real library without touching it.
//!
//! cargo run --release -p atelier-core --example dry_run -- <library root> <bookrack data dir> <scratch dir>
//!
//! Reads the library and Bookrack's data; writes only into <scratch dir>
//! (database + thumbnail cache).

use std::path::PathBuf;
use std::time::Instant;

use atelier_core::{db::Db, import, scan, thumb_jobs, thumbs, Query};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, bookrack, scratch] = &args[..] else {
        eprintln!("usage: dry_run <library root> <bookrack data dir> <scratch dir>");
        std::process::exit(2);
    };
    let (root, bookrack, scratch) = (PathBuf::from(root), PathBuf::from(bookrack), PathBuf::from(scratch));
    std::fs::create_dir_all(&scratch).unwrap();
    let db_path = scratch.join("atelier.db");
    let _ = std::fs::remove_file(&db_path);

    let t = Instant::now();
    let scanned = scan::scan_root(&root).expect("scan");
    println!(
        "scan: {} projects in {} month folders, {} skipped — {:?}",
        scanned.projects.len(),
        scanned.month_folders,
        scanned.skipped.len(),
        t.elapsed()
    );

    let t = Instant::now();
    let mut db = Db::open(&db_path).expect("open db");
    db.set_root_folder(&root).unwrap();
    let diff = db.apply_scan(&scanned.projects).unwrap();
    println!("store: +{} added, {} missing — {:?}", diff.added, diff.missing, t.elapsed());

    let t = Instant::now();
    let data = import::load_data_dir(&bookrack).expect("bookrack data");
    let report = import::apply(&mut db, &data, &root).expect("import");
    println!(
        "import: {} imported, {} orphans, {} outside root, {} fav, {} archived, {} tags — {:?}",
        report.imported,
        report.orphans,
        report.outside_root.len(),
        report.favorites,
        report.archived,
        report.tags,
        t.elapsed()
    );

    let t = Instant::now();
    let all = db.list(&Query::default()).unwrap();
    println!("query (library view): {} rows — {:?}", all.len(), t.elapsed());
    let t = Instant::now();
    let hits = db.list(&Query { search: Some("wolf".into()), ..Default::default() }).unwrap();
    println!("search 'wolf': {} hits — {:?}", hits.len(), t.elapsed());

    let mut status_counts = std::collections::BTreeMap::new();
    for r in &all {
        *status_counts.entry(r.status.clone()).or_insert(0) += 1;
    }
    println!("statuses (incl. defaults): {status_counts:?}");
    let no_cover = all.iter().filter(|r| r.cover_kind.is_none() && r.manual_cover.is_none()).count();
    let unsupported = all.iter().filter(|r| r.cover_kind.as_deref() == Some("unsupported") && r.manual_cover.is_none()).count();
    println!("covers: {no_cover} none, {unsupported} unsupported-only");

    let cache = scratch.join("cache");
    let jobs = thumb_jobs(&all, &root);
    let t = Instant::now();
    let r1 = thumbs::ensure_all(&jobs, &cache);
    println!(
        "thumbs (first run): {} jobs → {} copied from Bookrack, {} generated, {} failed — {:?}",
        jobs.len(),
        r1.copied.len(),
        r1.generated.len(),
        r1.failed.len(),
        t.elapsed()
    );
    let t = Instant::now();
    let r2 = thumbs::ensure_all(&jobs, &cache);
    println!("thumbs (second run): {} up to date — {:?}", r2.up_to_date, t.elapsed());
    if !r1.failed.is_empty() {
        let names: Vec<_> = all.iter().filter(|r| r1.failed.contains(&r.id)).map(|r| r.rel_path.clone()).collect();
        println!("failed: {names:?}");
    }
}
