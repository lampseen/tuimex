# Homebrew formula template for a tap (`brew install <tap>/tuimex`).
# VERSION and the four SHA256 values come from a release's checksums.txt;
# the tap repository that serves this file does not exist yet.
class Tuimex < Formula
  desc "Terminal mail client that treats mail as hostile input"
  homepage "https://github.com/lampseen/tuimex"
  version "VERSION"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/lampseen/tuimex/releases/download/vVERSION/tuimex_darwin_arm64.tar.gz"
      sha256 "SHA256_DARWIN_ARM64"
    end
    on_intel do
      url "https://github.com/lampseen/tuimex/releases/download/vVERSION/tuimex_darwin_amd64.tar.gz"
      sha256 "SHA256_DARWIN_AMD64"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/lampseen/tuimex/releases/download/vVERSION/tuimex_linux_arm64.tar.gz"
      sha256 "SHA256_LINUX_ARM64"
    end
    on_intel do
      url "https://github.com/lampseen/tuimex/releases/download/vVERSION/tuimex_linux_amd64.tar.gz"
      sha256 "SHA256_LINUX_AMD64"
    end
  end

  def install
    bin.install "tuimex"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/tuimex --version")
  end
end
