# frozen_string_literal: true

class Codeagent < Formula
  desc "High-performance wrapper for AI CLI backends (Claude, Codex, Gemini, Opencode)"
  homepage "https://github.com/user/codeagent-wrapper"
  version "1.0.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/user/codeagent-wrapper/releases/download/v1.0.0/codeagent-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_ARM64_MACOS"
    else
      url "https://github.com/user/codeagent-wrapper/releases/download/v1.0.0/codeagent-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_X64_MACOS"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/user/codeagent-wrapper/releases/download/v1.0.0/codeagent-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_ARM64_LINUX"
    else
      url "https://github.com/user/codeagent-wrapper/releases/download/v1.0.0/codeagent-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_X64_LINUX"
    end
  end

  def install
    bin.install "codeagent"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/codeagent --version")
  end
end
