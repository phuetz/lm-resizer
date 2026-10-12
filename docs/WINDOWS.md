# Windows : installation et récupération

Ces commandes de release s’appliquent après publication de la version 0.2.6.
Depuis un checkout, le script local `install.ps1` est utilisable dès que
l’archive et son fichier `.sha256` sont disponibles.

## HTTPS et PowerShell 5.1

Si le téléchargement PowerShell échoue avec une erreur Schannel, Node.js peut
télécharger le script avec son propre moteur TLS. Dans PowerShell ou cmd.exe :

```text
node -e "require('https').get('https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.6/install.ps1',{rejectUnauthorized:true},r=>{if(r.statusCode!==200)throw Error('HTTP '+r.statusCode);r.pipe(require('fs').createWriteStream('lm-install-0.2.6.ps1',{flags:'wx'}));}).on('error',e=>{throw e;})"
powershell -NoProfile -ExecutionPolicy Bypass -File .\lm-install-0.2.6.ps1
```

Le fichier doit être nouveau. Le script essaie HTTPS PowerShell avec TLS 1.2,
puis Node si disponible. Le SHA-256 est toujours contrôlé avant extraction et
la version du binaire est vérifiée avant installation. Le calcul du hash et
l’extraction ne nécessitent ni `Get-FileHash` ni le module Archive ; un
`PSModulePath` hérité de PowerShell 7 ne les empêche donc pas.

Autre possibilité : télécharger l’archive Windows et son sidecar `.sha256`
sur une autre machine, puis les placer ensemble dans un dossier local.
Dans PowerShell, avec le script issu du même checkout/tag :

```powershell
$env:LM_RESIZER_RELEASE_BASE_URL = 'C:\Téléchargements\assets vérifiés'
$env:LM_RESIZER_INSTALL_DIR = 'C:\Outils\LM Resizer'
$env:LM_RESIZER_SKIP_PATH_UPDATE = '1'
& .\install.ps1
$env:PATH = "$env:LM_RESIZER_INSTALL_DIR;" + $env:PATH
lm-resizer --version
```

Aucune validation TLS n’est désactivée. Un réseau exigeant un proxy ou une
autorité d’entreprise peut encore demander sa configuration habituelle ; le
repli n’est pas une réparation de Schannel.

## Commandes et octets

`exec` lance un programme, pas les alias du shell. Utiliser explicitement
l’hôte pour un script PowerShell ou une commande interne :

```powershell
lm-resizer exec -- pwsh -NoProfile -File 'C:\projet été\sortie.ps1'
lm-resizer exec -- cmd.exe /d /c 'dir /b'
lm-resizer exec --raw-on-failure -- npm.cmd test
```

Le tee garde les octets d’origine, y compris CP850 et CRLF. La vue texte d’un
flux non UTF-8 signale ses octets invalides par `\xNN` ; elle ne devine pas la
page de code. Pour lire des accents directement, configurer le producteur en
UTF-8. Pour sauvegarder un tee sans le transcodage de redirection de Windows
PowerShell 5.1, utiliser cmd.exe, ou une capture binaire par subprocess :

```text
cmd.exe /d /c "lm-resizer tee read IDENTIFIANT > original.bin"
```

`--raw-on-failure` évite toute compression après un échec ; la capture séparée
stdout/stderr de ce mode reste explicitement signalée. Les hashes CCR affichés
par `compress` désignent maintenant l’entrée avant les transformations.

## Recette reproductible

```text
python bench/real/check_windows_contract.py --binary target/release/lm-resizer.exe --output target/windows-contract.json
powershell -NoProfile -File scripts/test-install-binary.ps1 -ArchiveDir target/release-assets
pwsh -NoProfile -File scripts/test-install-binary.ps1 -ArchiveDir target/release-assets
```

Le premier script indique le système réellement utilisé : un résultat Linux
ne vaut pas une recette Windows. Le second vérifie installation, archive
altérée, version incorrecte et absence de `Get-FileHash` avec un chemin de
modules invalide. Il exerce également le PATH utilisateur et le restaure.
