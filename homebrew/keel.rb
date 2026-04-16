class Keel < Formula
  desc "A programming language where AI agents are first-class citizens"
  homepage "https://keel-lang.dev"
  license "MIT"
  version "0.9.0"

  on_macos do
    on_arm do
      url "https://github.com/keel-lang/keel/releases/download/v0.9.0/keel-aarch64-apple-darwin.tar.gz"
      # sha256 "UPDATE_AFTER_RELEASE"
    end
    on_intel do
      url "https://github.com/keel-lang/keel/releases/download/v0.9.0/keel-x86_64-apple-darwin.tar.gz"
      # sha256 "UPDATE_AFTER_RELEASE"
    end
  end

  on_linux do
    url "https://github.com/keel-lang/keel/releases/download/v0.9.0/keel-x86_64-unknown-linux-gnu.tar.gz"
    # sha256 "UPDATE_AFTER_RELEASE"
  end

  def install
    bin.install "keel"
  end

  test do
    system "#{bin}/keel", "--version"
  end
end
