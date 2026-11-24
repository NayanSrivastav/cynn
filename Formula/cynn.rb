class Cynn < Formula
  desc "Codebase archaeology CLI & agent forensic primitive. Don't just blame it, trace its cynn."
  homepage "https://nayansrivastav.github.io/cynn/"
  version "0.2.3"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/NayanSrivastav/cynn/releases/download/v0.2.3/cynn-v0.2.3-aarch64-apple-darwin.tar.gz"
      sha256 "d04bbd3bcb8a21dc0b7bbac040e7ec3ac08640b98cc36a40878e397bd46d21f4"

      def install
        bin.install "git-cynn" if File.exist?("git-cynn")
        bin.install "cynn" if File.exist?("cynn")
        bin.install_symlink bin/"cynn" => "git-cynn" unless File.exist?(bin/"git-cynn")
      end
    else
      url "https://github.com/NayanSrivastav/cynn/archive/refs/tags/v0.2.3.tar.gz"
      sha256 "c288f28a85d02de05a1b18d1e4860c993981c462b7835ba4285c2f927f357560"
      depends_on "rust" => :build

      def install
        system "cargo", "install", *std_cargo_args
      end
    end
  end

  on_linux do
    url "https://github.com/NayanSrivastav/cynn/archive/refs/tags/v0.2.3.tar.gz"
    sha256 "c288f28a85d02de05a1b18d1e4860c993981c462b7835ba4285c2f927f357560"
    depends_on "rust" => :build

    def install
      system "cargo", "install", *std_cargo_args
    end
  end

  test do
    system "#{bin}/cynn", "--version"
    system "#{bin}/git-cynn", "--version"
  end
end
