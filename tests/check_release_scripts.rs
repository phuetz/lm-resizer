use std::fs;

#[test]
fn test_check_release_scripts_use_workspace() {
    let sh_content = fs::read_to_string("scripts/check-release.sh").expect("failed to read check-release.sh");
    assert!(
        sh_content.contains("cargo test --workspace"),
        "scripts/check-release.sh must use --workspace to test all members"
    );

    let ps1_content = fs::read_to_string("scripts/check-release.ps1").expect("failed to read check-release.ps1");
    assert!(
        ps1_content.contains("cargo test --workspace"),
        "scripts/check-release.ps1 must use --workspace to test all members"
    );
}
