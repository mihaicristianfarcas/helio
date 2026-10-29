# Written by tools/update-casks for each release: change that script, not
# this file.
cask "helio" do
  arch arm: "arm64", intel: "x86_64"

  version "0.9.7"
  sha256 arm:   "4c5e854bdc66aabc8f17884d590b336a97766edae82ba02807330ca7b447a970",
         intel: "baf03210493f2d28bd906cf1e263b38a1f621a4b594c65f851b0af022e0f95f1"

  url "https://github.com/mihaicristianfarcas/helio/releases/download/v#{version}/Helio-#{version}-macos-#{arch}.zip"
  name "Helio"
  desc "Desktop and game streaming, client and host, post-quantum encrypted"
  homepage "https://github.com/mihaicristianfarcas/helio"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: :sonoma

  app "Helio.app"

  zap trash: [
    "~/Library/Application Support/Helio",
    "~/Library/Logs/Helio",
  ]
end
