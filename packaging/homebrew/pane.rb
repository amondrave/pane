# Homebrew formula for Pane. Lives in the tap repo (amondrave/homebrew-tap) as
# Formula/pane.rb; this copy is the template kept in the main repo.
#
# After each release, update `version`, `url` and `sha256` (the sha is published
# as the .sha256 asset on the GitHub Release).
class Pane < Formula
  desc "Fast native macOS viewer/reviewer for giant files and AI-agent diffs"
  homepage "https://github.com/amondrave/pane"
  version "0.1.0"
  url "https://github.com/amondrave/pane/releases/download/v#{version}/pane-#{version}-macos-universal.tar.gz"
  sha256 "REPLACE_WITH_SHA256_FROM_RELEASE_ASSET"
  license "MIT"

  def install
    bin.install "pane"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/pane --version")
  end
end
