//! Moving data from the pre-rename folder (runs alone in its own process because it sets XDG_DATA_HOME).

#[test]
fn old_mogger_folder_moves_to_pmog_once() {
    let home = std::env::temp_dir().join(format!("pmog-rename-{}", std::process::id()));
    std::fs::create_dir_all(home.join("mogger")).unwrap();
    std::fs::write(home.join("mogger").join("machine-id"), "abc").unwrap();
    // SAFETY: this test binary has a single test, so no other thread reads the environment.
    unsafe { std::env::set_var("XDG_DATA_HOME", &home) };
    let dir = pmog::utils::data_dir().unwrap();
    assert_eq!(dir, home.join("pmog"));
    assert_eq!(std::fs::read_to_string(dir.join("machine-id")).unwrap(), "abc", "identity kept");
    assert!(!home.join("mogger").exists());
    std::fs::create_dir_all(home.join("mogger")).unwrap();
    pmog::utils::data_dir().unwrap();
    assert!(home.join("mogger").exists(), "only moved when the new folder is missing");
    let _ = std::fs::remove_dir_all(&home);
}
