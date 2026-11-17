//! Moving data from the pre-rename folder.

use pmog::utils::data_dir_in;

#[test]
fn old_mogger_folder_moves_to_pmog_once() {
    let home = std::env::temp_dir().join(format!("pmog-rename-{}", std::process::id()));
    std::fs::create_dir_all(home.join("mogger")).unwrap();
    std::fs::write(home.join("mogger").join("machine-id"), "abc").unwrap();
    let dir = data_dir_in(&home);
    assert_eq!(dir, home.join("pmog"));
    assert_eq!(std::fs::read_to_string(dir.join("machine-id")).unwrap(), "abc", "identity kept");
    assert!(!home.join("mogger").exists());
    std::fs::create_dir_all(home.join("mogger")).unwrap();
    data_dir_in(&home);
    assert!(home.join("mogger").exists(), "only moved when the new folder is missing");
    let _ = std::fs::remove_dir_all(&home);
}
