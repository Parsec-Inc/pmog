# Fórmula do tap Parsec-Inc/homebrew-tap (scripts/package.sh brew troca @VERSION@ e @SHA256@).
class Pmog < Formula
  desc "Jogo de terminal multiplayer na rede local"
  homepage "https://github.com/Parsec-Inc/pmog"
  url "https://github.com/Parsec-Inc/pmog/releases/download/@VERSION@/pmog-@VERSION@-macos-universal.tar.gz"
  version "@VERSION@"
  sha256 "@SHA256@"

  def install
    bin.install "pmog", "pmog.sig"
  end

  test do
    assert_match "PMOG", shell_output("#{bin}/pmog --version")
  end
end
