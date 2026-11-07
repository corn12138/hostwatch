use super::*;
fn fixture() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "hostwatch-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(p.join("Downloads/nested")).unwrap();
    fs::write(p.join("Downloads/a.bin"), vec![1; 4096]).unwrap();
    fs::write(p.join("Downloads/nested/b.bin"), vec![1; 2048]).unwrap();
    p
}
fn options(root: PathBuf) -> Options {
    Options {
        root,
        seconds: 10,
        max_entries: 100,
        target: None,
        json: true,
    }
}
#[test]
fn scan_aggregates_and_does_not_modify_files() {
    let root = fixture();
    let r = scan(&options(root.clone())).unwrap();
    assert!(r.complete);
    assert_eq!(r.logical_bytes, 6144);
    assert_eq!(r.files, 2);
    assert!(r
        .directories
        .iter()
        .any(|d| d.path.ends_with("Downloads") && d.bytes == 6144));
    assert_eq!(r.largest_files[0].bytes, 4096);
    fs::create_dir(root.join("node_modules")).unwrap();
    fs::write(root.join("node_modules/package.json"), b"{}").unwrap();
    let nested = scan(&options(root.join("node_modules"))).unwrap();
    #[cfg(windows)]
    assert!(nested
        .strategies
        .iter()
        .any(|s| s.title == "调整开发依赖与缓存位置"));
    let _ = nested;
    assert_eq!(
        fs::read(root.join("Downloads/a.bin")).unwrap(),
        vec![1; 4096]
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn bounded_scan_is_partial_and_invalid_root_fails() {
    let root = fixture();
    let mut o = options(root.clone());
    o.max_entries = 1;
    let r = scan(&o).unwrap();
    assert!(!r.complete);
    assert_eq!(r.entries_visited, 1);
    o.root = root.join("missing");
    assert!(scan(&o).is_err());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn invalid_limits_and_parameters_are_rejected() {
    for a in [
        ["--seconds", "0"],
        ["--max-entries", "0"],
        ["--seconds", "nan"],
        ["--wat", "1"],
    ] {
        assert!(parse(&a.map(String::from)).is_err());
    }
}
#[cfg(windows)]
#[test]
fn junctions_are_skipped_without_recursion() {
    let root = fixture();
    let output = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(root.join("loop"))
        .arg(&root)
        .output()
        .unwrap();
    assert!(output.status.success());
    let r = scan(&options(root.clone())).unwrap();
    assert_eq!(r.logical_bytes, 6144);
    assert_eq!(r.skipped_reparse_or_cloud, 1);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn migration_previews_are_quoted_and_never_delete_or_copy() {
    let drives = vec![crate::model::Disk {
        path: "D:\\".into(),
        total_bytes: 100 * 1073741824,
        available_bytes: 80 * 1073741824,
        important_available_bytes: None,
    }];
    let entries = vec![Entry {
        path: "C:\\Users\\O'Brien\\Downloads".into(),
        bytes: 1073741824,
        files: 1,
        category: "个人文件",
    }];
    let p = plan::build(
        &entries,
        &drives,
        Some(Path::new("D:\\")),
        Path::new("C:\\"),
        true,
    )
    .unwrap();
    #[cfg(windows)]
    {
        let c = p[0].preview_command.as_ref().unwrap();
        assert!(c.contains("O''Brien"));
        assert!(c.ends_with(" /L"));
        assert!(!c.contains("/MOVE"));
        assert!(!c.contains("/MIR"));
        assert_eq!(p[0].target_has_capacity, Some(true));
    }
    assert!(plan::build(
        &entries,
        &drives,
        Some(Path::new("C:\\")),
        Path::new("C:\\"),
        true
    )
    .is_err());
    let mut full = drives;
    full[0].available_bytes = 1;
    #[cfg(windows)]
    {
        let p = plan::build(
            &entries,
            &full,
            Some(Path::new("D:\\")),
            Path::new("C:\\"),
            true,
        )
        .unwrap();
        assert_eq!(p[0].target_has_capacity, Some(false));
        assert!(p[0].preview_command.is_none());
    }
    let _ = p;
}

#[cfg(windows)]
#[test]
fn partial_scan_does_not_claim_target_capacity_and_unicode_target_cannot_panic() {
    let drives = vec![crate::model::Disk {
        path: "D:\\".into(),
        total_bytes: 100 * 1073741824,
        available_bytes: 80 * 1073741824,
        important_available_bytes: None,
    }];
    let entries = vec![Entry {
        path: "C:\\Users\\Alice\\Downloads".into(),
        bytes: 1073741824,
        files: 1,
        category: "个人文件",
    }];
    let p = plan::build(
        &entries,
        &drives,
        Some(Path::new("D:\\")),
        Path::new("C:\\"),
        false,
    )
    .unwrap();
    assert_eq!(p[0].target_has_capacity, None);
    assert!(p[0].preview_command.as_ref().unwrap().ends_with(" /L"));
    assert!(plan::build(
        &entries,
        &drives,
        Some(Path::new("D汉字")),
        Path::new("C:\\"),
        true
    )
    .is_err());
}
