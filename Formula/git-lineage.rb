class GitLineage < Formula
  desc "The Codebase Archaeology CLI and Agent Forensic Primitive"
  homepage "https://nayansrivastav.github.io/git-lineage/"
  version "0.2.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/NayanSrivastav/git-lineage/releases/download/v0.2.0/git-lineage-v0.2.0-aarch64-apple-darwin.tar.gz"
      # sha256 "<checksum_will_be_populated_on_asset_upload>"
    else
      url "https://github.com/NayanSrivastav/git-lineage/releases/download/v0.2.0/git-lineage-v0.2.0-x86_64-apple-darwin.tar.gz"
      # sha256 "<checksum_will_be_populated_on_asset_upload>"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/NayanSrivastav/git-lineage/releases/download/v0.2.0/git-lineage-v0.2.0-aarch64-unknown-linux-musl.tar.gz"
      # sha256 "<checksum_will_be_populated_on_asset_upload>"
    else
      url "https://github.com/NayanSrivastav/git-lineage/releases/download/v0.2.0/git-lineage-v0.2.0-x86_64-unknown-linux-musl.tar.gz"
      # sha256 "<checksum_will_be_populated_on_asset_upload>"
    end
  end

  def install
    bin.install "git-lineage"
  end

  test do
    system "#{bin}/git-lineage", "--version"
  end
end
