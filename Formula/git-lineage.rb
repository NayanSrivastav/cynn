class GitLineage < Formula
  desc "The Codebase Archaeology CLI and Agent Forensic Primitive"
  homepage "https://nayansrivastav.github.io/git-lineage/"
  version "0.2.1"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/NayanSrivastav/git-lineage/releases/download/v0.2.1/git-lineage-v0.2.1-aarch64-apple-darwin.tar.gz"
      sha256 "d04bbd3bcb8a21dc0b7bbac040e7ec3ac08640b98cc36a40878e397bd46d21f4"

      def install
        bin.install "git-lineage"
      end
    else
      url "https://github.com/NayanSrivastav/git-lineage/archive/refs/tags/v0.2.1.tar.gz"
      sha256 "c288f28a85d02de05a1b18d1e4860c993981c462b7835ba4285c2f927f357560"
      depends_on "rust" => :build

      def install
        system "cargo", "install", *std_cargo_args
      end
    end
  end

  on_linux do
    url "https://github.com/NayanSrivastav/git-lineage/archive/refs/tags/v0.2.1.tar.gz"
    sha256 "c288f28a85d02de05a1b18d1e4860c993981c462b7835ba4285c2f927f357560"
    depends_on "rust" => :build

    def install
      system "cargo", "install", *std_cargo_args
    end
  end

  test do
    system "#{bin}/git-lineage", "--version"
  end
end
