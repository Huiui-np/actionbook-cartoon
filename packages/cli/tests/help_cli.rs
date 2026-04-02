use assert_cmd::Command;

#[test]
fn top_level_help_marks_setup_as_coming_soon() {
    let output = Command::cargo_bin("actionbook")
        .expect("binary exists")
        .arg("--help")
        .output()
        .expect("run --help");

    assert!(
        output.status.success(),
        "expected --help success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("setup      Coming soon"),
        "top-level help should mark setup as coming soon\nstdout:\n{stdout}"
    );
    assert!(
        !stdout.contains("setup      Configure actionbook"),
        "top-level help should no longer advertise setup as available\nstdout:\n{stdout}"
    );
}
