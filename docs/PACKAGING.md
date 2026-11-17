# Empacotamento e releases

As builds saem de `scripts/build-dist.sh` (Linux e Windows, assinadas) e os pacotes de
`scripts/package.sh`, os mesmos scripts no seu computador e no CI.

## Como sai uma release

Todo push na `main` roda `.github/workflows/release.yml`:

1. **tests**: clippy (Linux e Windows) e `cargo test`. Nos PRs, também clippy no macOS.
2. **bump**: o commitizen calcula a versão pelos commits, atualiza `Cargo.toml`, `Cargo.lock` e
   `CHANGELOG.md`, cria a tag e dá push. Também fixa o `PMOG_RELEASE` (número do auto-update,
   `AAAAMMDDHHMM` em UTC) que todas as builds usam.
3. **linux**: `build-dist.sh` com a chave do secret `PMOG_RELEASE_KEY` gera `pmog-linux` e
   `pmog-windows.exe` com os `.sig`; depois `.deb`, `.rpm`, `.src.rpm` do COPR e PKGBUILD do AUR.
4. **macos**: binário universal (Apple Silicon + Intel) no runner do Mac, sem a chave.
5. **macos-sign**: assina o `pmog-macos` no Linux e monta o `.tar.gz` e a fórmula do Homebrew.
6. **msi**: no runner Windows, empacota o mesmo `.exe` assinado com o WiX 3.
7. **release**: junta tudo, gera `SHA256SUMS` e cria a release. macOS e MSI podem falhar sem
   travar a release; o job linux é obrigatório porque o auto-update baixa as builds dele.
8. **aur**, **copr** e **homebrew**: só rodam com as variáveis ligadas (abaixo).

Os nomes `pmog-linux`, `pmog-windows.exe`, `pmog-macos` e os `.sig` não levam versão: é o que o
auto-update procura na release.

## Build local

| Comando | Gera (em `dist/`) | Precisa de |
|---|---|---|
| `scripts/build-dist.sh` | `pmog-linux`, `pmog-windows.exe` e `.sig` | chave em `~/.config/pmog-release`, musl e mingw |
| `scripts/package.sh deb` | `.deb` | `cargo install cargo-deb` |
| `scripts/package.sh rpm` | `.rpm` e `.src.rpm` (o que vai para o COPR) | `rpm-build` |
| `scripts/package.sh aur` | `dist/aur/PKGBUILD` | |
| `scripts/package.sh msi` | `.msi` com exceção no firewall | WiX 3 (só Windows) |
| `scripts/package.sh macos` | `pmog-macos` universal | **só num Mac** |
| `scripts/package.sh brew` | `.tar.gz` do macOS e `dist/brew/pmog.rb` | `pmog-macos` |

## Permissões que os pacotes configuram

| Onde | O quê |
|---|---|
| Linux | serviço `pmog` do firewalld (`/usr/lib/firewalld/services/pmog.xml`) e perfil do ufw (`/etc/ufw/applications.d/pmog`): UDP 47777, TCP 47790-47799 (jogo) e 47800-47809 (celular) |
| Linux | ao instalar, `/usr/share/pmog/firewall open` libera no firewall ativo; ao remover, `close` fecha |
| Windows | MSI para a máquina em `Program Files\pmog`, `PATH` do sistema, atalho no menu Iniciar e exceção do Firewall do Windows para o `pmog.exe` |
| macOS | nada: o firewall do macOS pergunta na primeira vez |

Todos os pacotes levam o `.sig` ao lado do executável (`/usr/bin/pmog.sig`, `pmog.exe.sig`,
`pmog.sig` no Homebrew): é o que deixa uma instalação por pacote servir update na LAN.

## Atualização

Os dois caminhos convivem: o gerenciador de pacotes (quando o canal estiver ligado) e o
auto-update do próprio jogo (LAN ou GitHub, sempre com `.sig`). Instalado por pacote, o
executável fica numa pasta do sistema e o auto-update pede a senha (pkexec ou UAC) para trocar.

## Canais (configuração única)

Nada disso está configurado ainda; cada canal fica desligado até a variável virar `true`
(*Settings > Secrets and variables > Actions*).

| Canal | Para o jogador | O que configurar |
|---|---|---|
| **GitHub Releases** | baixar `.rpm` / `.deb` / `.msi` / `.tar.gz` / binário | nada (o secret `PMOG_RELEASE_KEY` já existe) |
| **AUR** (`pmog-bin`) | `yay -S pmog-bin` | Conta no AUR com chave SSH; secret `AUR_SSH_PRIVATE_KEY`, variável `PUBLISH_AUR=true`. |
| **COPR** (dnf) | `sudo dnf copr enable <dono>/pmog && sudo dnf install pmog` | Projeto no copr.fedorainfracloud.org; o conteúdo de *API* vai no secret `COPR_CONFIG`; variáveis `COPR_PROJECT=<dono>/pmog` e `PUBLISH_COPR=true`. |
| **Homebrew** | `brew install Parsec-Inc/tap/pmog` | Repo público `Parsec-Inc/homebrew-tap`; secret `HOMEBREW_TAP_TOKEN` (PAT com escrita nele); variável `PUBLISH_BREW=true`. |

AUR, COPR e Homebrew usam o binário da release (não compilam de novo), então ficam na mesma
versão da release do GitHub.

## O que não foi testado

- `.deb`, `.rpm` e o pacote do AUR foram instalados e removidos em containers (Debian, Fedora,
  Arch); o serviço do firewalld passou no `firewall-offline-cmd`. A abertura de portas com
  firewalld/ufw rodando de verdade não foi exercitada.
- O MSI, o build do macOS e a fórmula do Homebrew só rodam no CI; o primeiro release é o primeiro teste.
