# Placeholder — no v0.1.0 binary release exists yet.
# Keel is in alpha (v0.1.x). Install from source: `cargo install --path .`
# This formula will be filled in (URLs + sha256) when the first binary release ships.
class Keel < Formula
  desc "A programming language where AI agents are first-class citizens (alpha)"
  homepage "https://keel-lang.dev"
  license "MIT"
  version "0.1.0"

  on_macos do
    url "https://github.com/keel-lang/keel/releases/download/v0.1.0/keel-aarch64-apple-darwin.tar.gz"
    # sha256 "UPDATE_AFTER_RELEASE"
  end

  on_linux do
    url "https://github.com/keel-lang/keel/releases/download/v0.1.0/keel-x86_64-unknown-linux-gnu.tar.gz"
    # sha256 "UPDATE_AFTER_RELEASE"
  end

  def install
    bin.install "keel"
  end

  test do
    system "#{bin}/keel", "--version"
  end
end
