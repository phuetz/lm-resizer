use std::fs;

#[test]
fn test_check_release_scripts_use_workspace() {
    let sh_content =
        fs::read_to_string("scripts/check-release.sh").expect("failed to read check-release.sh");
    assert!(
        sh_content.contains("cargo test --workspace"),
        "scripts/check-release.sh must use --workspace to test all members"
    );

    let ps1_content =
        fs::read_to_string("scripts/check-release.ps1").expect("failed to read check-release.ps1");
    assert!(
        ps1_content.contains("cargo test --workspace"),
        "scripts/check-release.ps1 must use --workspace to test all members"
    );
}

/// B1 (recette Windows): `[Reflection.Assembly]::Load("<nom partiel>")` échoue
/// sous Windows PowerShell 5.1 ; seule la forme `Add-Type -AssemblyName` y
/// résout l'assembly. Contrôle statique de tous les scripts PowerShell livrés.
#[test]
fn powershell_scripts_avoid_partial_name_assembly_load() {
    let mut scripts = vec![std::path::PathBuf::from("install.ps1")];
    for entry in fs::read_dir("scripts").expect("scripts dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|e| e == "ps1") {
            scripts.push(path);
        }
    }
    for path in scripts {
        let text = fs::read_to_string(&path).expect("read script");
        for (n, line) in text.lines().enumerate() {
            let code = line.split('#').next().unwrap_or("").to_ascii_lowercase();
            assert!(
                !code.contains("assembly]::load(") && !code.contains("loadwithpartialname"),
                "{}:{}: Assembly.Load par nom partiel non supporté sous PS 5.1; utiliser Add-Type -AssemblyName",
                path.display(),
                n + 1
            );
        }
    }
    let install = fs::read_to_string("install.ps1").expect("install.ps1");
    assert!(
        install.contains("Add-Type -AssemblyName System.IO.Compression.FileSystem"),
        "install.ps1 doit charger ZipFile via Add-Type -AssemblyName"
    );
}
