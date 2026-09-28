# Provenance des trois captures réelles

Ces sources minimales servent à reproduire les sorties du corpus `compile_error`, `git_diff` et `dotnet_ok`. Les trois fichiers du corpus sont des sorties capturées, sans lignes de remplissage fabriquées. Un nouveau rejeu peut changer les chemins temporaires, les durées et les versions affichées ; les chiffres versionnés concernent uniquement les octets versionnés.

Versions de la capture : `rustc 1.95.0`, `git 2.43.0`, SDK `.NET 10.0.300` ciblant `net8.0`, `xunit 2.5.3` et `Microsoft.NET.Test.Sdk 17.14.1`. Les fichiers ne contiennent aucune donnée personnelle.

## Erreur Rust

Copier `rust/` dans un répertoire temporaire hors du workspace puis y lancer `cargo build -q --offline`, en capturant stdout et stderr ensemble. Le code de sortie est 101. Le diagnostic commence par `error[E0308]: mismatched types`, suivi de `--> src/main.rs:2:22`. Un appel direct à `rustc --crate-name atlas_ledger src/main.rs --emit=metadata` a également été exécuté (code 1) pour contrôler la forme du diagnostic du compilateur. La fixture est la sortie réelle de `cargo build -q --offline`, qui lance `rustc`.

SHA-256 de `bench/corpus/compile_error.txt` : `df14c5acc006d6a12df3a25da2ceeac2b605f54002e1ba5647024afa7eda1644`.

## Diff Git

Dans un dépôt temporaire, copier les deux fichiers `git/*-before.py.fixture` sous `src/billing.py` et `tests/test_billing.py`, faire `git init`, `git add src/billing.py`, `git add tests/test_billing.py`, puis un commit de base avec une identité fictive (`Fixture <fixture@example.invalid>`). Remplacer ces deux fichiers par `git/*-after.py.fixture` et capturer `git diff -- src/billing.py tests/test_billing.py`. Le test du harnais vérifie que les longueurs déclarées par les deux en-têtes de hunk correspondent à leur corps.

SHA-256 de `bench/corpus/git_diff.txt` : `4c381aa1327117f4fc05717928a1a669d87f389a970a85b12a8efad78fe2beb8`.

## Tests .NET

Copier `dotnet/` dans un répertoire temporaire, exécuter `dotnet restore Atlas.Tests.csproj`, puis `dotnet test Atlas.Tests.csproj --no-restore`. Les 65 données du test xUnit réussissent et la sortie standard contient la ligne `Passed!  - Failed: 0, Passed: 65` avec les espaces du CLI. La capture a été faite sous `/tmp/lmr-dotnet-real` ; aucun chemin personnel n'est présent. Le temps affiché (`41 ms`) est celui de cette exécution, pas une garantie de performance.

SHA-256 de `bench/corpus/dotnet_ok.txt` : `883e9bd15055bdd72c46e7d2b63b898e72aa98b6aef00898396dcff316eeb052`.
