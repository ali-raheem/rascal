use std::process::Command;

fn rascal(args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_rascal"))
        .arg("--no-config")
        .args(args)
        .output()
        .expect("run rascal");
    (
        out.status.success(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn world_map_has_requested_size_and_chars() {
    let (ok, out, _) = rascal(&["-w", "80", "-H", "20", "-f", "--land", "#", "--water", "-"]);
    assert!(ok);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 22);
    assert!(lines.iter().all(|l| l.chars().count() == 82));
    assert!(out.contains('#') && out.contains('-'));
}

#[test]
fn markers_and_legend() {
    let (ok, out, _) = rascal(&[
        "europe",
        "-w",
        "60",
        "-m",
        "Paris,%",
        "-m",
        "-33.9,18.4,@,Cape",
        "--legend",
    ]);
    assert!(ok);
    assert!(out.contains("% Paris"));
    assert!(out.contains("@  Cape (-33.900, 18.400)"));
}

#[test]
fn bad_input_fails_cleanly() {
    let (ok, _, err) = rascal(&["atlantis"]);
    assert!(!ok);
    assert!(err.contains("unknown region"));
}

#[test]
fn config_file_is_applied_and_cli_wins() {
    let dir = std::env::temp_dir().join(format!("rascal-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("c.toml");
    std::fs::write(
        &cfg,
        "width = 30\nregion = \"africa\"\n[chars]\nland = \"&\"\nwater = \".\"\n",
    )
    .unwrap();
    let (ok, out, err) = rascal(&["-c", cfg.to_str().unwrap(), "--water", "~"]);
    std::fs::remove_dir_all(&dir).ok();
    assert!(ok, "{err}");
    assert!(out.contains('&') && out.contains('~') && !out.contains('.'));
    assert!(out.lines().all(|l| l.chars().count() <= 30));
}
